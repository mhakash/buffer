//! One task's details: properties and notes — the same block editor and file list as the Notes
//! page (checklists, links and log lines live in the notes; `/todo`, `/log`). Owned by `Workspace`, which shows it as a resizable side panel or as a full page;
//! opened via `OpenTask` events from pages.

use std::time::Duration;

use chrono::{DateTime, Local, Utc};
use gpui_kit::component::{
    ActiveTheme, Icon, Sizable, Theme,
    button::{Button, ButtonRounded, ButtonVariants},
    calendar::Date,
    checkbox::Checkbox,
    date_picker::{DatePicker, DatePickerEvent, DatePickerState},
    h_flex,
    input::{Escape, Input, InputEvent, InputState, Textarea, TextareaState},
    menu::{DropdownMenu, PopupMenuItem},
    v_flex,
};
use gpui_kit::{
    Animation, AnimationExt, AnyElement, Context, Entity, EventEmitter, FontWeight, Hsla,
    Subscription, Window, assets::IconName, div, ease_out_quint, prelude::*, px, rgb,
};

use super::tasks::{due_label, priority_menu, status_color, status_menu};
use crate::{
    components::{DragScroll, Outline, category_tag, file_list, icon_button, property_row},
    editor::{BlockEditor, BlockEditorEvent},
    tasks::{Id, Status, Task, TaskStore},
};

/// Side-panel width bounds; the user drags the left edge between them.
pub const PANEL_MIN_W: f32 = 360.;
pub const PANEL_DEFAULT_W: f32 = 440.;
/// Reading width of the full-page view.
const PAGE_MAX_W: f32 = 820.;

/// Emitted by any page that wants a task opened in the panel.
pub struct OpenTask(pub Id);

pub enum DetailEvent {
    Close,
    /// Switched between side panel and full page.
    Layout,
}

/// Drag payload of the panel's resize edge; `Workspace` tracks the move.
#[derive(Clone)]
pub struct ResizePanel;

struct ResizeGhost;

impl Render for ResizeGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl EventEmitter<DetailEvent> for TaskDetail {}

pub struct TaskDetail {
    store: Entity<TaskStore>,
    task: Option<Id>,
    /// Full page instead of the side panel.
    full_page: bool,
    /// Notes show their Markdown source instead of the block editor.
    source_notes: bool,
    blocks: Entity<BlockEditor>,
    title: Entity<InputState>,
    notes: Entity<TextareaState>,
    due: Entity<DatePickerState>,
    /// The body follows a selection dragged past its edges.
    drag_scroll: DragScroll,
    /// Heading outline at the top right of the body.
    outline: Outline,
    _subscriptions: Vec<Subscription>,
}

