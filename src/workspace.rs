//! Root view of the main window: ambient backdrop, title bar, sidebar and the active page.

use std::time::Duration;

use chrono::Local;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, Theme, TitleBar, WindowExt, h_flex, v_flex};
use gpui_kit::{
    AnimationExt, AnyView, Context, Entity, FocusHandle, FontWeight, SharedString, SpringAnimation,
    SpringConfig, Subscription, Task, Window, assets::IconName, div, prelude::*, px,
};

use crate::{
    APP_NAME,
    actions::{CloseWindow, NewTask, ShowFocus, ShowSettings, ShowTasks, ToggleTimer},
    components::{
        fx::{self, SPRING},
        icon_button,
    },
    focus::FocusTimer,
    pages::{DetailEvent, FocusPage, OpenTask, PANEL_W, Page, SettingsPage, TaskDetail, TasksPage},
    prefs, quotes,
    tasks::{TaskStore, completions_by_day, streak},
    theme::{self, Appearance},
};

const NAV_ITEM_H: f32 = 40.;
const NAV_GAP: f32 = 4.;
/// Critically damped (no overshoot), ~250 ms: the dim behind dialogs fades in and out.
const SCRIM_SPRING: SpringConfig = SpringConfig::new(300., 34.6, 1.);

pub struct Workspace {
    focus_handle: FocusHandle,
    active: Page,
    store: Entity<TaskStore>,
    timer: Entity<FocusTimer>,
    tasks: Entity<TasksPage>,
    focus: Entity<FocusPage>,
    settings: Entity<SettingsPage>,
    detail: Entity<TaskDetail>,
    _clock: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = cx.new(TaskStore::load);
        let timer = cx.new(FocusTimer::load);
        let tasks = cx.new(|cx| TasksPage::new(store.clone(), window, cx));
        let focus = cx.new(|cx| FocusPage::new(timer.clone(), store.clone(), cx));
        let settings = cx.new(|cx| SettingsPage::new(store.clone(), window, cx));
        let detail = cx.new(|cx| TaskDetail::new(store.clone(), window, cx));

