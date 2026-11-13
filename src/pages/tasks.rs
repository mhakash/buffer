use chrono::{DateTime, Local, NaiveDate, Utc};
use gpui::{
    AnyElement, App, Context, Entity, Focusable, FontWeight, Hsla, SharedString, Subscription,
    Window, div, prelude::*, px, rgb,
};

use crate::{
    components::{Button, IconName, TextInput, TextInputEvent, chip, icon, icon_button},
    tasks::{Category, Id, Priority, Task, TaskStore, parse_quick_add},
    theme::{ActiveTheme, Theme},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Todo,
    Done,
}

pub struct TasksPage {
    store: Entity<TaskStore>,
    composer: Entity<TextInput>,
    view: View,
    filter: Option<Id>,
    draft_priority: Priority,
    draft_category: Option<Id>,
    editing: Option<Id>,
    last_deleted: Option<Task>,
    _subscriptions: Vec<Subscription>,
}

struct Section {
    title: SharedString,
    color: Option<Hsla>,
    tasks: Vec<Task>,
}

impl TasksPage {
    pub fn new(store: Entity<TaskStore>, cx: &mut Context<Self>) -> Self {
        let composer =
            cx.new(|cx| TextInput::new("Add a task…   tip: #category  !urgent  !high  !low", cx));
        let subscriptions = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            cx.subscribe(
                &composer,
                |this, _, event: &TextInputEvent, cx| match event {
                    TextInputEvent::Submit => this.submit(cx),
                    TextInputEvent::Cancel => this.cancel_edit(cx),
                },
            ),
        ];
        Self {
            store,
            composer,
            view: View::Todo,
            filter: None,
            draft_priority: Priority::default(),
            draft_category: None,
            editing: None,
            last_deleted: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn focus_composer(&self, window: &mut Window, cx: &App) {
        self.composer.focus_handle(cx).focus(window);
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let parsed = parse_quick_add(self.composer.read(cx).text());
        if parsed.title.is_empty() {
            return;
        }
        let priority = parsed.priority.unwrap_or(self.draft_priority);
        let fallback_category = self.draft_category;
        let editing = self.editing.take();
        self.store.update(cx, |store, cx| {
            let category = match parsed.category.as_deref() {
                Some(name) => store.ensure_category(name, cx),
                None => fallback_category,
            };
            match editing {
                Some(id) => store.update_task(id, parsed.title, priority, category, cx),
                None => store.add_task(parsed.title, priority, category, cx),
            }
        });
        self.composer.update(cx, |input, cx| input.clear(cx));
        if editing.is_some() {
            self.reset_draft();
        }
        self.last_deleted = None;
        self.view = View::Todo;
        cx.notify();
    }

    fn reset_draft(&mut self) {
        self.draft_priority = Priority::default();
        self.draft_category = self.filter;
    }

    fn cancel_edit(&mut self, cx: &mut Context<Self>) {
        if self.editing.take().is_some() {
            self.reset_draft();
        }
        self.composer.update(cx, |input, cx| input.clear(cx));
        cx.notify();
    }

    fn start_edit(&mut self, id: Id, window: &mut Window, cx: &mut Context<Self>) {
        let Some(task) = self.store.read(cx).task(id).cloned() else {
            return;
        };
        self.editing = Some(id);
        self.draft_priority = task.priority;
        self.draft_category = task.category;
        self.composer
            .update(cx, |input, cx| input.set_text(task.title, cx));
        self.focus_composer(window, cx);
        cx.notify();
    }

    fn delete(&mut self, id: Id, cx: &mut Context<Self>) {
        if self.editing == Some(id) {
            self.cancel_edit(cx);
        }
        self.last_deleted = self.store.update(cx, |store, cx| store.delete_task(id, cx));
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
            self.draft_category = filter;
        }
        cx.notify();
    }

