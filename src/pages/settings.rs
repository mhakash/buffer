use gpui::{Context, Entity, FontWeight, Subscription, Window, div, prelude::*, px, rgb};

use crate::{
    components::{Button, ButtonVariant, TextInput, TextInputEvent, card, icon_button},
    storage,
    tasks::TaskStore,
    theme::{ActiveTheme, Appearance, Theme},
};

pub struct SettingsPage {
    store: Entity<TaskStore>,
    category_input: Entity<TextInput>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsPage {
    pub fn new(store: Entity<TaskStore>, cx: &mut Context<Self>) -> Self {
        let category_input = cx.new(|cx| TextInput::new("New category name, then Enter", cx));
        let subscriptions = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            cx.subscribe(
                &category_input,
                |this, input, event: &TextInputEvent, cx| {
                    if let TextInputEvent::Submit = event {
                        let name = input.read(cx).text().to_string();
                        this.store
                            .update(cx, |store, cx| store.ensure_category(&name, cx));
                    }
                    input.update(cx, |input, cx| input.clear(cx));
                },
            ),
        ];
        Self {
            store,
            category_input,
            _subscriptions: subscriptions,
        }
    }

    fn render_categories(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let store = self.store.read(cx);
        let rows: Vec<_> = store
            .categories()
            .iter()
            .map(|category| {
                let (open, done) = store
                    .tasks()
                    .iter()
                    .filter(|t| t.category == Some(category.id))
                    .fold((0, 0), |(open, done), t| {
                        if t.is_done() {
                            (open, done + 1)
                        } else {
                            (open + 1, done)
                        }
                    });
                (category.clone(), open, done)
            })
            .collect();

        card("Categories", cx)
            .child(
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child("Click a color dot to change it. Deleting a category keeps its tasks."),
            )
            .children(rows.into_iter().map(|(category, open, done)| {
                let id = category.id;
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .py_1()
                    .child(
                        div()
                            .id(("category-color", id))
                            .size_4()
                            .rounded_full()
                            .bg(rgb(category.color))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.store
                                    .update(cx, |store, cx| store.cycle_category_color(id, cx));
                            })),
                    )
                    .child(div().flex_1().text_sm().child(category.name))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.text_muted)
                            .child(format!("{open} open · {done} done")),
                    )
                    .child(
                        icon_button(("category-delete", id), "✕", theme.danger, cx).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.store
                                    .update(cx, |store, cx| store.delete_category(id, cx));
                            }),
                        ),
                    )
            }))
            .child(
                div()
                    .mt_1()
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.background)
                    .text_sm()
                    .child(self.category_input.clone()),
            )
    }
}

impl Render for SettingsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let current = theme.appearance;
        let appearance_button = |id: &'static str, label: &'static str, appearance: Appearance| {
            Button::new(id, label)
                .variant(ButtonVariant::Secondary)
                .selected(current == appearance)
                .on_click(move |_, _, cx| Theme::set(appearance, cx))
        };
        let data_dir = storage::data_dir();

        div()
            .flex()
            .flex_col()
            .gap_4()
            .max_w(px(640.))
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::BOLD)
                    .child("Settings"),
            )
            .child(
                card("Appearance", cx).child(
                    div()
                        .flex()
                        .gap_2()
                        .child(appearance_button("theme-light", "Light", Appearance::Light))
                        .child(appearance_button("theme-dark", "Dark", Appearance::Dark)),
                ),
            )
            .child(self.render_categories(cx))
            .child(
                card("Data", cx)
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.text_muted)
                            .child(format!("Saved automatically to {}", data_dir.display())),
                    )
                    .child(
                        div().flex().child(
                            Button::new("reveal-data", "Show in Finder")
                                .variant(ButtonVariant::Secondary)
                                .on_click(move |_, _, cx| {
                                    let _ = std::fs::create_dir_all(&data_dir);
                                    cx.reveal_path(&data_dir.join("tasks.json"));
                                }),
                        ),
                    ),
            )
    }
}
