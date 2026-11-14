//! Side panel for one task: title, subtasks, links (Jira/GitLab aware), notes and a work log.
//! Owned by `Workspace`; opened via `OpenTask` events from pages.

use std::time::Duration;

use chrono::{DateTime, Local, Utc};
use gpui_kit::component::{
    ActiveTheme, Icon, Sizable, Theme,
    checkbox::Checkbox,
    h_flex,
    input::{Escape, Input, InputEvent, InputState, Textarea, TextareaState},
    v_flex,
};
use gpui_kit::{
    Animation, AnimationExt, AnyElement, Context, Entity, EventEmitter, FontWeight, Hsla,
    SharedString, Subscription, Window, assets::IconName, div, ease_out_quint, prelude::*, px, rgb,
};

use crate::{
    components::{category_tag, fx, icon_button},
    tasks::{Id, LinkKind, Task, TaskStore, classify, is_url},
};

pub const PANEL_W: f32 = 460.;

/// Emitted by any page that wants a task opened in the panel.
pub struct OpenTask(pub Id);

pub enum DetailEvent {
    Close,
}

impl EventEmitter<DetailEvent> for TaskDetail {}

pub struct TaskDetail {
    store: Entity<TaskStore>,
    task: Option<Id>,
    title: Entity<InputState>,
    notes: Entity<TextareaState>,
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
                .auto_grow(6, 40)
                .placeholder("Notes, context, decisions, snippets…")
        });
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
                if let (InputEvent::Change, Some(id)) = (event, this.task) {
                    let value = input.read(cx).value();
                    this.store
                        .update(cx, |store, cx| store.set_notes(id, &value, cx));
                }
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
            title,
            notes,
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

    pub fn open(&mut self, id: Id, window: &mut Window, cx: &mut Context<Self>) {
        let Some(task) = self.store.read(cx).task(id).cloned() else {
            return;
        };
        self.task = Some(id);
        self.title
            .update(cx, |i, cx| i.set_value(task.title, window, cx));
        self.notes
            .update(cx, |i, cx| i.set_value(task.notes, window, cx));
        for input in [&self.subtask_input, &self.link_input, &self.log_input] {
            input.update(cx, |i, cx| i.set_value("", window, cx));
        }
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.task = None;
        cx.emit(DetailEvent::Close);
        cx.notify();
    }

    fn render_header(
        &self,
        task: &Task,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = task.id;
        let done = task.is_done();
        let priority = task.priority;
        let color: Hsla = rgb(priority.color()).into();
        let next = priority.cycle();
        let category = task.category.and_then(|cid| {
            self.store
                .read(cx)
                .categories()
                .iter()
                .find(|c| c.id == cid)
                .cloned()
        });

        h_flex()
            .gap_2()
            .px_5()
            .pt_4()
            .pb_2()
            .child(
                Checkbox::new("detail-done")
                    .checked(done)
                    .on_click(cx.listener(move |this, _: &bool, _, cx| {
                        this.store.update(cx, |s, cx| s.toggle_done(id, cx));
                    })),
            )
            .child(
                h_flex()
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
                    })),
            )
            .when_some(category, |this, c| {
                this.child(category_tag(c.name, rgb(c.color).into()))
            })
            .child(div().flex_1())
            .child(div().text_xs().text_color(theme.muted_foreground).child(
                match task.completed_at {
                    Some(ts) => format!("Done {}", when(ts)),
                    None => format!("Created {}", when(task.created_at)),
                },
            ))
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
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.store
                                        .update(cx, |s, cx| s.delete_subtask(id, sub_id, cx));
                                })),
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
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.store
                                        .update(cx, |s, cx| s.delete_link(id, link_id, cx));
                                })),
                        ),
                )
        });
        Self::section(IconName::Link, "Links", None, theme)
            .child(v_flex().children(rows))
            .child(Self::add_row(&self.link_input, theme))
            .into_any_element()
    }

    fn render_notes(&self, theme: &Theme) -> AnyElement {
        Self::section(IconName::NotebookPen, "Notes", None, theme)
            .child(
                div()
                    .px_3()
                    .py_2()
                    .rounded_xl()
                    .bg(theme.secondary)
                    .text_sm()
                    .child(Textarea::new(&self.notes).appearance(false)),
            )
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
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.store
                                        .update(cx, |s, cx| s.delete_log(id, entry_id, cx));
                                })),
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let Some(task) = self
            .task
            .and_then(|id| self.store.read(cx).task(id).cloned())
        else {
            return div().into_any_element();
        };

        v_flex()
            .flex_none()
            .w(px(PANEL_W))
            .h_full()
            .rounded_2xl()
            .bg(theme.group_box)
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .on_action(cx.listener(|this, _: &Escape, _, cx| this.close(cx)))
            .child(self.render_header(&task, &theme, cx))
            .child(
                div()
                    .px_5()
                    .text_xl()
                    .font_weight(FontWeight::BOLD)
                    .child(Input::new(&self.title).appearance(false)),
            )
            .child(
                v_flex()
                    .id("detail-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_5()
                    .pt_3()
                    .pb_6()
                    .gap_6()
                    .child(self.render_subtasks(&task, &theme, cx))
                    .child(self.render_links(&task, &theme, cx))
                    .child(self.render_notes(&theme))
                    .child(self.render_log(&task, &theme, cx)),
            )
            .with_animation(
                "detail-panel",
                Animation::new(Duration::from_millis(320)).with_easing(ease_out_quint()),
                |el, t| el.opacity(t).relative().left(px((1. - t) * 24.)),
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
