use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
    tab::{Tab, TabBar},
    v_flex,
};
use gpui_kit::{Context, Entity, FontWeight, Subscription, Window, div, prelude::*, px, rgb};

use crate::{
    components::{card, icon_button},
    storage,
    tasks::TaskStore,
    theme::{self, Appearance},
};

const APPEARANCES: [Appearance; 2] = [Appearance::Light, Appearance::Dark];

pub struct SettingsPage {
    store: Entity<TaskStore>,
    category_input: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsPage {
    pub fn new(store: Entity<TaskStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let category_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("New category name, then Enter"));
        let subscriptions = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &category_input,
                window,
                |this, input, event: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        let name = input.read(cx).value().to_string();
                        this.store
                            .update(cx, |store, cx| store.ensure_category(&name, cx));
                        input.update(cx, |input, cx| input.set_value("", window, cx));
                    }
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
                    .text_color(theme.muted_foreground)
                    .child("Click a color dot to change it. Deleting a category keeps its tasks."),
            )
            .children(rows.into_iter().map(|(category, open, done)| {
                let id = category.id;
                h_flex()
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
                            .text_color(theme.muted_foreground)
                            .child(format!("{open} open · {done} done")),
                    )
                    .child(
                        icon_button(
                            ("category-delete", id),
                            IconName::X,
                            theme.danger,
                            "Delete category",
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.store
                                .update(cx, |store, cx| store.delete_category(id, cx));
                        })),
                    )
            }))
            .child(
                div()
                    .mt_1()
                    .px_2()
                    .rounded_full()
                    .bg(theme.background)
                    .text_sm()
                    .child(Input::new(&self.category_input).appearance(false)),
            )
    }
}

impl Render for SettingsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let current = Appearance::current(cx);
        let data_dir = storage::data_dir();

        v_flex()
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
                    h_flex().child(
                        TabBar::new("appearance")
                            .segmented()
                            .small()
                            .selected_index(
                                APPEARANCES.iter().position(|a| *a == current).unwrap_or(0),
                            )
                            .children(APPEARANCES.map(|a| Tab::new().label(a.label())))
                            .on_click(|ix, _, cx| theme::set(APPEARANCES[*ix], cx)),
                    ),
                ),
            )
            .child(self.render_categories(cx))
            .child(
                card("Data", cx)
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!("Saved automatically to {}", data_dir.display())),
                    )
                    .child(
                        h_flex().child(
                            Button::new("reveal-data")
                                .secondary()
                                .small()
                                .label("Show in Finder")
                                .on_click(move |_, _, cx| {
                                    let _ = std::fs::create_dir_all(&data_dir);
                                    cx.reveal_path(&data_dir.join("tasks.json"));
                                }),
                        ),
                    ),
            )
    }
}