    fn render_header(
        &self,
        open: usize,
        done_today: usize,
        urgent: usize,
        theme: &Theme,
    ) -> impl IntoElement {
        div()
            .flex()
            .items_end()
            .justify_between()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::BOLD)
                            .child("Tasks"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.text_muted)
                            .child(format!("{open} open  ·  {done_today} done today")),
                    ),
            )
            .when(urgent > 0, |this| {
                let red = hex(Priority::Urgent.color());
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .px_2p5()
                        .py_1()
                        .rounded_full()
                        .bg(red.opacity(0.15))
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(red)
                        .child(div().size_2().rounded_full().bg(red))
                        .child(format!("{urgent} urgent")),
                )
            })
    }

    fn render_composer(&self, categories: &[Category], cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let editing = self.editing.is_some();

        let priority_chips = Priority::ALL.into_iter().map(|priority| {
            chip(
                ("draft-priority", priority as u64),
                priority.label(),
                Some(hex(priority.color())),
                self.draft_priority == priority,
                cx,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.draft_priority = priority;
                cx.notify();
            }))
        });

        let category_chips = std::iter::once(None)
            .chain(categories.iter().map(Some))
            .map(|category| {
                let id = category.map(|c| c.id);
                chip(
                    ("draft-category", id.unwrap_or(0)),
                    category.map_or("No category".to_string(), |c| c.name.clone()),
                    category.map(|c| hex(c.color)),
                    self.draft_category == id,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.draft_category = id;
                    cx.notify();
                }))
            });

        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .rounded_2xl()
            .bg(theme.surface)
            .border_1()
            .border_color(if editing {
                theme.accent
            } else {
                gpui::transparent_black()
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(icon(
                        if editing {
                            IconName::Pencil
                        } else {
                            IconName::Plus
                        },
                        theme.text_muted,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_base()
                            .child(self.composer.clone()),
                    )
                    .child(
                        Button::new("composer-submit", if editing { "Save" } else { "Add" })
                            .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_1()
                    .children(priority_chips)
                    .child(div().w(px(1.)).h_4().mx_2().bg(theme.border))
                    .children(category_chips)
                    .when(editing, |this| {
                        this.child(
                            div()
                                .ml_auto()
                                .text_xs()
                                .text_color(theme.text_muted)
                                .child("Enter to save  ·  Esc to cancel"),
                        )
                    }),
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

        let segment = |id: &'static str,
                       label: &'static str,
                       count: usize,
                       view: View,
                       cx: &mut Context<Self>| {
            let selected = self.view == view;
            let muted = theme.text_muted;
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_2()
                .px_3p5()
                .py_1()
                .rounded_full()
                .text_sm()
                .cursor_pointer()
                .map(|this| {
                    if selected {
                        this.bg(theme.selected).text_color(theme.text)
                    } else {
                        let text = theme.text;
                        this.text_color(muted).hover(move |s| s.text_color(text))
                    }
                })
                .child(label)
                .child(div().text_xs().text_color(muted).child(count.to_string()))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.view = view;
                    cx.notify();
                }))
        };

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
                        cx,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let next = (this.filter != Some(id)).then_some(id);
                        this.set_filter(next, cx);
                    }))
                })
            })
            .collect::<Vec<_>>();

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_none()
                    .p_1()
                    .rounded_full()
                    .bg(theme.surface)
                    .child(segment("view-todo", "To do", open, View::Todo, cx))
                    .child(segment("view-done", "Done", done, View::Done, cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_end()
                    .gap_1()
                    .child(
                        chip("filter-all", "All", None, self.filter.is_none(), cx)
                            .on_click(cx.listener(|this, _, _, cx| this.set_filter(None, cx))),
                    )
                    .children(filter_chips),
            )
    }

    fn sections(&self, mut tasks: Vec<Task>) -> Vec<Section> {
        match self.view {
            View::Todo => {
                tasks.sort_by_key(|t| (t.priority, t.created_at));
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
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        if tasks.is_empty() {
            let (glyph, message) = match (self.view, self.filter.is_some()) {
                (View::Todo, false) => (
                    IconName::Sun,
                    "All clear. Add something above when it comes up.",
                ),
                (View::Todo, true) => (IconName::Sun, "Nothing open in this category."),
                (View::Done, _) => (IconName::Tasks, "Completed tasks will show up here."),
            };
            return div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_3()
                .py_16()
                .text_color(theme.text_muted)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(56.))
                        .rounded_full()
                        .bg(theme.surface)
                        .child(icon(glyph, theme.text_muted).size_6()),
                )
                .child(div().text_sm().child(message))
                .into_any_element();
        }

        div()
            .flex()
            .flex_col()
            .gap_5()
            .children(self.sections(tasks).into_iter().map(|section| {
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .pb_1()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_muted)
                            .when_some(section.color, |this, color| {
                                this.child(div().size_2().rounded_full().bg(color))
                            })
                            .child(section.title.to_uppercase())
                            .child(div().opacity(0.7).child(section.tasks.len().to_string())),
                    )
                    .children(section.tasks.iter().map(|task| {
                        let category = task
                            .category
                            .and_then(|id| categories.iter().find(|c| c.id == id));
                        self.render_row(task, category, cx)
                    }))
            }))
            .into_any_element()
    }

    fn render_row(
        &self,
        task: &Task,
        category: Option<&Category>,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let id = task.id;
        let done = task.is_done();
        let priority = task.priority;
        let priority_color = hex(priority.color());
        let group = SharedString::from(format!("task-row-{id}"));
        let hover = theme.hover;
        let meta = match task.completed_at {
            Some(ts) => ts.with_timezone(&Local).format("%H:%M").to_string(),
            None => age(task.created_at),
        };

        let checkbox = div()
            .id(("check", id))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(18.))
            .rounded_full()
            .border_2()
            .cursor_pointer()
            .map(|this| {
                if done {
                    this.border_color(theme.success)
                        .bg(theme.success)
                        .child(icon(IconName::Check, gpui::white()).size_2p5())
                } else {
                    this.border_color(priority_color)
                        .hover(move |s| s.bg(priority_color.opacity(0.25)))
                }
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.store.update(cx, |store, cx| store.toggle_done(id, cx));
            }));

        let actions = div()
            .flex()
            .flex_none()
            .gap_0p5()
            .opacity(0.)
            .group_hover(group.clone(), |s| s.opacity(1.))
            .when(!done, |this| {
                this.child(
                    icon_button(
                        ("priority", id),
                        IconName::Flag,
                        hex(priority.cycle().color()),
                        cx,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.store
                            .update(cx, |store, cx| store.set_priority(id, priority.cycle(), cx));
                    })),
                )
                .child(
                    icon_button(("edit", id), IconName::Pencil, theme.text_muted, cx).on_click(
                        cx.listener(move |this, _, window, cx| this.start_edit(id, window, cx)),
                    ),
                )
            })
            .child(
                icon_button(("delete", id), IconName::X, theme.danger, cx)
                    .on_click(cx.listener(move |this, _, _, cx| this.delete(id, cx))),
            );

        div()
            .id(("task", id))
            .group(group)
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .rounded_xl()
            .when(self.editing == Some(id), |this| this.bg(theme.selected))
            .hover(move |s| s.bg(hover))
            .child(checkbox)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .when(done, |this| {
                        this.line_through().text_color(theme.text_muted)
                    })
                    .child(task.title.clone()),
            )
            .when_some(category, |this, category| {
                let color = hex(category.color);
                this.child(
                    div()
                        .flex_none()
                        .px_2()
                        .py_0p5()
                        .rounded_full()
                        .bg(color.opacity(0.15))
                        .text_xs()
                        .text_color(color)
                        .child(category.name.clone()),
                )
            })
            .child(
                div()
                    .flex_none()
                    .w(px(40.))
                    .text_right()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(meta),
            )
            .child(actions)
    }
}