impl TaskDetail {
    pub fn new(store: Entity<TaskStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Task title"));
        let notes = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(8, 400)
                .placeholder("Markdown: # headings, **bold**, - [ ] checklists, `code`, links…")
        });
        let blocks = cx.new(|cx| {
            let mut editor = BlockEditor::new(window, cx);
            editor.set_compact(true, cx);
            // Files added to the notes go in `tasks/<id>/files/`.
            editor.set_files(Some("files".into()));
            editor
        });
        let due = cx.new(|cx| DatePickerState::new(window, cx).date_format("%a, %b %-d"));

        let subscriptions = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            cx.subscribe_in(&title, window, |this, input, event: &InputEvent, _, cx| {
                if let (InputEvent::Change, Some(id)) = (event, this.task) {
                    let value = input.read(cx).value();
                    this.store
                        .update(cx, |store, cx| store.set_title(id, &value, cx));
                }
            }),
            cx.subscribe_in(&notes, window, |this, input, event: &InputEvent, _, cx| {
                if let (InputEvent::Change, Some(id), true) = (event, this.task, this.source_notes)
                {
                    let value = input.read(cx).value();
                    this.store
                        .update(cx, |store, cx| store.set_notes(id, &value, cx));
                }
            }),
            cx.subscribe_in(
                &blocks,
                window,
                |this, _, event: &BlockEditorEvent, _, cx| {
                    if let (BlockEditorEvent::Changed(notes), Some(id)) = (event, this.task) {
                        this.store
                            .update(cx, |store, cx| store.set_notes(id, notes, cx));
                    }
                },
            ),
            cx.subscribe_in(&due, window, |this, _, event: &DatePickerEvent, _, cx| {
                let (DatePickerEvent::Change(value), Some(id)) = (event, this.task) else {
                    return;
                };
                let date = value.start().map(|d| d.date());
                this.store
                    .update(cx, |store, cx| store.set_due(id, date, cx));
            }),
        ];

        Self {
            store,
            task: None,
            full_page: false,
            source_notes: false,
            blocks,
            title,
            notes,
            due,
            drag_scroll: DragScroll::default(),
            outline: Outline::default(),
            _subscriptions: subscriptions,
        }
    }

    pub fn task_id(&self) -> Option<Id> {
        self.task
    }

    pub fn is_full_page(&self) -> bool {
        self.full_page
    }

    pub fn open(&mut self, id: Id, window: &mut Window, cx: &mut Context<Self>) {
        let Some(task) = self.store.read(cx).task(id).cloned() else {
            return;
        };
        self.task = Some(id);
        self.source_notes = false;
        self.blocks
            .update(cx, |b, cx| b.load(&task.notes, TaskStore::task_dir(id), cx));
        self.title
            .update(cx, |i, cx| i.set_value(task.title, window, cx));
        self.due.update(cx, |d, cx| {
            d.set_date(task.due.map_or(Date::Single(None), Date::from), window, cx)
        });
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.task = None;
        self.source_notes = false;
        cx.emit(DetailEvent::Close);
        cx.notify();
    }

    pub fn set_full_page(&mut self, full_page: bool, cx: &mut Context<Self>) {
        if self.full_page != full_page {
            self.full_page = full_page;
            self.blocks
                .update(cx, |b, cx| b.set_compact(!full_page, cx));
            cx.emit(DetailEvent::Layout);
            cx.notify();
        }
    }

    /// Esc leaves the notes source first, then the full page, then closes.
    fn escape(&mut self, cx: &mut Context<Self>) {
        if self.source_notes {
            self.show_blocks(cx);
        } else if self.full_page {
            self.set_full_page(false, cx);
        } else {
            self.close(cx);
        }
    }

    /// Back from the source to the block editor.
    fn show_blocks(&mut self, cx: &mut Context<Self>) {
        self.source_notes = false;
        if let Some(task) = self
            .task
            .and_then(|id| self.store.read(cx).task(id).cloned())
        {
            self.blocks.update(cx, |b, cx| {
                b.load(&task.notes, TaskStore::task_dir(task.id), cx)
            });
        }
        cx.notify();
    }

    fn edit_notes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(task) = self
            .task
            .and_then(|id| self.store.read(cx).task(id).cloned())
        else {
            return;
        };
        // The store is the source of truth while the block editor shows.
        self.source_notes = true;
        self.notes.update(cx, |n, cx| {
            n.set_value(task.notes, window, cx);
            n.focus(window, cx);
        });
        cx.notify();
    }

    fn render_toolbar(
        &self,
        task: &Task,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = task.id;
        let full = self.full_page;
        h_flex()
            .gap_1()
            .px_3()
            .pt_3()
            .when(full, |this| {
                this.child(
                    Button::new("detail-back")
                        .ghost()
                        .small()
                        .rounded(ButtonRounded::Size(px(999.)))
                        .icon(Icon::new(IconName::ArrowLeft).text_color(theme.muted_foreground))
                        .label("Tasks")
                        .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                )
            })
            .child(div().flex_1())
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .mr_1()
                    .child(match task.updated_at {
                        Some(ts) => format!("Edited {}", when(ts)),
                        None => format!("Created {}", when(task.created_at)),
                    }),
            )
            .child(
                icon_button(
                    "detail-folder",
                    IconName::FolderOpen,
                    theme.muted_foreground,
                    "Show task folder in Finder",
                )
                .on_click(move |_, _, cx| {
                    let dir = TaskStore::task_dir(id);
                    let target = if dir.join("notes.md").exists() {
                        dir.join("notes.md")
                    } else {
                        dir.join("task.json")
                    };
                    cx.reveal_path(&target);
                }),
            )
            .child(
                icon_button(
                    "detail-layout",
                    if full {
                        IconName::PanelRightOpen
                    } else {
                        IconName::Maximize2
                    },
                    theme.muted_foreground,
                    if full {
                        "Open as side panel"
                    } else {
                        "Open as full page"
                    },
                )
                .on_click(cx.listener(move |this, _, _, cx| this.set_full_page(!full, cx))),
            )
            .child(
                icon_button(
                    "detail-close",
                    IconName::X,
                    theme.muted_foreground,
                    "Close (Esc)",
                )
                .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            )
    }

    fn render_title(&self, task: &Task, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let id = task.id;
        h_flex()
            .gap_3()
            .px(px(self.gutter()))
            .pt_2()
            .child(
                Checkbox::new("detail-done")
                    .checked(task.is_done())
                    .when(self.full_page, |c| c.large())
                    .on_click(cx.listener(move |this, _: &bool, _, cx| {
                        this.store.update(cx, |s, cx| s.toggle_done(id, cx));
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .font_weight(FontWeight::BOLD)
                    .when(self.full_page, |d| d.text_3xl())
                    .when(!self.full_page, |d| d.text_xl())
                    .when(task.is_done(), |d| d.text_color(theme.muted_foreground))
                    .child(Input::new(&self.title).appearance(false)),
            )
    }

    fn render_properties(
        &self,
        task: &Task,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = task.id;
        let priority = task.priority;
        let color: Hsla = rgb(priority.color()).into();
        let assigned: Vec<_> = {
            let all = self.store.read(cx).categories();
            task.categories
                .iter()
                .filter_map(|cid| all.iter().find(|c| c.id == *cid).cloned())
                .collect()
        };
        let today = Local::now().date_naive();

        let status = task.status();
        let status_since = match status {
            Status::Done => task.completed_at.map(when),
            Status::FollowUp => task.follow_up_at.map(|ts| format!("since {}", when(ts))),
            Status::Todo => None,
        };
        let status_pill = {
            let store = self.store.clone();
            select_pill("detail-status", status.label(), status_color(status, theme))
                .dropdown_menu(move |menu, _, _| status_menu(menu, &store, id, status))
        };

        let priority_pill = {
            let store = self.store.clone();
            select_pill("detail-priority", priority.label(), color)
                .dropdown_menu(move |menu, _, _| priority_menu(menu, &store, id, priority))
        };

        let store = self.store.clone();
        let picker = Button::new("detail-categories")
            .ghost()
            .xsmall()
            .rounded(ButtonRounded::Size(px(999.)))
            .icon(Icon::new(IconName::Plus).text_color(theme.muted_foreground))
            .when(assigned.is_empty(), |b| b.label("Add"))
            .tooltip("Categories")
            .dropdown_menu(move |mut menu, _, cx| {
                let store_ref = store.read(cx);
                let current = store_ref
                    .task(id)
                    .map(|t| t.categories.clone())
                    .unwrap_or_default();
                let all = store_ref.categories().to_vec();
                if all.is_empty() {
                    return menu.label("No categories yet — add some in Settings");
                }
                for category in all {
                    let cid = category.id;
                    let store = store.clone();
                    menu = menu.item(
                        PopupMenuItem::new(category.name)
                            .checked(current.contains(&cid))
                            .on_click(move |_, _, cx| {
                                store.update(cx, |s, cx| s.toggle_category(id, cid, cx));
                            }),
                    );
                }
                menu.min_w(px(180.))
            });

        let overdue = task.is_overdue(today);
        v_flex()
            .px(px(self.gutter()))
            .py_2()
            .child(
                property_row(IconName::CircleDot, "Status", theme)
                    .child(status_pill)
                    .when_some(status_since, |this, since| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(since),
                        )
                    }),
            )
            .child(property_row(IconName::Flag, "Priority", theme).child(priority_pill))
            .child(
                property_row(IconName::Tags, "Categories", theme).child(
                    h_flex()
                        .flex_wrap()
                        .gap_1()
                        .children(
                            assigned
                                .iter()
                                .map(|c| category_tag(c.name.clone(), rgb(c.color).into())),
                        )
                        .child(picker),
                ),
            )
            .child(
                property_row(IconName::Calendar, "Due", theme)
                    .child(
                        div().w(px(190.)).child(
                            DatePicker::new(&self.due)
                                .small()
                                .appearance(false)
                                .cleanable(true)
                                .placeholder("No date"),
                        ),
                    )
                    .when_some(task.due.filter(|_| !task.is_done()), |this, due| {
                        let color = if overdue {
                            theme.danger
                        } else if due == today {
                            theme.warning
                        } else {
                            theme.muted_foreground
                        };
                        this.child(div().text_xs().text_color(color).child(if overdue {
                            format!("Overdue · {}", due_label(due, today))
                        } else {
                            due_label(due, today)
                        }))
                    }),
            )
            .child(
                property_row(IconName::Clock, "Created", theme)
                    .child(div().text_sm().child(when(task.created_at))),
            )
    }

    fn render_notes(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let source = self.source_notes;
        let toggle = Button::new("notes-mode")
            .ghost()
            .xsmall()
            .rounded(ButtonRounded::Size(px(999.)))
            .icon(
                Icon::new(if source {
                    IconName::Type
                } else {
                    IconName::SquareCode
                })
                .text_color(theme.muted_foreground),
            )
            .label(if source { "Rich text" } else { "Markdown" })
            .tooltip(if source {
                "Back to the block editor"
            } else {
                "Edit the Markdown source"
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.source_notes {
                    this.show_blocks(cx);
                } else {
                    this.edit_notes(window, cx);
                }
            }));

        let body = if source {
            div()
                .px_3()
                .py_2()
                .rounded_xl()
                .bg(theme.secondary)
                .border_1()
                .border_color(theme.primary.opacity(0.35))
                .text_sm()
                .font_family(theme.mono_font_family.clone())
                .child(Textarea::new(&self.notes).appearance(false))
                .into_any_element()
        } else {
            // Same editor as the Notes page.
            self.outline
                .wrap_editor(self.blocks.clone())
                .into_any_element()
        };

        Self::section(IconName::NotebookPen, "Notes", None, theme)
            .child(h_flex().absolute().top(px(-1.)).right_0().child(toggle))
            .relative()
            .child(body)
            .into_any_element()
    }

    /// Horizontal padding of the content.
    fn gutter(&self) -> f32 {
        if self.full_page { 48. } else { 20. }
    }

    fn section(
        icon: IconName,
        title: &'static str,
        trailing: Option<String>,
        theme: &Theme,
    ) -> gpui_kit::Div {
        v_flex().gap_2().child(
            h_flex()
                .gap_2()
                .text_xs()
                .font_weight(FontWeight::BOLD)
                .text_color(theme.muted_foreground)
                .child(Icon::new(icon).xsmall().text_color(theme.muted_foreground))
                .child(title.to_uppercase())
                .when_some(trailing, |this, t| {
                    this.child(div().ml_auto().font_weight(FontWeight::NORMAL).child(t))
                }),
        )
    }
}

