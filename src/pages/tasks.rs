use std::time::Duration;

use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Sizable, Theme,
    button::{Button, ButtonRounded, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    input::{Escape, Input, InputEvent, InputState},
    menu::{DropdownMenu, PopupMenuItem},
    v_flex,
};
use gpui_kit::{
    Anchor, Animation, AnimationExt, AnyElement, App, Context, DragMoveEvent, Entity, EventEmitter,
    Focusable, FontWeight, Hsla, Render, SharedString, SpringAnimation, Subscription, Window, div,
    ease_in_out, prelude::*, px, rgb,
};

use super::OpenTask;
use crate::{
    components::{category_tag, chip, confirm::confirm_delete, fx, icon_button},
    prefs, quotes,
    tasks::{
        Category, Id, Priority, Task, TaskStore, classify, completions_by_day, last_days,
        parse_quick_add, streak,
    },
};

const COMPLETE_MS: u64 = 520;
const TOAST_SECS: u64 = 4;
const UNDO_SECS: u64 = 8;
const CHART_DAYS: usize = 7;
const CHART_H: f32 = 64.;

/// Numbers shown in the hero, computed once per render.
struct Stats {
    open: usize,
    done_today: usize,
    urgent: usize,
    streak: usize,
    week: Vec<usize>,
}

const VIEWS: [View; 2] = [View::Todo, View::Done];

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Todo,
    Done,
}

pub struct TasksPage {
    store: Entity<TaskStore>,
    composer: Entity<InputState>,
    view: View,
    filter: Option<Id>,
    draft_priority: Priority,
    draft_categories: Vec<Id>,
    editing: Option<Id>,
    last_deleted: Option<Task>,
    drag_session: Option<DragSession>,
    /// Task open in the detail panel (highlighted in the list).
    selected: Option<Id>,
    /// Ticked tasks playing their exit animation before they move to Done.
    completing: Vec<Id>,
    toast: Option<(u64, SharedString)>,
    _toast_timer: Option<gpui_kit::Task<()>>,
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct DragSession {
    dragged_id: Id,
    target: Option<(Id, bool)>,
}

enum BandRow {
    Task(Task),
    Placeholder,
}

struct Section {
    title: SharedString,
    color: Option<Hsla>,
    tasks: Vec<Task>,
}

#[derive(Clone)]
struct TaskDrag {
    id: Id,
    title: SharedString,
    categories: Vec<(SharedString, u32)>,
}

struct TaskDragPreview {
    title: SharedString,
    categories: Vec<(SharedString, u32)>,
}

impl Render for TaskDragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        h_flex()
            .w(px(540.))
            .gap_3()
            .px_3()
            .py_2()
            .rounded_xl()
            .bg(theme.group_box)
            .border_2()
            .border_color(theme.primary.opacity(0.45))
            .shadow_lg()
            .child(
                Icon::new(IconName::GripVertical)
                    .text_color(theme.foreground.opacity(0.5))
                    .small(),
            )
            .child(Checkbox::new("drag-preview-check"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .text_color(theme.foreground)
                    .child(self.title.clone()),
            )
            .children(
                self.categories
                    .iter()
                    .map(|(name, color)| category_tag(name.clone(), hex(*color))),
            )
    }
}