        // Focusing during window construction is dropped; wait until the window exists.
        cx.defer_in(window, |this, window, cx| {
            this.navigate(Page::Tasks, window, cx)
        });
        // Keeps the title-bar clock and greeting current.
        let clock = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(15))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });

        Self {
            focus_handle: cx.focus_handle(),
            active: Page::Tasks,
            _subscriptions: vec![
                cx.observe(&store, |_, _, cx| cx.notify()),
                cx.observe(&timer, |_, _, cx| cx.notify()),
                cx.subscribe_in(&tasks, window, |this, _, e: &OpenTask, window, cx| {
                    this.open_task(e.0, window, cx)
                }),
                cx.subscribe_in(&focus, window, |this, _, e: &OpenTask, window, cx| {
                    this.open_task(e.0, window, cx)
                }),
                cx.subscribe(&detail, |this, _, event: &DetailEvent, cx| match event {
                    DetailEvent::Close => {
                        this.tasks.update(cx, |t, cx| t.set_selected(None, cx));
                        cx.notify();
                    }
                }),
            ],
            _clock: clock,
            store,
            timer,
            tasks,
            focus,
            settings,
            detail,
        }
    }

    fn open_task(&mut self, id: crate::tasks::Id, window: &mut Window, cx: &mut Context<Self>) {
        if self.active == Page::Settings {
            self.navigate(Page::Tasks, window, cx);
        }
        self.detail.update(cx, |d, cx| d.open(id, window, cx));
        self.tasks.update(cx, |t, cx| t.set_selected(Some(id), cx));
        cx.notify();
    }

    fn navigate(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        self.active = page;
        match page {
            Page::Tasks => self
                .tasks
                .update(cx, |tasks, cx| tasks.focus_composer(window, cx)),
            Page::Focus | Page::Settings => self.focus_handle.focus(window, cx),
        }
        cx.notify();
    }

    fn active_view(&self) -> AnyView {
        match self.active {
            Page::Tasks => self.tasks.clone().into(),
            Page::Focus => self.focus.clone().into(),
            Page::Settings => self.settings.clone().into(),
        }
    }

    fn render_title_bar(&self, theme: &Theme) -> impl IntoElement {
        let now = Local::now();
        TitleBar::new()
            .bg(theme.transparent)
            .border_b_0()
            .child(div())
            .child(
                h_flex()
                    .pr_4()
                    .gap_2()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(now.format("%A, %B %-d").to_string())
                    .child(div().size_1().rounded_full().bg(theme.muted_foreground))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.foreground)
                            .child(now.format("%H:%M").to_string()),
                    ),
            )
    }

    fn render_brand(&self, theme: &Theme) -> impl IntoElement {
        // Same 2x2 tile grid as the app icon (scripts/make_icon.swift), scaled from 824px.
        let tile =
            |solid: bool| {
                div().size(px(9.6)).rounded(px(2.4)).bg(theme
                    .primary_foreground
                    .opacity(if solid { 1. } else { 0.55 }))
            };
        let gap = px(2.8);
        let mark = div()
            .flex()
            .items_center()
            .justify_center()
            .size(px(36.))
            .rounded(px(8.))
            .bg(fx::brand_gradient(theme, 150.))
            .shadow(fx::glow(theme.chart_1.opacity(0.45), 18., 0.))
            .child(
                v_flex()
                    .gap(gap)
                    .child(h_flex().gap(gap).child(tile(true)).child(tile(false)))
                    .child(h_flex().gap(gap).child(tile(false)).child(tile(false))),
            );
        h_flex().gap_3().px_2().child(mark).child(
            v_flex()
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::BOLD)
                        .child(APP_NAME),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                ),
        )
    }

    fn render_nav(&self, theme: &Theme, ambient: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let open = self.store.read(cx).open_count();
        let timer = self.timer.read(cx);
        let timer_label = timer.is_active().then(|| timer.clock());
        let timer_running = timer.is_running();
        let active_ix = Page::ALL
            .iter()
            .position(|p| *p == self.active)
            .unwrap_or(0);

        let indicator = div()
            .absolute()
            .left_0()
            .right_0()
            .h(px(NAV_ITEM_H))
            .rounded_xl()
            .bg(theme.secondary_active)
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(10.))
                    .w(px(3.))
                    .h(px(NAV_ITEM_H - 20.))
                    .rounded_full()
                    .bg(fx::brand_gradient(theme, 180.))
                    .shadow(fx::glow(theme.chart_1.opacity(0.6), 10., 0.)),
            )
            .with_spring(
                "nav-indicator",
                SpringAnimation::new(SPRING).to(px(active_ix as f32 * (NAV_ITEM_H + NAV_GAP))),
                |el, top| el.top(top),
            );

        let items = Page::ALL.into_iter().map(|page| {
            let active = page == self.active;
            let suffix: Option<gpui_kit::AnyElement> = match page {
                Page::Tasks if open > 0 => Some(
                    div()
                        .px_2()
                        .rounded_full()
                        .bg(theme.primary.opacity(0.18))
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.primary)
                        .child(open.to_string())
                        .into_any_element(),
                ),
                Page::Focus => timer_label.clone().map(|label| {
                    let dot = div().size_1p5().rounded_full().bg(theme.chart_3);
                    h_flex()
                        .gap_1p5()
                        .px_2()
                        .rounded_full()
                        .bg(theme.chart_3.opacity(0.16))
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.chart_3)
                        .child(fx::pulse(dot, "timer-dot", ambient && timer_running, 0.25))
                        .child(label)
                        .into_any_element()
                }),
                _ => None,
            };
            let group = SharedString::from(format!("nav-{}", page.label()));
            let shortcut = suffix.is_none().then(|| {
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .opacity(0.)
                    .group_hover(group.clone(), |s| s.opacity(1.))
                    .child(page.shortcut())
            });
            h_flex()
                .id(group.clone())
                .group(group)
                .relative()
                .h(px(NAV_ITEM_H))
                .gap_3()
                .px_3()
                .rounded_xl()
                .cursor_pointer()
                .text_sm()
                .when(!active, |this| {
                    this.text_color(theme.muted_foreground)
                        .hover(|s| s.bg(theme.secondary))
                })
                .when(active, |this| {
                    this.text_color(theme.foreground)
                        .font_weight(FontWeight::SEMIBOLD)
                })
                .child(Icon::new(page.icon()).text_color(if active {
                    theme.primary
                } else {
                    theme.muted_foreground
                }))
                .child(div().flex_1().child(page.label()))
                .children(suffix)
                .children(shortcut)
                .on_click(cx.listener(move |this, _, window, cx| this.navigate(page, window, cx)))
        });

        div()
            .relative()
            .child(indicator)
            .child(v_flex().gap(px(NAV_GAP)).children(items))
    }

    fn render_today_card(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let today = Local::now().date_naive();
        let by_day = completions_by_day(store.tasks());
        let done_today = by_day.get(&today).copied().unwrap_or(0);
        let open = store.open_count();
        let streak = streak(&by_day, today);
        let total = done_today + open;
        let fraction = if total == 0 {
            0.
        } else {
            done_today as f32 / total as f32
        };

        v_flex()
            .gap_2()
            .p_3()
            .rounded_2xl()
            .bg(fx::wash(theme, 0.16))
            .child(
                h_flex()
                    .justify_between()
                    .text_xs()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Today"))
                    .child(
                        div()
                            .text_color(theme.muted_foreground)
                            .child(format!("{done_today}/{total}")),
                    ),
            )
            .child(fx::progress_bar("sidebar-progress", fraction, theme))
            .child(
                h_flex()
                    .gap_1()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(
                        Icon::new(IconName::Flame)
                            .xsmall()
                            .text_color(if streak > 0 {
                                theme.chart_4
                            } else {
                                theme.muted_foreground
                            }),
                    )
                    .child(match streak {
                        0 => "Start a streak today".to_string(),
                        1 => "1-day streak".to_string(),
                        n => format!("{n}-day streak"),
                    }),
            )
    }

    fn render_sidebar(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let ambient = prefs::get(cx).ambient;
        let dark = Appearance::current(cx) == Appearance::Dark;

        v_flex()
            .flex_none()
            .w(px(236.))
            .ml_3()
            .mb_3()
            .p_3()
            .gap_5()
            .rounded_2xl()
            .bg(theme.sidebar)
            .border_1()
            .border_color(theme.border)
            .child(self.render_brand(theme))
            .child(self.render_nav(theme, ambient, cx))
            .child(div().flex_1())
            .child(self.render_today_card(theme, cx))
            .child(
                h_flex()
                    .gap_2()
                    .items_start()
                    .px_1()
                    .child(
                        Icon::new(IconName::Quote)
                            .xsmall()
                            .flex_none()
                            .mt_0p5()
                            .text_color(theme.chart_1),
                    )
                    .child(
                        div()
                            .text_xs()
                            .italic()
                            .text_color(theme.muted_foreground)
                            .child(quotes::today()),
                    ),
            )
            .child(
                h_flex()
                    .justify_between()
                    .px_1()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("⌘N new · ⌘⇧F focus")
                    .child(
                        icon_button(
                            "toggle-theme",
                            if dark { IconName::Sun } else { IconName::Moon },
                            theme.muted_foreground,
                            "Toggle light/dark (⌘⇧T)",
                        )
                        .on_click(|_, _, cx| theme::toggle(cx)),
                    ),
            )
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let ambient = prefs::get(cx).ambient;
        let page_id = self.active as u64;
        let dialog_open = window.has_active_dialog(cx);
        // Hide the panel on Settings, and drop it if its task was deleted.
        let open = self.detail.read(cx).task_id();
        if open.is_some_and(|id| self.store.read(cx).task(id).is_none()) {
            self.detail.update(cx, |d, cx| d.close(cx));
        }
        let show_detail = self.active != Page::Settings && self.detail.read(cx).task_id().is_some();
        // Dock the panel beside the page only if the page keeps room to breathe.
        const SIDEBAR_W: f32 = 236. + 12.;
        const MIN_CONTENT_W: f32 = 480.;
        let docked = window.viewport_size().width >= px(SIDEBAR_W + MIN_CONTENT_W + PANEL_W + 12.);

        let page = div()
            .w_full()
            .max_w(px(920.))
            .mx_auto()
            .when(show_detail && docked, |this| this.px_6())
            .when(!(show_detail && docked), |this| this.px_10())
            .pt_4()
            .pb_12()
            .child(self.active_view());

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
            .on_action(
                cx.listener(|this, _: &ShowFocus, window, cx| {
                    this.navigate(Page::Focus, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &ShowSettings, window, cx| {
                this.navigate(Page::Settings, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleTimer, _, cx| {
                this.timer.update(cx, |timer, cx| timer.toggle(cx));
            }))
            .on_action(|_: &CloseWindow, window, _| window.remove_window())
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(theme.background)
            .text_color(theme.foreground)
            .font_family(theme.font_family.clone())
            .child(fx::aurora(ambient, cx))
            .child(
                v_flex()
                    .absolute()
                    .inset_0()
                    .size_full()
                    .child(self.render_title_bar(&theme))
                    .child(
                        h_flex()
                            .relative()
                            .items_stretch()
                            .flex_1()
                            .min_h_0()
                            .child(self.render_sidebar(&theme, cx))
                            .child(
                                div()
                                    .id("content")
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_y_scroll()
                                    .child(fx::enter(page, ("page", page_id), 0)),
                            )
                            .when(show_detail && docked, |this| {
                                this.child(
                                    div().flex_none().mr_3().mb_3().child(self.detail.clone()),
                                )
                            })
                            // Too narrow to share: float the panel over the content instead.
                            .when(show_detail && !docked, |this| {
                                this.child(
                                    div()
                                        .absolute()
                                        .top_0()
                                        .bottom_3()
                                        .right_3()
                                        .rounded_2xl()
                                        .bg(theme.background)
                                        .shadow(vec![gpui_kit::BoxShadow {
                                            color: gpui_kit::black().opacity(0.25),
                                            offset: gpui_kit::point(px(0.), px(12.)),
                                            blur_radius: px(48.),
                                            spread_radius: px(0.),
                                            inset: false,
                                        }])
                                        .child(self.detail.clone()),
                                )
                            }),
                    ),
            )
            // Our own dim layer under kit dialogs: the kit's backdrop can't fade (it paints in a
            // deferred pass), so theme `overlay` is transparent and this fades instead.
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .bg(if theme.is_dark() {
                        theme.background.opacity(0.72)
                    } else {
                        theme.foreground.opacity(0.28)
                    })
                    .with_spring(
                        "dialog-scrim",
                        SpringAnimation::new(SCRIM_SPRING).to(if dialog_open { 1. } else { 0. }),
                        |el, v| el.opacity(v.clamp(0., 1.)),
                    ),
            )
    }
}
