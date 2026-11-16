use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Icon, Sizable,
    button::{Button, ButtonRounded, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
    switch::Switch,
    tab::{Tab, TabBar},
    v_flex,
};
use gpui_kit::{
    Context, Entity, Focusable, FontWeight, SharedString, Subscription, Window, div, prelude::*,
    px, rgb,
};

use crate::{
    components::{card, confirm::confirm_delete, fx, icon_button},
    prefs, storage,
    tasks::TaskStore,
    theme::{self, Appearance},
    workspaces,
};

const APPEARANCES: [Appearance; 2] = [Appearance::Light, Appearance::Dark];

pub struct SettingsPage {
    store: Entity<TaskStore>,
    category_input: Entity<InputState>,
    name_input: Entity<InputState>,
    /// Renames the active workspace as you type.
    workspace_name: Entity<InputState>,
    new_workspace: Entity<InputState>,
    new_workspace_error: Option<SharedString>,
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
        let active = workspaces::get(cx).active().name.clone();
        let workspace_name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Workspace name")
                .default_value(active)
        });
        let new_workspace =
            cx.new(|cx| InputState::new(window, cx).placeholder("New workspace name, then Enter"));
        let subscriptions = vec![
            cx.subscribe_in(
                &workspace_name,
                window,
                |_, input, event: &InputEvent, _, cx| {
                    if let InputEvent::Change = event {
                        let name = input.read(cx).value().to_string();
                        let id = workspaces::get(cx).active().id;
                        workspaces::rename(id, &name, cx);
                    }
                },
            ),
            cx.subscribe_in(
                &new_workspace,
                window,
                |this, input, event: &InputEvent, _, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        let name = input.read(cx).value().trim().to_string();
                        if name.is_empty() {
                            return;
                        }
                        // Success rebuilds this page for the new workspace.
                        if let Err(err) = workspaces::create(&name, cx) {
                            this.new_workspace_error =
                                Some(format!("Couldn't create workspace: {err:#}").into());
                            cx.notify();
                        }
                    }
                },
            ),
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
            workspace_name,
            new_workspace,
            new_workspace_error: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn focus_new_workspace(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_workspace.focus_handle(cx).focus(window, cx);
    }

    fn render_workspaces(&self, cx: &mut Context<Self>) -> gpui_kit::Div {
        let theme = cx.theme().clone();
        let registry = workspaces::get(cx);
        let active = registry.active().id;
        let entries = registry.entries().to_vec();

        card("Workspaces", cx)
            .child(div().text_xs().text_color(theme.muted_foreground).child(
                "Each workspace keeps its tasks, notes and focus history in its own folder.",
            ))
            .children(entries.into_iter().map(|entry| {
                let id = entry.id;
                let is_active = id == active;
                let root = entry.root();
                let group = SharedString::from(format!("workspace-row-{id}"));
                let name = if is_active {
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(Input::new(&self.workspace_name).appearance(false).px_0())
                        .into_any_element()
                } else {
                    div().text_sm().child(entry.name.clone()).into_any_element()
                };
                let reveal = {
                    let root = root.clone();
                    icon_button(
                        ("workspace-reveal", id),
                        IconName::FolderOpen,
                        theme.muted_foreground,
                        "Show in Finder",
                    )
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        cx.reveal_path(&root);
                    })
                };
                let remove = (!is_active && !entry.is_default()).then(|| {
                    let (name, root) = (entry.name.clone(), root.clone());
                    icon_button(
                        ("workspace-remove", id),
                        IconName::X,
                        theme.danger,
                        "Remove from list",
                    )
                    .on_click(cx.listener(move |_, _, window, cx| {
                        cx.stop_propagation();
                        confirm_delete(
                            window,
                            cx,
                            "Remove workspace?",
                            format!(
                                "“{name}” leaves this list. Its files stay in {}.",
                                root.display()
                            ),
                            move |_, cx| workspaces::remove(id, cx),
                        );
                    }))
                });
                h_flex()
                    .id(("workspace-row", id))
                    .group(group.clone())
                    .gap_3()
                    .px_2()
                    .py_1()
                    .rounded_xl()
                    .when(is_active, |this| this.bg(theme.secondary_active))
                    .when(!is_active, |this| {
                        this.cursor_pointer()
                            .hover(|s| s.bg(theme.secondary))
                            .on_click(move |_, _, cx| workspaces::switch(id, cx))
                    })
                    .child(
                        Icon::new(if is_active {
                            IconName::Check
                        } else {
                            IconName::FolderOpen
                        })
                        .xsmall()
                        .flex_none()
                        .text_color(if is_active {
                            theme.primary
                        } else {
                            theme.muted_foreground
                        }),
                    )
                    .child(
                        v_flex().flex_1().min_w_0().child(name).child(
                            div()
                                .text_xs()
                                .truncate()
                                .text_color(theme.muted_foreground)
                                .child(tilde(&root)),
                        ),
                    )
                    .child(
                        h_flex()
                            .flex_none()
                            .opacity(0.)
                            .group_hover(group, |s| s.opacity(1.))
                            .child(reveal)
                            .children(remove),
                    )
            }))
            .child(
                h_flex()
                    .mt_1()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .px_2()
                            .rounded_full()
                            .bg(theme.background)
                            .text_sm()
                            .child(Input::new(&self.new_workspace).appearance(false)),
                    )
                    .child(
                        Button::new("open-workspace-folder")
                            .secondary()
                            .small()
                            .rounded(ButtonRounded::Size(px(999.)))
                            .icon(IconName::FolderPlus)
                            .label("Open folder…")
                            .on_click(|_, _, cx| workspaces::prompt_open_folder(cx)),
                    ),
            )
            .children(
                self.new_workspace_error
                    .clone()
                    .map(|err| div().text_xs().text_color(theme.danger).child(err)),
            )
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
                    .filter(|t| t.categories.contains(&category.id))
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
                    .child(div().flex_1().text_sm().child(category.name.clone()))
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
                        .on_click({
                            let name = category.name.clone();
                            cx.listener(move |this, _, window, cx| {
                                let store = this.store.clone();
                                confirm_delete(
                                    window,
                                    cx,
                                    "Delete category?",
                                    format!("“{name}” will be removed. Its tasks are kept."),
                                    move |_, cx| {
                                        store.update(cx, |s, cx| s.delete_category(id, cx));
                                    },
                                );
                            })
                        }),
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
            .child(fx::enter(self.render_workspaces(cx), "settings-workspaces", 2))
            .child(fx::enter(self.render_categories(cx), "settings-categories", 3))
            .child(fx::enter(
                card("Data", cx)
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!(
                                "This workspace saves automatically to {}",
                                data_dir.display()
                            )),
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
                4,
            ))
    }
}

/// `~/…` for paths under the home folder; workspace roots are long otherwise.
fn tilde(path: &std::path::Path) -> String {
    match dirs::home_dir().and_then(|home| path.strip_prefix(home).ok().map(|p| p.to_owned())) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}
