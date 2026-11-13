//! Root view of the main window: sidebar navigation, page content and status bar.

use gpui::{
    AnyView, Context, Entity, FocusHandle, FontWeight, Subscription, Window, div, prelude::*, px,
};

use crate::{
    APP_NAME,
    actions::{CloseWindow, NewTask, ShowSettings, ShowTasks},
    components::icon,
    pages::{Page, SettingsPage, TasksPage},
    storage,
    tasks::TaskStore,
    theme::{ActiveTheme, Appearance},
};

pub struct Workspace {
    focus_handle: FocusHandle,
    active: Page,
    store: Entity<TaskStore>,
    tasks: Entity<TasksPage>,
    settings: Entity<SettingsPage>,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = cx.new(TaskStore::load);
        let tasks = cx.new(|cx| TasksPage::new(store.clone(), cx));
        let settings = cx.new(|cx| SettingsPage::new(store.clone(), cx));
        tasks.read(cx).focus_composer(window, cx);

        Self {
            focus_handle: cx.focus_handle(),
            active: Page::Tasks,
            _subscriptions: vec![cx.observe(&store, |_, _, cx| cx.notify())],
            store,
            tasks,
            settings,
        }
    }

    fn navigate(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        self.active = page;
        match page {
            Page::Tasks => self.tasks.read(cx).focus_composer(window, cx),
            Page::Settings => self.focus_handle.focus(window),
        }
        cx.notify();
    }

    fn active_view(&self) -> AnyView {
        match self.active {
            Page::Tasks => self.tasks.clone().into(),
            Page::Settings => self.settings.clone().into(),
        }
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let open = self.store.read(cx).open_count();

        div()
            .flex()
            .flex_col()
            .w(px(200.))
            .h_full()
            .flex_none()
            .bg(theme.sidebar)
            .p_3()
            .gap_1()
            .child(
                div()
                    .px_2()
                    .py_3()
                    .text_lg()
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text)
                    .child(APP_NAME),
            )
            .children(Page::ALL.into_iter().map(|page| {
                let selected = page == self.active;
                let badge = (page == Page::Tasks && open > 0).then(|| open.to_string());
                let color = if selected {
                    theme.text
                } else {
                    theme.text_muted
                };
                div()
                    .id(page.label())
                    .flex()
                    .items_center()
                    .gap_2p5()
                    .px_3()
                    .py_2()
                    .rounded_xl()
                    .text_sm()
                    .cursor_pointer()
                    .text_color(color)
                    .when(selected, |this| this.bg(theme.selected))
                    .hover(|s| s.bg(theme.hover).text_color(theme.text))
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.navigate(page, window, cx)),
                    )
                    .child(icon(page.icon(), color))
                    .child(div().flex_1().child(page.label()))
                    .when_some(badge, |this, badge| {
                        this.child(
                            div()
                                .px_2()
                                .rounded_full()
                                .bg(theme.accent.opacity(0.2))
                                .text_xs()
                                .text_color(theme.accent)
                                .child(badge),
                        )
                    })
            }))
    }

    fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let appearance = match theme.appearance {
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        };

        div()
            .flex()
            .flex_none()
            .justify_between()
            .items_center()
            .h_6()
            .px_3()
            .bg(theme.sidebar)
            .text_xs()
            .text_color(theme.text_muted)
            .child(format!("{APP_NAME} v{}", env!("CARGO_PKG_VERSION")))
            .child(format!(
                "⌘N new task  ·  ⌘⇧T theme ({appearance})  ·  data: {}",
                storage::data_dir().display()
            ))
    }
}

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

        div()
            .id("workspace")
            .track_focus(&self.focus_handle)
            .on_action(
                cx.listener(|this, _: &ShowTasks, window, cx| {
                    this.navigate(Page::Tasks, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &NewTask, window, cx| this.navigate(Page::Tasks, window, cx)),
            )
            .on_action(cx.listener(|this, _: &ShowSettings, window, cx| {
                this.navigate(Page::Settings, window, cx)
            }))
            .on_action(|_: &CloseWindow, window, _| window.remove_window())
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.text)
            .font_family(".SystemUIFont")
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_sidebar(cx))
                    .child(
                        div()
                            .id("content")
                            .flex_1()
                            .min_w_0()
                            .overflow_y_scroll()
                            .child(
                                div()
                                    .w_full()
                                    .max_w(px(860.))
                                    .mx_auto()
                                    .px_8()
                                    .py_8()
                                    .child(self.active_view()),
                            ),
                    ),
            )
            .child(self.render_status_bar(cx))
    }
}