impl Render for TaskDetail {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let Some(task) = self
            .task
            .and_then(|id| self.store.read(cx).task(id).cloned())
        else {
            return div().into_any_element();
        };
        let _ = window;
        let full = self.full_page;
        let gutter = self.gutter();
        let outline = if self.source_notes {
            None
        } else {
            self.outline.render(
                "task-outline",
                task.title.clone().into(),
                self.blocks.read(cx).headings(),
                &self.drag_scroll.handle(),
                &theme,
            )
        };

        let content = v_flex()
            .relative()
            .w_full()
            // The side panel's padding is too narrow for the outline beside the text.
            .when(outline.is_some() && !full, |this| this.pr(px(20.)))
            .child(self.outline.probe_content())
            .when(full, |this| this.max_w(px(PAGE_MAX_W)).mx_auto())
            .child(self.render_title(&task, &theme, cx))
            .child(self.render_properties(&task, &theme, cx))
            .child(div().mx(px(gutter)).h(px(1.)).bg(theme.border))
            .child(
                v_flex()
                    .px(px(gutter))
                    .pt_5()
                    .pb_10()
                    .gap_7()
                    .child(self.render_notes(&theme, cx))
                    .children(file_list(
                        "task-file",
                        &task.notes,
                        &TaskStore::task_dir(task.id),
                        &theme,
                    )),
            );

