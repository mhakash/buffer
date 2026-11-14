//! One task's details: properties, Markdown notes, subtasks, links (Jira/GitLab aware) and a
//! work log. Owned by `Workspace`, which shows it as a resizable side panel or as a full page;
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
    text::{TextView, TextViewStyle},
    v_flex,
};
use gpui_kit::{
    Animation, AnimationExt, AnyElement, Context, Entity, EventEmitter, FontWeight, Hsla,
    SharedString, Subscription, Window, assets::IconName, div, ease_out_quint, prelude::*, px, rgb,
};

use super::tasks::due_label;
use crate::{
    components::{category_tag, confirm::confirm_delete, fx, icon_button},
    tasks::{Id, LinkKind, Task, TaskStore, classify, is_url},
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
    /// Notes show rendered Markdown unless this is set.
    editing_notes: bool,
    title: Entity<InputState>,
    notes: Entity<TextareaState>,
    due: Entity<DatePickerState>,
    subtask_input: Entity<InputState>,
    link_input: Entity<InputState>,
    log_input: Entity<InputState>,
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
        let due = cx.new(|cx| DatePickerState::new(window, cx).date_format("%a, %b %-d"));
        let subtask_input = cx.new(|cx| InputState::new(window, cx).placeholder("Add a subtask"));
        let link_input = cx
            .new(|cx| InputState::new(window, cx).placeholder("Paste a Jira, GitLab or any link"));
        let log_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("What did you do? Enter to log"));

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
                if let (InputEvent::Change, Some(id), true) = (event, this.task, this.editing_notes)
                {
                    let value = input.read(cx).value();
                    this.store
                        .update(cx, |store, cx| store.set_notes(id, &value, cx));
                }
            }),
            cx.subscribe_in(&due, window, |this, _, event: &DatePickerEvent, _, cx| {
                let (DatePickerEvent::Change(value), Some(id)) = (event, this.task) else {
                    return;
                };
                let date = value.start().map(|d| d.date());
                this.store
                    .update(cx, |store, cx| store.set_due(id, date, cx));
            }),
            Self::on_enter(&subtask_input, window, cx, |store, id, text, cx| {
                store.add_subtask(id, text, cx)
            }),
            Self::on_enter(&link_input, window, cx, |store, id, text, cx| {
                for word in text.split_whitespace().filter(|w| is_url(w)) {
                    store.add_link(id, word, cx);
                }
            }),
            Self::on_enter(&log_input, window, cx, |store, id, text, cx| {
                store.add_log(id, text, cx)
            }),
        ];

        Self {
            store,
            task: None,
            full_page: false,
            editing_notes: false,
            title,
            notes,
            due,
            subtask_input,
            link_input,
            log_input,
            _subscriptions: subscriptions,
        }
    }

    /// Enter in `input` runs `apply` on the open task, then clears the input.
    fn on_enter(
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
        apply: impl Fn(&mut TaskStore, Id, &str, &mut Context<TaskStore>) + 'static,
    ) -> Subscription {
        cx.subscribe_in(
            input,
            window,
            move |this, input, event: &InputEvent, window, cx| {
                let (InputEvent::PressEnter { .. }, Some(id)) = (event, this.task) else {
                    return;
                };
                let text = input.read(cx).value().to_string();
                if text.trim().is_empty() {
                    return;
                }
                this.store
                    .update(cx, |store, cx| apply(store, id, &text, cx));
                input.update(cx, |input, cx| input.set_value("", window, cx));
            },
        )
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
        self.editing_notes = false;
        self.title
            .update(cx, |i, cx| i.set_value(task.title, window, cx));
        self.due.update(cx, |d, cx| {
            d.set_date(task.due.map_or(Date::Single(None), Date::from), window, cx)
        });
        for input in [&self.subtask_input, &self.link_input, &self.log_input] {
            input.update(cx, |i, cx| i.set_value("", window, cx));
        }
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.task = None;
        self.editing_notes = false;
        cx.emit(DetailEvent::Close);
        cx.notify();
    }

    pub fn set_full_page(&mut self, full_page: bool, cx: &mut Context<Self>) {
        if self.full_page != full_page {
            self.full_page = full_page;
            cx.emit(DetailEvent::Layout);
            cx.notify();
        }
    }

    /// Esc leaves the notes editor first, then the full page, then closes.
    fn escape(&mut self, cx: &mut Context<Self>) {
        if self.editing_notes {
            self.editing_notes = false;
            cx.notify();
        } else if self.full_page {
            self.set_full_page(false, cx);
        } else {
            self.close(cx);
        }
    }

    fn edit_notes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(task) = self
            .task
            .and_then(|id| self.store.read(cx).task(id).cloned())
        else {
            return;
        };
        // The store is the source of truth while previewing (checkbox clicks edit it there).
        self.editing_notes = true;
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

    fn property(icon: IconName, label: &'static str, theme: &Theme) -> gpui_kit::Div {
        h_flex().min_h(px(32.)).gap_2().child(
            h_flex()
                .flex_none()
                .w(px(112.))
                .gap_2()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(Icon::new(icon).small().text_color(theme.muted_foreground))
                .child(label),
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
        let next = priority.cycle();
        let assigned: Vec<_> = {
            let all = self.store.read(cx).categories();
            task.categories
                .iter()
                .filter_map(|cid| all.iter().find(|c| c.id == *cid).cloned())
                .collect()
        };
        let today = Local::now().date_naive();

        let status = match task.completed_at {
            Some(ts) => h_flex()
                .gap_1p5()
                .text_sm()
                .text_color(theme.success)
                .child(div().size_2().rounded_full().bg(theme.success))
                .child(format!("Done {}", when(ts))),
            None => h_flex()
                .gap_1p5()
                .text_sm()
                .child(div().size_2().rounded_full().bg(theme.primary))
                .child("Open"),
        };

        let priority_pill = h_flex()
            .id("detail-priority")
            .gap_1p5()
            .px_2()
            .py_0p5()
            .rounded_full()
            .cursor_pointer()
            .bg(color.opacity(0.14))
            .hover(|s| s.bg(color.opacity(0.24)))
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(color)
            .child(div().size_1p5().rounded_full().bg(color))
            .child(priority.label())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.store.update(cx, |s, cx| s.set_priority(id, next, cx));
            }));

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
            .child(Self::property(IconName::CircleDot, "Status", theme).child(status))
            .child(Self::property(IconName::Flag, "Priority", theme).child(priority_pill))
            .child(
                Self::property(IconName::Tags, "Categories", theme).child(
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
                Self::property(IconName::Calendar, "Due", theme)
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
                Self::property(IconName::Clock, "Created", theme)
                    .child(div().text_sm().child(when(task.created_at))),
            )
    }

    fn render_notes(&self, task: &Task, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let id = task.id;
        let editing = self.editing_notes;
        let toggle = Button::new("notes-mode")
            .ghost()
            .xsmall()
            .rounded(ButtonRounded::Size(px(999.)))
            .icon(
                Icon::new(if editing {
                    IconName::Eye
                } else {
                    IconName::SquarePen
                })
                .text_color(theme.muted_foreground),
            )
            .label(if editing { "Preview" } else { "Edit" })
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.editing_notes {
                    this.editing_notes = false;
                    cx.notify();
                } else {
                    this.edit_notes(window, cx);
                }
            }));

        let body = if editing {
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
        } else if task.notes.trim().is_empty() {
            // Block, not a flex row: plain text in a flex row doesn't wrap and overflows.
            div()
                .id("notes-empty")
                .w_full()
                .px_3()
                .py_3()
                .rounded_xl()
                .cursor_text()
                .border_1()
                .border_dashed()
                .border_color(theme.border)
                .text_sm()
                .text_color(theme.muted_foreground)
                .hover(|s| s.bg(theme.secondary))
                .child("Write notes in Markdown — context, decisions, checklists, snippets…")
                .on_click(cx.listener(|this, _, window, cx| this.edit_notes(window, cx)))
                .into_any_element()
        } else {
            let store = self.store.clone();
            div()
                .id("notes-preview")
                .text_sm()
                .child(
                    TextView::markdown(("notes-md", id), task.notes.clone())
                        .style(notes_style())
                        .selectable(true)
                        .on_task_toggle(move |event, _, cx| {
                            store.update(cx, |s, cx| {
                                s.set_note_checkbox(id, event.marker_range(), event.checked(), cx)
                            });
                        }),
                )
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

    fn add_row(input: &Entity<InputState>, theme: &Theme) -> impl IntoElement {
        h_flex()
            .gap_2()
            .px_3()
            .rounded_xl()
            .bg(theme.secondary)
            .text_sm()
            .child(
                Icon::new(IconName::Plus)
                    .small()
                    .text_color(theme.muted_foreground),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(input).appearance(false)),
            )
    }

    fn render_subtasks(&self, task: &Task, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let id = task.id;
        let (done, total) = task.subtask_progress();
        let rows = task.subtasks.iter().map(|sub| {
            let sub_id = sub.id;
            let checked = sub.completed_at.is_some();
            let group = SharedString::from(format!("sub-{sub_id}"));
            h_flex()
                .group(group.clone())
                .gap_2()
                .px_2()
                .py_1()
                .rounded_lg()
                .hover(|s| s.bg(theme.secondary))
                .child(
                    Checkbox::new(("sub-check", sub_id))
                        .checked(checked)
                        .on_click(cx.listener(move |this, _: &bool, _, cx| {
                            this.store
                                .update(cx, |s, cx| s.toggle_subtask(id, sub_id, cx));
                        })),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .when(checked, |d| {
                            d.line_through().text_color(theme.muted_foreground)
                        })
                        .child(sub.title.clone()),
                )
                .child(
                    div()
                        .opacity(0.)
                        .group_hover(group, |s| s.opacity(1.))
                        .child(
                            icon_button(("sub-del", sub_id), IconName::X, theme.danger, "Remove")
                                .on_click({
                                    let what = sub.title.clone();
                                    cx.listener(move |this, _, window, cx| {
                                        let store = this.store.clone();
                                        confirm_delete(
                                            window,
                                            cx,
                                            "Remove subtask?",
                                            format!("“{what}” will be removed."),
                                            move |_, cx| {
                                                store.update(cx, |s, cx| {
                                                    s.delete_subtask(id, sub_id, cx)
                                                });
                                            },
                                        );
                                    })
                                }),
                        ),
                )
        });
        let fraction = if total == 0 {
            0.
        } else {
            done as f32 / total as f32
        };
        Self::section(
            IconName::ListChecks,
            "Subtasks",
            (total > 0).then(|| format!("{done}/{total}")),
            theme,
        )
        .when(total > 0, |this| {
            this.child(fx::progress_bar(("sub-progress", id), fraction, theme))
        })
        .child(v_flex().children(rows))
        .child(Self::add_row(&self.subtask_input, theme))
        .into_any_element()
    }

    fn render_links(&self, task: &Task, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let id = task.id;
        let rows = task.links.iter().map(|link| {
            let link_id = link.id;
            let url = link.url.clone();
            let info = classify(&link.url);
            let (icon, color) = match info.kind {
                LinkKind::Jira => (IconName::Ticket, theme.chart_2),
                LinkKind::MergeRequest | LinkKind::PullRequest => {
                    (IconName::GitPullRequest, theme.chart_3)
                }
                LinkKind::Issue => (IconName::CircleDot, theme.chart_4),
                LinkKind::Docs => (IconName::BookOpen, theme.chart_5),
                LinkKind::Web => (IconName::Globe, theme.chart_1),
            };
            let group = SharedString::from(format!("link-{link_id}"));
            h_flex()
                .id(("link", link_id))
                .group(group.clone())
                .gap_3()
                .p_2()
                .rounded_xl()
                .cursor_pointer()
                .hover(|s| s.bg(theme.secondary))
                .on_click(move |_, _, cx| cx.open_url(&url))
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .size(px(30.))
                        .rounded_lg()
                        .bg(color.opacity(0.15))
                        .child(Icon::new(icon).small().text_color(color)),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .truncate()
                                .child(info.label),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .truncate()
                                .child(info.detail),
                        ),
                )
                .child(
                    h_flex()
                        .opacity(0.)
                        .group_hover(group, |s| s.opacity(1.))
                        .child(
                            Icon::new(IconName::ExternalLink)
                                .xsmall()
                                .text_color(theme.muted_foreground),
                        )
                        .child(
                            icon_button(("link-del", link_id), IconName::X, theme.danger, "Remove")
                                .on_click({
                                    let what = link.url.clone();
                                    cx.listener(move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        let store = this.store.clone();
                                        confirm_delete(
                                            window,
                                            cx,
                                            "Remove link?",
                                            format!("“{what}” will be removed."),
                                            move |_, cx| {
                                                store.update(cx, |s, cx| {
                                                    s.delete_link(id, link_id, cx)
                                                });
                                            },
                                        );
                                    })
                                }),
                        ),
                )
        });
        Self::section(IconName::Link, "Links", None, theme)
            .child(v_flex().children(rows))
            .child(Self::add_row(&self.link_input, theme))
            .into_any_element()
    }

    fn render_log(&self, task: &Task, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let id = task.id;
        let entries = task.log.iter().rev().map(|entry| {
            let entry_id = entry.id;
            let group = SharedString::from(format!("log-{entry_id}"));
            h_flex()
                .group(group.clone())
                .items_start()
                .gap_3()
                .child(
                    v_flex()
                        .items_center()
                        .pt_1p5()
                        .child(div().size_2().rounded_full().bg(theme.chart_1))
                        .child(div().w(px(1.)).min_h(px(18.)).flex_1().bg(theme.border)),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .pb_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(when(entry.at)),
                        )
                        .child(div().text_sm().child(entry.text.clone())),
                )
                .child(
                    div()
                        .opacity(0.)
                        .group_hover(group, |s| s.opacity(1.))
                        .child(
                            icon_button(("log-del", entry_id), IconName::X, theme.danger, "Remove")
                                .on_click({
                                    let what = entry.text.clone();
                                    cx.listener(move |this, _, window, cx| {
                                        let store = this.store.clone();
                                        confirm_delete(
                                            window,
                                            cx,
                                            "Remove log entry?",
                                            format!("“{what}” will be removed."),
                                            move |_, cx| {
                                                store.update(cx, |s, cx| {
                                                    s.delete_log(id, entry_id, cx)
                                                });
                                            },
                                        );
                                    })
                                }),
                        ),
                )
        });
        Self::section(
            IconName::ScrollText,
            "Work log",
            (!task.log.is_empty()).then(|| task.log.len().to_string()),
            theme,
        )
        .child(Self::add_row(&self.log_input, theme))
        .child(v_flex().pt_1().children(entries))
        .into_any_element()
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

        let content = v_flex()
            .w_full()
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
                    .child(self.render_notes(&task, &theme, cx))
                    .child(self.render_subtasks(&task, &theme, cx))
                    .child(self.render_links(&task, &theme, cx))
                    .child(self.render_log(&task, &theme, cx)),
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
                    .id("detail-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(content),
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

/// `14:05`, `Yesterday 14:05`, `Nov 3, 14:05`.
fn when(ts: DateTime<Utc>) -> String {
    let local = ts.with_timezone(&Local);
    let today = Local::now().date_naive();
    match (today - local.date_naive()).num_days() {
        0 => local.format("today %H:%M").to_string(),
        1 => local.format("yesterday %H:%M").to_string(),
        _ => local.format("%b %-d, %H:%M").to_string(),
    }
}

/// Notes sit in a narrow panel under small section labels: keep headings close to the
/// body size (hierarchy from weight) and paragraphs tight.
fn notes_style() -> TextViewStyle {
    TextViewStyle::default()
        .paragraph_gap(gpui_kit::rems(0.5))
        .heading_font_size(|level, _| match level {
            1 => px(18.),
            2 => px(16.),
            3 => px(15.),
            _ => px(14.),
        })
}