impl Render for TasksPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let store = self.store.read(cx);
        let categories = store.categories().to_vec();
        let save_error = store.save_error().cloned();
        let (done, open): (Vec<Task>, Vec<Task>) =
            store.tasks().iter().cloned().partition(Task::is_done);

        let today = Local::now().date_naive();
        let done_today = done
            .iter()
            .filter(|t| {
                t.completed_at
                    .is_some_and(|ts| ts.with_timezone(&Local).date_naive() == today)
            })
            .count();
        let urgent = open
            .iter()
            .filter(|t| t.priority == Priority::Urgent)
            .count();
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
                    in_view.iter().filter(|t| t.category == Some(c.id)).count(),
                )
            })
            .collect();
        let visible: Vec<Task> = in_view
            .into_iter()
            .filter(|t| self.filter.is_none() || t.category == self.filter)
            .collect();

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_header(open_count, done_today, urgent, &theme))
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
            .child(self.render_composer(&categories, cx))
            .child(self.render_toolbar(open_count, done_count, &categories, &counts, cx))
            .when_some(self.last_deleted.as_ref(), |this, task| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_4()
                        .py_2()
                        .rounded_xl()
                        .bg(theme.surface)
                        .text_sm()
                        .text_color(theme.text_muted)
                        .child(format!("Deleted “{}”", task.title))
                        .child(
                            div()
                                .id("undo-delete")
                                .text_color(theme.accent)
                                .font_weight(FontWeight::SEMIBOLD)
                                .cursor_pointer()
                                .child("Undo")
                                .on_click(cx.listener(|this, _, _, cx| this.undo_delete(cx))),
                        ),
                )
            })
            .child(self.render_list(visible, &categories, cx))
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

fn day_label(date: NaiveDate, today: NaiveDate) -> String {
    match (today - date).num_days() {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        2..=6 => date.format("%A").to_string(),
        _ => date.format("%a, %b %-d").to_string(),
    }
}