impl TasksPage {
    pub fn new(store: Entity<TaskStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let composer = cx.new(|cx| InputState::new(window, cx).placeholder("What needs doing?"));
        let subscriptions = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &composer,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    match event {
                        InputEvent::PressEnter { .. } => this.submit(window, cx),
                        // Add/Save enables once there's text.
                        InputEvent::Change => cx.notify(),
                        _ => {}
                    }
                },
            ),
        ];
        Self {
            store,
            composer,
            view: View::Todo,
            filter: None,
            draft_priority: Priority::default(),
            draft_categories: Vec::new(),
            editing: None,
            last_deleted: None,
            drag_session: None,
            selected: None,
            completing: Vec::new(),
            toast: None,
            _toast_timer: None,
            _subscriptions: subscriptions,
        }
    }

    fn complete_drag_if_finished(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.drag_session.take() else {
            return;
        };
        if cx.has_active_drag() {
            self.drag_session = Some(session);
            return;
        }
        if let Some((over_id, before)) = session.target
            && over_id != session.dragged_id
        {
            let dragged_id = session.dragged_id;
            self.store.update(cx, |store, cx| {
                store.reorder_task(dragged_id, over_id, before, cx);
            });
        }
    }

    fn ensure_drag_session(&mut self, dragged_id: Id, cx: &mut Context<Self>) {
        if self.drag_session.is_none() {
            self.drag_session = Some(DragSession {
                dragged_id,
                target: None,
            });
            cx.notify();
        }
    }

    fn set_drag_target(
        &mut self,
        dragged_id: Id,
        over_id: Id,
        before: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = &mut self.drag_session else {
            self.drag_session = Some(DragSession {
                dragged_id,
                target: Some((over_id, before)),
            });
            cx.notify();
            return;
        };
        if session.dragged_id != dragged_id {
            return;
        }
        let next = Some((over_id, before));
        if session.target == next {
            return;
        }
        session.target = next;
        cx.notify();
    }

    fn band_rows(&self, tasks: &[Task], session: &DragSession) -> Vec<BandRow> {
        let without: Vec<&Task> = tasks
            .iter()
            .filter(|t| t.id != session.dragged_id)
            .collect();

        match session.target {
            None => tasks
                .iter()
                .map(|t| {
                    if t.id == session.dragged_id {
                        BandRow::Placeholder
                    } else {
                        BandRow::Task(t.clone())
                    }
                })
                .collect(),
            Some((over_id, _)) if over_id == session.dragged_id => tasks
                .iter()
                .map(|t| {
                    if t.id == session.dragged_id {
                        BandRow::Placeholder
                    } else {
                        BandRow::Task(t.clone())
                    }
                })
                .collect(),
            Some((over_id, before)) => {
                let Some(over_ix) = without.iter().position(|t| t.id == over_id) else {
                    return tasks.iter().map(|t| BandRow::Task(t.clone())).collect();
                };
                let insert_ix = if before { over_ix } else { over_ix + 1 };
                let mut rows = Vec::new();
                for (i, t) in without.iter().enumerate() {
                    if i == insert_ix {
                        rows.push(BandRow::Placeholder);
                    }
                    rows.push(BandRow::Task((*t).clone()));
                }
                if insert_ix >= without.len() {
                    rows.push(BandRow::Placeholder);
                }
                rows
            }
        }
    }

    fn render_drop_placeholder(&self, theme: &Theme) -> AnyElement {
        div()
            .mx_3()
            .my_0p5()
            .h(px(34.))
            .rounded_xl()
            .border_2()
            .border_dashed()
            .border_color(theme.drag_border.opacity(0.55))
            .bg(theme.drop_target)
            .into_any_element()
    }

    pub fn set_selected(&mut self, id: Option<Id>, cx: &mut Context<Self>) {
        self.selected = id;
        cx.notify();
    }

    /// Plays the exit animation, then marks the task done and celebrates.
    fn complete(&mut self, id: Id, cx: &mut Context<Self>) {
        if self.completing.contains(&id) {
            return;
        }
        self.completing.push(id);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(COMPLETE_MS))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.completing.retain(|c| *c != id);
                this.store.update(cx, |store, cx| store.toggle_done(id, cx));
                this.celebrate(cx);
            });
        })
        .detach();
    }

    fn celebrate(&mut self, cx: &mut Context<Self>) {
        let by_day = completions_by_day(self.store.read(cx).tasks());
        let today = Local::now().date_naive();
        let n = by_day.get(&today).copied().unwrap_or(0);
        let open = self.store.read(cx).open_count();
        let message = match (n, open) {
            (_, 0) => "Everything's done. Take a breath — you earned it.".to_string(),
            (1, _) => "First one down. Momentum!".to_string(),
            (3, _) => "Hat trick — three done today.".to_string(),
            (5, _) => "Five today. You're on fire.".to_string(),
            (10, _) => "Ten today?! Legendary.".to_string(),
            (n, _) => format!("Nice! {n} done today."),
        };
        let nonce = self.toast.as_ref().map_or(1, |(n, _)| n + 1);
        self.toast = Some((nonce, message.into()));
        self._toast_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_secs(TOAST_SECS))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.toast.as_ref().is_some_and(|(n, _)| *n == nonce) {
                    this.toast = None;
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    pub fn focus_composer(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
    }

    fn clear_composer(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let parsed = parse_quick_add(&self.composer.read(cx).value());
        if parsed.title.is_empty() {
            return;
        }
        let priority = parsed.priority.unwrap_or(self.draft_priority);
        let fallback = self.draft_categories.clone();
        let editing = self.editing.take();
        self.store.update(cx, |store, cx| {
            let categories = if parsed.categories.is_empty() {
                fallback
            } else {
                parsed
                    .categories
                    .iter()
                    .filter_map(|name| store.ensure_category(name, cx))
                    .collect()
            };
            match editing {
                Some(id) => {
                    store.update_task(id, parsed.title, priority, categories, cx);
                    store.add_links(id, &parsed.links, cx);
                }
                None => {
                    store.add_task(parsed.title, priority, categories, parsed.links, cx);
                }
            }
        });
        self.clear_composer(window, cx);
        if editing.is_some() {
            self.reset_draft();
        }
        self.last_deleted = None;
        self.view = View::Todo;
        cx.notify();
    }

    fn reset_draft(&mut self) {
        self.draft_priority = Priority::default();
        self.draft_categories = self.filter.into_iter().collect();
    }

    fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing.take().is_some() {
            self.reset_draft();
        }
        self.clear_composer(window, cx);
        cx.notify();
    }

    fn start_edit(&mut self, id: Id, window: &mut Window, cx: &mut Context<Self>) {
        let Some(task) = self.store.read(cx).task(id).cloned() else {
            return;
        };
        self.editing = Some(id);
        self.draft_priority = task.priority;
        self.draft_categories = task.categories.clone();
        self.composer
            .update(cx, |input, cx| input.set_value(task.title, window, cx));
        self.focus_composer(window, cx);
        cx.notify();
    }

    fn delete(&mut self, id: Id, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing == Some(id) {
            self.cancel_edit(window, cx);
        }
        self.last_deleted = self.store.update(cx, |store, cx| store.delete_task(id, cx));
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_secs(UNDO_SECS))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.last_deleted.as_ref().is_some_and(|t| t.id == id) {
                    this.last_deleted = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn undo_delete(&mut self, cx: &mut Context<Self>) {
        if let Some(task) = self.last_deleted.take() {
            self.store
                .update(cx, |store, cx| store.restore_task(task, cx));
        }
        cx.notify();
    }

    fn set_filter(&mut self, filter: Option<Id>, cx: &mut Context<Self>) {
        self.filter = filter;
        if self.editing.is_none() {
            self.draft_categories = filter.into_iter().collect();
        }
        cx.notify();
    }

    fn stats(open: &[Task], done: &[Task]) -> Stats {
        let today = Local::now().date_naive();
        let by_day = completions_by_day(done);
        Stats {
            open: open.len(),
            done_today: by_day.get(&today).copied().unwrap_or(0),
            urgent: open
                .iter()
                .filter(|t| t.priority == Priority::Urgent)
                .count(),
            streak: streak(&by_day, today),
            week: last_days(&by_day, today, CHART_DAYS),
        }
    }

    fn render_hero(&self, stats: &Stats, theme: &Theme, cx: &App) -> impl IntoElement {
        let name = prefs::get(cx).name.trim().to_string();
        let greeting = if name.is_empty() {
            format!("{}.", quotes::greeting())
        } else {
            format!("{}, {name}.", quotes::greeting())
        };
        let subtitle = match (stats.open, stats.urgent) {
            (0, _) => "Inbox zero. Beautiful.".to_string(),
            (_, 0) => format!(
                "{} open · {} done today. Pick one and go.",
                stats.open, stats.done_today
            ),
            (_, 1) => "1 urgent task waiting — knock it out first.".to_string(),
            (_, n) => format!("{n} urgent tasks waiting — start at the top."),
        };
        let total = stats.open + stats.done_today;
        let fraction = if total == 0 {
            0.
        } else {
            stats.done_today as f32 / total as f32
        };

        let pill = |icon: IconName, color: Hsla, label: String| {
            h_flex()
                .gap_1p5()
                .px_2p5()
                .py_1()
                .rounded_full()
                .bg(color.opacity(0.14))
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color)
                .child(Icon::new(icon).xsmall().text_color(color))
                .child(label)
        };

        h_flex()
            .flex_wrap()
            .gap_x_8()
            .gap_y_5()
            .p_6()
            .rounded_2xl()
            .bg(fx::wash(theme, 0.20))
            .border_1()
            .border_color(theme.border)
            .child(
                v_flex()
                    .flex_1()
                    // Wraps the chart + ring below instead of squeezing the greeting.
                    .min_w(px(280.))
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.primary)
                            .child(
                                Local::now()
                                    .format("%A · %B %-d")
                                    .to_string()
                                    .to_uppercase(),
                            ),
                    )
                    .child(
                        div()
                            .text_3xl()
                            .font_weight(FontWeight::BOLD)
                            .child(greeting),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(subtitle),
                    )
                    .child(
                        h_flex()
                            .flex_wrap()
                            .gap_2()
                            .pt_2()
                            .child(pill(
                                IconName::Flame,
                                theme.chart_4,
                                match stats.streak {
                                    0 => "No streak yet".into(),
                                    n => format!("{n}-day streak"),
                                },
                            ))
                            .child(pill(
                                IconName::Zap,
                                theme.chart_5,
                                format!("{} done today", stats.done_today),
                            ))
                            .when(stats.urgent > 0, |this| {
                                this.child(pill(
                                    IconName::Flag,
                                    hex(Priority::Urgent.color()),
                                    format!("{} urgent", stats.urgent),
                                ))
                            }),
                    ),
            )
            .child(
                h_flex()
                    .flex_none()
                    .gap_8()
                    // The week chart is the first thing to go when the detail panel is open.
                    .when(self.selected.is_none(), |this| {
                        this.child(self.render_week(&stats.week, theme))
                    })
                    .child(
                        fx::ring_box(112.)
                            .child(fx::ring(
                                "hero-ring",
                                fraction,
                                112.,
                                10.,
                                fx::brand_pair(theme),
                                theme,
                            ))
                            .child(
                                v_flex()
                                    .items_center()
                                    .child(
                                        div().text_xl().font_weight(FontWeight::BOLD).child(
                                            format!("{}%", (fraction * 100.).round() as u32),
                                        ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child("of today"),
                                    ),
                            ),
                    ),
            )
    }

    /// Completions over the last week as springy bars; today glows.
    fn render_week(&self, week: &[usize], theme: &Theme) -> impl IntoElement {
        let max = week.iter().copied().max().unwrap_or(0).max(1) as f32;
        let today = Local::now().date_naive();
        let bars = week.iter().enumerate().map(|(ix, n)| {
            let is_today = ix == week.len() - 1;
            let day = today - chrono::Days::new((week.len() - 1 - ix) as u64);
            let height = (*n as f32 / max * CHART_H).max(4.);
            let bar = div()
                .w(px(10.))
                .rounded_full()
                .when(is_today, |this| {
                    this.bg(fx::brand_gradient(theme, 180.)).shadow(fx::glow(
                        theme.chart_1.opacity(0.5),
                        12.,
                        0.,
                    ))
                })
                .when(!is_today, |this| this.bg(theme.chart_1.opacity(0.28)))
                .with_spring(
                    ("week-bar", ix as u64),
                    SpringAnimation::new(fx::SPRING).to(px(height)).from(px(0.)),
                    |el, h| el.h(h),
                );
            v_flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .h(px(CHART_H))
                        .flex()
                        .flex_col()
                        .justify_end()
                        .child(bar),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(if is_today {
                            theme.foreground
                        } else {
                            theme.muted_foreground
                        })
                        .child(
                            day.weekday()
                                .to_string()
                                .chars()
                                .next()
                                .unwrap_or(' ')
                                .to_string(),
                        ),
                )
        });
        v_flex()
            .flex_none()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Last 7 days"),
            )
            .child(h_flex().items_end().gap_2().children(bars))
    }

    fn render_toast(&self, theme: &Theme) -> Option<impl IntoElement> {
        let (nonce, message) = self.toast.clone()?;
        let toast = h_flex()
            .gap_2()
            .px_4()
            .py_2()
            .rounded_full()
            .bg(fx::brand_gradient(theme, 90.))
            .shadow(fx::glow(theme.chart_1.opacity(0.4), 20., 0.))
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.primary_foreground)
            .child(Icon::new(IconName::Sparkles).text_color(theme.primary_foreground))
            .child(message);
        Some(
            h_flex()
                .justify_center()
                .child(fx::enter(toast, ("toast", nonce), 0)),
        )
    }

    fn render_composer(
        &self,
        categories: &[Category],
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let editing = self.editing.is_some();
        let focused = self.composer.focus_handle(cx).contains_focused(window, cx);

        let has_text = !self.composer.read(cx).value().trim().is_empty();
        let page = cx.entity().downgrade();

        // Priority: one pill showing the current choice; menu to change it.
        let priority = self.draft_priority;
        let priority_color = hex(priority.color());
        let priority_picker = {
            let page = page.clone();
            Button::new("draft-priority")
                .ghost()
                .small()
                .rounded(ButtonRounded::Size(px(999.)))
                .child(
                    h_flex()
                        .gap_1p5()
                        .child(div().size_2().rounded_full().bg(priority_color))
                        .child(priority.label()),
                )
                .tooltip("Priority  ·  or type !high")
                .dropdown_menu_with_anchor(Anchor::TopRight, move |mut menu, _, _| {
                    for p in Priority::ALL {
                        let page = page.clone();
                        menu = menu.item(
                            PopupMenuItem::new(p.label())
                                .checked(p == priority)
                                .on_click(move |_, _, cx| {
                                    let _ = page.update(cx, |this, cx| {
                                        this.draft_priority = p;
                                        cx.notify();
                                    });
                                }),
                        );
                    }
                    menu.min_w(px(150.))
                })
        };

        // Categories: summary of the selection; menu toggles each one.
        let selected: Vec<&Category> = categories
            .iter()
            .filter(|c| self.draft_categories.contains(&c.id))
            .collect();
        let category_label = match selected.as_slice() {
            [] => None,
            [one] => Some(one.name.clone()),
            [first, rest @ ..] => Some(format!("{} +{}", first.name, rest.len())),
        };
        let category_color = selected.first().map(|c| hex(c.color));
        let all_categories = categories.to_vec();
        let draft = self.draft_categories.clone();
        let category_picker = Button::new("draft-categories")
            .ghost()
            .small()
            .rounded(ButtonRounded::Size(px(999.)))
            .map(|b| match (category_label, category_color) {
                (Some(label), Some(color)) => b.child(
                    h_flex()
                        .gap_1p5()
                        .child(div().size_2().rounded_full().bg(color))
                        .child(label),
                ),
                _ => b.icon(Icon::new(IconName::Tags).text_color(theme.muted_foreground)),
            })
            .tooltip("Categories  ·  or type #name")
            .dropdown_menu_with_anchor(Anchor::TopRight, move |mut menu, _, _| {
                if all_categories.is_empty() {
                    return menu.label("No categories yet — type #name to add one");
                }
                for category in &all_categories {
                    let cid = category.id;
                    let page = page.clone();
                    menu = menu.item(
                        PopupMenuItem::new(category.name.clone())
                            .checked(draft.contains(&cid))
                            .on_click(move |_, _, cx| {
                                let _ = page.update(cx, |this, cx| {
                                    match this.draft_categories.iter().position(|c| *c == cid) {
                                        Some(ix) => {
                                            this.draft_categories.remove(ix);
                                        }
                                        None => this.draft_categories.push(cid),
                                    }
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu.min_w(px(180.))
            });

        h_flex()
            .gap_2()
            .pl_4()
            .pr_2()
            .py_2()
            .rounded_2xl()
            .bg(theme.group_box)
            .border_1()
            .border_color(if editing || focused {
                theme.primary.opacity(0.55)
            } else {
                theme.border
            })
            .when(focused, |this| {
                this.shadow(fx::glow(theme.primary.opacity(0.22), 28., 0.))
            })
            .on_action(cx.listener(|this, _: &Escape, window, cx| this.cancel_edit(window, cx)))
            .child(
                Icon::new(if editing {
                    IconName::Pencil
                } else {
                    IconName::Plus
                })
                .small()
                .text_color(if editing || focused {
                    theme.primary
                } else {
                    theme.muted_foreground
                }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(&self.composer).appearance(false)),
            )
            .child(priority_picker)
            .child(category_picker)
            .child(
                Button::new("composer-submit")
                    .primary()
                    .small()
                    .rounded(ButtonRounded::Size(px(999.)))
                    .label(if editing { "Save" } else { "Add" })
                    .disabled(!has_text)
                    .when(editing, |b| b.tooltip("Enter to save  ·  Esc to cancel"))
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            )
    }

    fn render_toolbar(
        &self,
        open: usize,
        done: usize,
        categories: &[Category],
        counts: &[(Id, usize)],
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let view_tabs = h_flex().gap_1().children(VIEWS.into_iter().map(|view| {
            let (label, count) = match view {
                View::Todo => ("To do", open),
                View::Done => ("Done", done),
            };
            let active = self.view == view;
            h_flex()
                .id(label)
                .h(px(32.))
                .gap_2()
                .pl_3()
                .pr_2()
                .rounded_full()
                .cursor_pointer()
                .text_sm()
                .when(active, |this| {
                    this.bg(theme.secondary_active)
                        .text_color(theme.foreground)
                        .font_weight(FontWeight::SEMIBOLD)
                })
                .when(!active, |this| {
                    this.text_color(theme.muted_foreground)
                        .hover(|s| s.bg(theme.secondary))
                })
                .child(label)
                .child(
                    div()
                        .min_w(px(20.))
                        .px_1p5()
                        .rounded_full()
                        .text_xs()
                        .text_center()
                        .when(active, |this| {
                            this.bg(theme.primary.opacity(0.16))
                                .text_color(theme.primary)
                        })
                        .when(!active, |this| this.bg(theme.secondary))
                        .child(count.to_string()),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.view = view;
                    cx.notify();
                }))
        }));

        let filter_chips = categories
            .iter()
            .filter_map(|category| {
                let count = counts
                    .iter()
                    .find(|(id, _)| *id == category.id)
                    .map_or(0, |(_, n)| *n);
                let selected = self.filter == Some(category.id);
                (count > 0 || selected).then(|| {
                    let id = category.id;
                    chip(
                        ("filter", id),
                        format!("{}  {count}", category.name),
                        Some(hex(category.color)),
                        selected,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let next = (this.filter != Some(id)).then_some(id);
                        this.set_filter(next, cx);
                    }))
                })
            })
            .collect::<Vec<_>>();

        h_flex()
            .justify_between()
            .gap_4()
            .child(div().flex_none().child(view_tabs))
            .when(!filter_chips.is_empty(), |this| {
                this.child(
                    h_flex()
                        .flex_wrap()
                        .justify_end()
                        .gap_1()
                        .child(
                            chip("filter-all", "All", None, self.filter.is_none())
                                .on_click(cx.listener(|this, _, _, cx| this.set_filter(None, cx))),
                        )
                        .children(filter_chips),
                )
            })
    }

    fn sections(&self, mut tasks: Vec<Task>) -> Vec<Section> {
        match self.view {
            View::Todo => {
                tasks.sort_by_key(|t| (t.priority, t.sort_order, t.created_at));
                Priority::ALL
                    .into_iter()
                    .filter_map(|priority| {
                        let tasks: Vec<Task> = tasks
                            .iter()
                            .filter(|t| t.priority == priority)
                            .cloned()
                            .collect();
                        (!tasks.is_empty()).then(|| Section {
                            title: priority.label().into(),
                            color: Some(hex(priority.color())),
                            tasks,
                        })
                    })
                    .collect()
            }
            View::Done => {
                tasks.sort_by_key(|t| std::cmp::Reverse(t.completed_at));
                let today = Local::now().date_naive();
                let mut sections: Vec<(NaiveDate, Section)> = Vec::new();
                for task in tasks {
                    let date = task
                        .completed_at
                        .map_or(today, |ts| ts.with_timezone(&Local).date_naive());
                    match sections.last_mut() {
                        Some((last, section)) if *last == date => section.tasks.push(task),
                        _ => sections.push((
                            date,
                            Section {
                                title: day_label(date, today).into(),
                                color: None,
                                tasks: vec![task],
                            },
                        )),
                    }
                }
                sections.into_iter().map(|(_, section)| section).collect()
            }
        }
    }

    fn render_list(
        &self,
        tasks: Vec<Task>,
        categories: &[Category],
        reorderable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        if tasks.is_empty() {
            let (glyph, message) = match (self.view, self.filter.is_some()) {
                (View::Todo, false) => (
                    IconName::Sparkles,
                    "All clear. Add something above when it comes up.",
                ),
                (View::Todo, true) => (IconName::Sun, "Nothing open in this category."),
                (View::Done, _) => (IconName::ListTodo, "Completed tasks will show up here."),
            };
            let badge = h_flex()
                .justify_center()
                .size(px(72.))
                .rounded_full()
                .bg(fx::brand_gradient(&theme, 135.))
                .shadow(fx::glow(theme.chart_1.opacity(0.4), 36., 0.))
                .child(
                    Icon::new(glyph)
                        .text_color(theme.primary_foreground)
                        .size_8(),
                );
            return v_flex()
                .items_center()
                .justify_center()
                .gap_4()
                .py_16()
                .text_color(theme.muted_foreground)
                .child(badge)
                .child(div().text_sm().child(message))
                .into_any_element();
        }

        let mut section_elements = Vec::new();
        let mut row_ix = 0;
        for section in self.sections(tasks) {
            let mut task_rows = Vec::new();
            let band_rows = if reorderable {
                if let Some(session) = &self.drag_session
                    && section.tasks.iter().any(|t| t.id == session.dragged_id)
                {
                    self.band_rows(&section.tasks, session)
                } else {
                    section.tasks.iter().cloned().map(BandRow::Task).collect()
                }
            } else {
                section.tasks.iter().cloned().map(BandRow::Task).collect()
            };
            for row in band_rows {
                match row {
                    BandRow::Placeholder => {
                        task_rows.push(self.render_drop_placeholder(&theme));
                    }
                    BandRow::Task(task) => {
                        let task_categories: Vec<&Category> = task
                            .categories
                            .iter()
                            .filter_map(|id| categories.iter().find(|c| c.id == *id))
                            .collect();
                        let id = task.id;
                        let row = self.render_row(&task, &task_categories, reorderable, cx);
                        task_rows.push(
                            fx::enter(div().child(row), ("row-in", id), row_ix).into_any_element(),
                        );
                        row_ix += 1;
                    }
                }
            }
            section_elements.push(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .px_3()
                            .pb_1()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.muted_foreground)
                            .when_some(section.color, |this, color| {
                                this.child(
                                    div().size_2().rounded_full().bg(color).shadow(fx::glow(
                                        color.opacity(0.7),
                                        8.,
                                        0.,
                                    )),
                                )
                            })
                            .child(section.title.to_uppercase())
                            .child(
                                div()
                                    .px_1p5()
                                    .rounded_full()
                                    .bg(theme.secondary)
                                    .child(section.tasks.len().to_string()),
                            )
                            .child(div().flex_1().h(px(1.)).ml_2().bg(theme.border)),
                    )
                    .children(task_rows)
                    .into_any_element(),
            );
        }

        v_flex()
            .id("task-list")
            .gap_5()
            .when(reorderable, |this| {
                this.on_drag_move::<TaskDrag>(cx.listener(
                    |this, event: &DragMoveEvent<TaskDrag>, _, cx| {
                        let Some(drag) = event.dragged_item().downcast_ref::<TaskDrag>() else {
                            return;
                        };
                        this.ensure_drag_session(drag.id, cx);
                    },
                ))
            })
            .children(section_elements)
            .into_any_element()
    }

    fn render_row(
        &self,
        task: &Task,
        categories: &[&Category],
        reorderable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let ambient = fx::ambient(cx);
        let id = task.id;
        let done = task.is_done();
        let completing = self.completing.contains(&id);
        let priority = task.priority;
        let group = SharedString::from(format!("task-row-{id}"));
        let hover = theme.secondary;
        let meta = match task.completed_at {
            Some(ts) => ts.with_timezone(&Local).format("%H:%M").to_string(),
            None => age(task.created_at),
        };

        let checkbox = Checkbox::new(("check", id))
            .checked(done || completing)
            .on_click(cx.listener(move |this, _: &bool, _, cx| {
                if done {
                    this.store.update(cx, |store, cx| store.toggle_done(id, cx));
                } else {
                    this.complete(id, cx);
                }
            }));

        let stripe_color = if done {
            theme.success
        } else {
            hex(priority.color())
        };
        let stripe = div()
            .flex_none()
            .w(px(3.))
            .h(px(18.))
            .rounded_full()
            .bg(stripe_color)
            .shadow(fx::glow(stripe_color.opacity(0.6), 8., 0.));
        let stripe = fx::pulse(
            stripe,
            ("urgent-pulse", id),
            ambient && !done && priority == Priority::Urgent,
            0.35,
        );

        let actions = h_flex()
            .flex_none()
            .gap_0p5()
            .opacity(0.)
            .group_hover(group.clone(), |s| s.opacity(1.))
            .when(!done, |this| {
                let next = priority.cycle();
                this.child(
                    icon_button(
                        ("priority", id),
                        IconName::Flag,
                        hex(next.color()),
                        format!("Priority → {}", next.label()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.store
                            .update(cx, |store, cx| store.set_priority(id, next, cx));
                    })),
                )
                .child(
                    icon_button(
                        ("edit", id),
                        IconName::Pencil,
                        theme.muted_foreground,
                        "Edit",
                    )
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.start_edit(id, window, cx)),
                    ),
                )
            })
            .child(
                icon_button(("delete", id), IconName::X, theme.danger, "Delete").on_click({
                    let title = task.title.clone();
                    cx.listener(move |_, _, window, cx| {
                        let view = cx.entity().downgrade();
                        confirm_delete(
                            window,
                            cx,
                            "Delete task?",
                            format!(
                                "“{title}” and its notes, subtasks, links and log will be removed."
                            ),
                            move |window, cx| {
                                let _ = view.update(cx, |this, cx| this.delete(id, window, cx));
                            },
                        );
                    })
                }),
            );

        let drag_handle = reorderable.then(|| {
            let drag = TaskDrag {
                id,
                title: task.title.clone().into(),
                categories: categories
                    .iter()
                    .map(|c| (c.name.clone().into(), c.color))
                    .collect(),
            };
            div()
                .id(("drag", id))
                .flex_none()
                .cursor_move()
                .opacity(0.)
                .group_hover(group.clone(), |s| s.opacity(1.))
                .child(
                    Icon::new(IconName::GripVertical)
                        .text_color(theme.muted_foreground)
                        .small(),
                )
                .on_drag(drag, |drag: &TaskDrag, _, _, cx| {
                    cx.new(|_| TaskDragPreview {
                        title: drag.title.clone(),
                        categories: drag.categories.clone(),
                    })
                })
        });

        let mut row = h_flex()
            .id(("task", id))
            .group(group)
            .gap_1p5()
            .px_3()
            .py_1p5()
            .rounded_xl()
            .when(self.editing == Some(id), |this| {
                this.bg(theme.secondary_active)
            })
            .when(
                self.selected == Some(id) && self.editing != Some(id),
                |this| this.bg(theme.secondary_active),
            )
            .when(completing, |this| this.bg(theme.success.opacity(0.12)))
            .hover(move |s| s.bg(hover))
            .child(stripe)
            .when_some(drag_handle, |this, handle| this.child(handle))
            .child(checkbox);
        if reorderable {
            row = row.on_drag_move::<TaskDrag>(cx.listener(
                move |this, event: &DragMoveEvent<TaskDrag>, _, cx| {
                    let Some(drag) = event.dragged_item().downcast_ref::<TaskDrag>() else {
                        return;
                    };
                    let dragged_id = drag.id;
                    this.ensure_drag_session(dragged_id, cx);
                    let pos = event.event.position;
                    let bounds = event.bounds;
                    if pos.y < bounds.top() || pos.y > bounds.bottom() {
                        return;
                    }
                    if dragged_id == id {
                        return;
                    }
                    let before = pos.y < bounds.center().y;
                    this.set_drag_target(dragged_id, id, before, cx);
                },
            ));
        }
        let muted = theme.muted_foreground;
        let badge = |icon: IconName, label: String| {
            h_flex()
                .flex_none()
                .gap_1()
                .text_xs()
                .text_color(muted)
                .child(Icon::new(icon).xsmall().text_color(muted))
                .child(label)
        };
        let (sub_done, sub_total) = task.todo_progress();
        let links = task.web_links();
        let today = Local::now().date_naive();
        // Title wins the space: badges and tags shrink and clip before it truncates.
        let badges = h_flex()
            .min_w_0()
            .overflow_hidden()
            .gap_3()
            .when_some(task.due.filter(|_| !done), |this, due| {
                let color = if task.is_overdue(today) {
                    theme.danger
                } else if due == today {
                    theme.warning
                } else {
                    muted
                };
                this.child(
                    h_flex()
                        .gap_1()
                        .text_xs()
                        .text_color(color)
                        .child(Icon::new(IconName::Calendar).xsmall().text_color(color))
                        .child(due_label(due, today)),
                )
            })
            .when(sub_total > 0, |this| {
                this.child(badge(
                    IconName::ListChecks,
                    format!("{sub_done}/{sub_total}"),
                ))
            })
            .when(!links.is_empty(), |this| {
                let label = match links.as_slice() {
                    [only] => classify(only).label,
                    many => many.len().to_string(),
                };
                this.child(badge(IconName::Link, label))
            })
            .when(!task.notes.trim().is_empty(), |this| {
                this.child(Icon::new(IconName::NotebookPen).xsmall().text_color(muted))
            });
        let row = row
            .child(
                h_flex()
                    .id(("open", id))
                    .flex_1()
                    .min_w(px(TITLE_MIN_W))
                    .gap_3()
                    .cursor_pointer()
                    .on_click(cx.listener(move |_, _, _, cx| cx.emit(OpenTask(id))))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(TITLE_MIN_W))
                            .truncate()
                            .text_sm()
                            .when(done || completing, |this| {
                                this.line_through().text_color(theme.muted_foreground)
                            })
                            .child(task.title.clone()),
                    )
                    .child(badges),
            )
            // Two tags fit comfortably; more collapse into a "+n" pill.
            .when(!categories.is_empty(), |this| {
                this.child(
                    h_flex()
                        .min_w_0()
                        .overflow_hidden()
                        .gap_1p5()
                        .children(categories.iter().take(2).map(|category| {
                            category_tag(category.name.clone(), hex(category.color)).flex_none()
                        }))
                        .when(categories.len() > 2, |this| {
                            let rest = categories[2..]
                                .iter()
                                .map(|c| c.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ");
                            this.child(
                                div()
                                    .id(("more-categories", id))
                                    .flex_none()
                                    .px_1p5()
                                    .rounded_full()
                                    .bg(theme.secondary)
                                    .text_xs()
                                    .text_color(muted)
                                    .child(format!("+{}", categories.len() - 2))
                                    .tooltip(move |window, cx| {
                                        gpui_kit::component::tooltip::Tooltip::new(rest.clone())
                                            .build(window, cx)
                                    }),
                            )
                        }),
                )
            })
            .child(
                div()
                    .flex_none()
                    .w(px(40.))
                    .text_right()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(meta),
            )
            .child(actions);

        if completing {
            row.with_animation(
                ("completing", id),
                Animation::new(Duration::from_millis(COMPLETE_MS)).with_easing(ease_in_out),
                |el, t| {
                    let fade = ((t - 0.35) / 0.65).clamp(0., 1.);
                    el.opacity(1. - fade).relative().left(px(fade * 28.))
                },
            )
            .into_any_element()
        } else {
            row.into_any_element()
        }
    }
}

