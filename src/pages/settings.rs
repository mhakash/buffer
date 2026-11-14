use gpui_kit::component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
    switch::Switch,
    tab::{Tab, TabBar},
    v_flex,
};
use gpui_kit::{
    Context, Entity, FontWeight, SharedString, Subscription, Window, div, prelude::*, px, rgb,
};

use crate::{
    components::{
        card,
        confirm::{Confirmable, confirm, delete_button},
        fx,
    },
    prefs, storage,
    tasks::TaskStore,
    theme::{self, Appearance},
};

const APPEARANCES: [Appearance; 2] = [Appearance::Light, Appearance::Dark];

pub struct SettingsPage {
    store: Entity<TaskStore>,
    category_input: Entity<InputState>,
    name_input: Entity<InputState>,
    armed: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsPage {
    pub fn new(store: Entity<TaskStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let category_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("New category name, then Enter"));
        let name = prefs::get(cx).name.clone();
        let name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Your name, for the greeting")
                .default_value(name)
        });
        let subscriptions = vec![
            cx.subscribe_in(
                &name_input,
                window,
                |_, input, event: &InputEvent, _, cx| {
                    if let InputEvent::Change = event {
                        let name = input.read(cx).value().trim().to_string();
                        if prefs::get(cx).name != name {
                            prefs::update(cx, |p| p.name = name);
                        }
                    }
                },
            ),
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
            name_input,
            armed: None,
            _subscriptions: subscriptions,
        }
    }

    fn render_categories(&self, cx: &mut Context<Self>) -> gpui_kit::Div {
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
                    .child({
                        let key = SharedString::from(format!("category-{id}"));
                        let armed = self.armed.as_ref() == Some(&key);
                        delete_button(("category-delete", id), armed, "Delete category", cx)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if confirm(this, key.clone(), cx) {
                                    this.store
                                        .update(cx, |store, cx| store.delete_category(id, cx));
                                }
                            }))
                    })
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

impl Confirmable for SettingsPage {
    fn armed(&mut self) -> &mut Option<SharedString> {
        &mut self.armed
    }
}

impl Render for SettingsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let current = Appearance::current(cx);
        let ambient = prefs::get(cx).ambient;
        let data_dir = storage::data_dir();

        v_flex()
            .gap_4()
            .max_w(px(640.))
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_3xl()
                            .font_weight(FontWeight::BOLD)
                            .child("Settings"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("Make it yours."),
                    ),
            )
            .child(fx::enter(
                card("Profile", cx).child(
                    div()
                        .px_3()
                        .py_1()
                        .rounded_full()
                        .bg(theme.background)
                        .text_sm()
                        .child(Input::new(&self.name_input).appearance(false)),
                ),
                "settings-profile",
                0,
            ))
            .child(fx::enter(
                card("Appearance", cx)
                    .child(
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
                )
                    .child(
                        h_flex()
                            .justify_between()
                            .pt_2()
                            .child(
                                v_flex()
                                    .child(div().text_sm().child("Ambient motion"))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child("Drifting glow and breathing indicators. Turn off to save battery."),
                                    ),
                            )
                            .child(
                                Switch::new("ambient")
                                    .checked(ambient)
                                    .on_click(|checked: &bool, _, cx| {
                                        let checked = *checked;
                                        prefs::update(cx, |p| p.ambient = checked);
                                    }),
                            ),
                    ),
                "settings-appearance",
                1,
            ))
            .child(fx::enter(self.render_categories(cx), "settings-categories", 2))
            .child(fx::enter(
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
                "settings-data",
                3,
            ))
    }
}