        let panel = v_flex()
            .relative()
            .flex_none()
            .h_full()
            .w_full()
            .rounded_2xl()
            .bg(theme.group_box)
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .on_action(cx.listener(|this, _: &Escape, _, cx| this.escape(cx)))
            .child(self.render_toolbar(&task, &theme, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        self.drag_scroll
                            .attach(div().id("detail-body").size_full().overflow_y_scroll())
                            .child(content),
                    )
                    .children(outline),
            )
            // Drag the left edge to resize the side panel.
            .when(!full, |this| {
                this.child(
                    div()
                        .id("detail-resize")
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(6.))
                        .cursor_col_resize()
                        .hover(|s| s.bg(theme.primary.opacity(0.35)))
                        .on_drag(ResizePanel, |_, _, _, cx| cx.new(|_| ResizeGhost)),
                )
            });

        panel
            .with_animation(
                if full { "detail-page" } else { "detail-panel" },
                Animation::new(Duration::from_millis(320)).with_easing(ease_out_quint()),
                move |el, t| {
                    if full {
                        el.opacity(t).relative().top(px((1. - t) * 16.))
                    } else {
                        el.opacity(t).relative().left(px((1. - t) * 24.))
                    }
                },
            )
            .into_any_element()
    }
}

/// `today 14:05`, `yesterday 14:05`, `Nov 3, 14:05`.
/// Tinted pill showing a property's current value; opens its dropdown.
fn select_pill(id: &'static str, label: &'static str, color: Hsla) -> Button {
    Button::new(id)
        .ghost()
        .xsmall()
        .rounded(ButtonRounded::Size(px(999.)))
        .child(
            h_flex()
                .gap_1p5()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color)
                .child(div().size_1p5().rounded_full().bg(color))
                .child(label)
                .child(
                    Icon::new(IconName::ChevronDown)
                        .xsmall()
                        .text_color(color.opacity(0.7)),
                ),
        )
}

pub fn when(ts: DateTime<Utc>) -> String {
    let local = ts.with_timezone(&Local);
    let today = Local::now().date_naive();
    match (today - local.date_naive()).num_days() {
        0 => local.format("today %H:%M").to_string(),
        1 => local.format("yesterday %H:%M").to_string(),
        _ => local.format("%b %-d, %H:%M").to_string(),
    }
}