/// Width a task title keeps before row badges and tags give way.
const TITLE_MIN_W: f32 = 160.;

impl EventEmitter<OpenTask> for TasksPage {}

impl Render for TasksPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.complete_drag_if_finished(cx);
        let theme = cx.theme().clone();
        let store = self.store.read(cx);
        let categories = store.categories().to_vec();
        let save_error = store.save_error().cloned();
        let (done, open): (Vec<Task>, Vec<Task>) =
            store.tasks().iter().cloned().partition(Task::is_done);

        let stats = Self::stats(&open, &done);
        let (open_count, done_count) = (open.len(), done.len());

        let in_view = match self.view {
            View::Todo => open,
            View::Done => done,
        };
        let counts: Vec<(Id, usize)> = categories
            .iter()
            .map(|c| {
                (
                    c.id,
                    in_view
                        .iter()
                        .filter(|t| t.categories.contains(&c.id))
                        .count(),
                )
            })
            .collect();
        let visible: Vec<Task> = in_view
            .into_iter()
            .filter(|t| self.filter.is_none_or(|f| t.categories.contains(&f)))
            .collect();

        v_flex()
            .gap_5()
            .child(self.render_hero(&stats, &theme, cx))
            .when_some(save_error, |this, error| {
                this.child(
                    div()
                        .px_4()
                        .py_2()
                        .rounded_xl()
                        .bg(theme.danger.opacity(0.15))
                        .text_sm()
                        .text_color(theme.danger)
                        .child(error),
                )
            })
            .child(self.render_composer(&categories, window, cx))
            .children(self.render_toast(&theme))
            .child(self.render_toolbar(open_count, done_count, &categories, &counts, cx))
            .when_some(self.last_deleted.as_ref(), |this, task| {
                let notice = h_flex()
                    .gap_3()
                    .pl_4()
                    .pr_1()
                    .py_1()
                    .rounded_full()
                    .bg(theme.secondary)
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(
                        div()
                            .max_w(px(360.))
                            .truncate()
                            .child(format!("Deleted “{}”", task.title)),
                    )
                    .child(
                        div()
                            .id("undo-delete")
                            .px_3()
                            .py_0p5()
                            .rounded_full()
                            .cursor_pointer()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.primary)
                            .hover(|s| s.bg(theme.primary.opacity(0.12)))
                            .child("Undo")
                            .on_click(cx.listener(|this, _, _, cx| this.undo_delete(cx))),
                    );
                this.child(h_flex().justify_center().child(fx::enter(
                    notice,
                    ("undo-notice", task.id),
                    0,
                )))
            })
            .child(self.render_list(visible, &categories, self.view == View::Todo, cx))
    }
}

fn hex(color: u32) -> Hsla {
    rgb(color).into()
}

fn age(ts: DateTime<Utc>) -> String {
    let minutes = (Utc::now() - ts).num_minutes();
    match minutes {
        ..1 => "now".into(),
        1..60 => format!("{minutes}m"),
        60..1440 => format!("{}h", minutes / 60),
        _ => format!("{}d", minutes / 1440),
    }
}

/// `Today`, `Tomorrow`, `Yesterday`, `Fri`, `Nov 20`.
pub fn due_label(due: NaiveDate, today: NaiveDate) -> String {
    match (due - today).num_days() {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        -1 => "Yesterday".into(),
        2..=6 => due.format("%a").to_string(),
        _ if due.year() == today.year() => due.format("%b %-d").to_string(),
        _ => due.format("%b %-d, %Y").to_string(),
    }
}

fn day_label(date: NaiveDate, today: NaiveDate) -> String {
    match (today - date).num_days() {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        2..=6 => date.format("%A").to_string(),
        _ => date.format("%a, %b %-d").to_string(),
    }
}
