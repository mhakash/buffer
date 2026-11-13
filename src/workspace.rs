//! Root view of the main window: sidebar navigation, page content and status bar.

use gpui_kit::component::{
    ActiveTheme, h_flex,
    sidebar::{Sidebar, SidebarMenu, SidebarMenuItem},
    v_flex,
};
use gpui_kit::{
    AnyView, Context, Entity, FocusHandle, FontWeight, Subscription, Window, div, prelude::*, px,
};

use crate::{
    APP_NAME,
    actions::{CloseWindow, NewTask, ShowSettings, ShowTasks},
    components::category_tag,
    pages::{Page, SettingsPage, TasksPage},
    storage,
    tasks::TaskStore,
    theme::Appearance,
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
        let tasks = cx.new(|cx| TasksPage::new(store.clone(), window, cx));
        let settings = cx.new(|cx| SettingsPage::new(store.clone(), window, cx));
        // Focusing during window construction is dropped; wait until the window exists.
        cx.defer_in(window, |this, window, cx| {
            this.navigate(Page::Tasks, window, cx)
        });

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
            Page::Tasks => self
                .tasks
                .update(cx, |tasks, cx| tasks.focus_composer(window, cx)),
            Page::Settings => self.focus_handle.focus(window, cx),
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
        let open = self.store.read(cx).open_count();
        let accent = cx.theme().primary;

        Sidebar::new("sidebar")
            .w(px(200.))
            .header(
                div()
                    .px_2()
                    .py_1()
                    .text_lg()
                    .font_weight(FontWeight::BOLD)
                    .child(APP_NAME),
            )
            .child(
                SidebarMenu::new().children(Page::ALL.into_iter().map(|page| {
                    let badge = (page == Page::Tasks && open > 0).then(|| open.to_string());
                    SidebarMenuItem::new(page.label())
                        .icon(page.icon())
                        .active(page == self.active)
                        .on_click(
                            cx.listener(move |this, _, window, cx| this.navigate(page, window, cx)),
                        )
                        .when_some(badge, |this, badge| {
                            this.suffix(move |_, _| category_tag(badge.clone(), accent))
                        })
                })),
            )
    }

    fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let appearance = Appearance::current(cx).label();

        h_flex()
            .flex_none()
            .justify_between()
            .h_6()
            .px_3()
            .bg(theme.status_bar)
            .text_xs()
            .text_color(theme.muted_foreground)
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

        v_flex()
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
            .bg(theme.background)
            .text_color(theme.foreground)
            .font_family(theme.font_family.clone())
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
