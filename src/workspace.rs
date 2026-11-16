//! Root view of the main window: ambient backdrop, title bar, sidebar and the active page.

use std::{path::PathBuf, time::Duration};

use chrono::Local;
use gpui_kit::component::{
    ActiveTheme, Icon, Sizable, Theme, TitleBar, WindowExt, h_flex,
    menu::{DropdownMenu, PopupMenuItem},
    sidebar::SidebarHeader,
    v_flex,
};
use gpui_kit::{
    AnimationExt, AnyView, Context, DragMoveEvent, Entity, FocusHandle, FontWeight, SharedString,
    SpringAnimation, SpringConfig, Subscription, Task, Window, assets::IconName, div, prelude::*,
    px,
};

use crate::{
    APP_NAME,
    actions::{
        CloseWindow, NewNote, NewTask, NewWorkspace, ShowFocus, ShowNotes, ShowSettings, ShowTasks,
        ToggleTimer,
    },
    components::{
        fx::{self, SPRING},
        icon_button, lightbox,
    },
    focus::FocusTimer,
    pages::{
        DetailEvent, FocusPage, NotesPage, OpenTask, PANEL_MIN_W, Page, ResizePanel, SettingsPage,
        TaskDetail, TasksPage,
    },
    prefs, quotes,
    tasks::{TaskStore, completions_by_day, streak},
    theme::{self, Appearance},
    workspaces,
};

const NAV_ITEM_H: f32 = 40.;
const NAV_GAP: f32 = 4.;
const SIDEBAR_W: f32 = 236. + 12.;
/// The list keeps at least this much room before the detail panel floats over it instead.
const MIN_CONTENT_W: f32 = 480.;
/// Right/bottom margin around the detail panel (`mr_3`).
const EDGE: f32 = 12.;
/// Critically damped (no overshoot), ~250 ms: the dim behind dialogs fades in and out.
const SCRIM_SPRING: SpringConfig = SpringConfig::new(300., 34.6, 1.);

pub struct Workspace {
    focus_handle: FocusHandle,
    active: Page,
    store: Entity<TaskStore>,
    timer: Entity<FocusTimer>,
    tasks: Entity<TasksPage>,
    focus: Entity<FocusPage>,
    notes: Entity<NotesPage>,
    settings: Entity<SettingsPage>,
    detail: Entity<TaskDetail>,
    /// Root of the workspace the stores were loaded from.
    root: PathBuf,
    _clock: Task<()>,
    _registry: Subscription,
    _subscriptions: Vec<Subscription>,
}

/// Everything that belongs to one workspace; rebuilt when the active workspace changes.
struct Content {
    store: Entity<TaskStore>,
    timer: Entity<FocusTimer>,
    tasks: Entity<TasksPage>,
    focus: Entity<FocusPage>,
    notes: Entity<NotesPage>,
    settings: Entity<SettingsPage>,
    detail: Entity<TaskDetail>,
    subscriptions: Vec<Subscription>,
}

impl Content {
    fn load(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        crate::blocks::load(cx);
        let store = cx.new(TaskStore::load);
        let timer = cx.new(FocusTimer::load);
        let tasks = cx.new(|cx| TasksPage::new(store.clone(), window, cx));
        let focus = cx.new(|cx| FocusPage::new(timer.clone(), store.clone(), cx));
        let notes = cx.new(|cx| NotesPage::new(store.clone(), window, cx));
        let settings = cx.new(|cx| SettingsPage::new(store.clone(), window, cx));
        let detail = cx.new(|cx| TaskDetail::new(store.clone(), window, cx));
        let subscriptions = vec![
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
                DetailEvent::Layout => cx.notify(),
            }),
        ];
        Self {
            store,
            timer,
            tasks,
            focus,
            notes,
            settings,
            detail,
            subscriptions,
        }
    }
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let content = Content::load(window, cx);
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
        let registry = cx.observe_global_in::<workspaces::Registry>(window, |this, window, cx| {
            let root = workspaces::get(cx).active().root();
            if root != this.root {
                this.root = root;
                this.reload(window, cx);
            }
        });

        Self {
            focus_handle: cx.focus_handle(),
            active: Page::Tasks,
            root: workspaces::get(cx).active().root(),
            store: content.store,
            timer: content.timer,
            tasks: content.tasks,
            focus: content.focus,
            notes: content.notes,
            settings: content.settings,
            detail: content.detail,
            _clock: clock,
            _registry: registry,
            _subscriptions: content.subscriptions,
        }
    }

    fn set_content(&mut self, content: Content) {
        self.store = content.store;
        self.timer = content.timer;
        self.tasks = content.tasks;
        self.focus = content.focus;
        self.notes = content.notes;
        self.settings = content.settings;
        self.detail = content.detail;
        self._subscriptions = content.subscriptions;
    }

    /// The active workspace changed: drop every store and page and load them from the new root.
    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let content = Content::load(window, cx);
        self.set_content(content);
        self.navigate(self.active, window, cx);
    }

    fn open_task(&mut self, id: crate::tasks::Id, window: &mut Window, cx: &mut Context<Self>) {
        if !self.active.shows_task_detail() {
            self.navigate(Page::Tasks, window, cx);
        }
        self.detail.update(cx, |d, cx| d.open(id, window, cx));
        self.tasks.update(cx, |t, cx| t.set_selected(Some(id), cx));
        cx.notify();
    }

    fn navigate(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        self.active = page;
        // A task open as a full page covers the page; leaving it closes the task.
        if self.detail.read(cx).is_full_page() {
            self.detail.update(cx, |d, cx| {
                d.set_full_page(false, cx);
                d.close(cx);
            });
        }
        match page {
            Page::Tasks => self
                .tasks
                .update(cx, |tasks, cx| tasks.focus_composer(window, cx)),
            Page::Notes => self.notes.update(cx, |notes, cx| notes.focus(window, cx)),
            Page::Focus | Page::Settings => self.focus_handle.focus(window, cx),
        }
        cx.notify();
    }

    fn active_view(&self) -> AnyView {
        match self.active {
            Page::Tasks => self.tasks.clone().into(),
            Page::Focus => self.focus.clone().into(),
            Page::Notes => self.notes.clone().into(),
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

    fn render_brand(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
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
        let name = workspaces::get(cx).active().name.clone();
        let mut header = SidebarHeader::new();
        header.interactivity().element_id = Some("workspace-switcher".into());
        // Notion-style: the brand block is the workspace switcher.
        header
            .group("workspace-switcher")
            .gap_3()
            .p_1()
            .rounded_xl()
            .cursor_pointer()
            .child(mark)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::BOLD)
                            .child(APP_NAME),
                    )
                    .child(
                        h_flex()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(div().min_w_0().truncate().child(name))
                            .child(
                                div()
                                    .flex_none()
                                    .child(format!(" · v{}", env!("CARGO_PKG_VERSION"))),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .opacity(0.)
                    .group_hover("workspace-switcher", |s| s.opacity(1.))
                    .child(
                        Icon::new(IconName::ChevronsUpDown)
                            .xsmall()
                            .text_color(theme.muted_foreground),
                    ),
            )
            .dropdown_menu(|mut menu, _, cx| {
                let registry = workspaces::get(cx);
                let active = registry.active().id;
                for entry in registry.entries().to_vec() {
                    let id = entry.id;
                    menu = menu.item(
                        PopupMenuItem::new(entry.name)
                            .checked(id == active)
                            .on_click(move |_, _, cx| workspaces::switch(id, cx)),
                    );
                }
                menu.separator()
                    .menu_with_icon("New workspace…", IconName::Plus, Box::new(NewWorkspace))
                    .item(
                        PopupMenuItem::new("Open folder…")
                            .icon(IconName::FolderOpen)
                            .on_click(|_, _, cx| workspaces::prompt_open_folder(cx)),
                    )
                    .min_w(px(220.))
            })
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
            .child(self.render_brand(theme, cx))
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
                            .flex_1()
                            .min_w_0()
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
                    .child("⌘N task · ⌘⇧N note")
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
        // Hide the panel on pages without tasks, and drop it if its task was deleted.
        let open = self.detail.read(cx).task_id();
        if open.is_some_and(|id| self.store.read(cx).task(id).is_none()) {
            self.detail.update(cx, |d, cx| d.close(cx));
        }
        let show_detail =
            self.active.shows_task_detail() && self.detail.read(cx).task_id().is_some();
        let full_page = show_detail && self.detail.read(cx).is_full_page();
        let notes_page = self.active == Page::Notes;
        let viewport_w = f32::from(window.viewport_size().width);
        let panel_w = clamp_panel_width(prefs::get(cx).detail_width, viewport_w);
        // Dock the panel beside the page only if the page keeps room to breathe.
        let docked = viewport_w >= SIDEBAR_W + MIN_CONTENT_W + panel_w + EDGE;

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
            .capture_key_down(lightbox::capture_key)
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
            .on_action(
                cx.listener(|this, _: &ShowNotes, window, cx| {
                    this.navigate(Page::Notes, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &NewNote, window, cx| {
                this.navigate(Page::Notes, window, cx);
                this.notes.update(cx, |notes, cx| notes.create(window, cx));
            }))
            .on_action(cx.listener(|this, _: &ShowSettings, window, cx| {
                this.navigate(Page::Settings, window, cx)
            }))
            .on_action(cx.listener(|this, _: &NewWorkspace, window, cx| {
                this.navigate(Page::Settings, window, cx);
                this.settings
                    .update(cx, |s, cx| s.focus_new_workspace(window, cx));
            }))
            .on_action(cx.listener(|this, _: &ToggleTimer, _, cx| {
                this.timer.update(cx, |timer, cx| timer.toggle(cx));
            }))
            .on_action(|_: &CloseWindow, window, _| window.remove_window())
            // Dragging the detail panel's left edge: width follows the pointer; saved on drop.
            .on_drag_move::<ResizePanel>(cx.listener(
                |_, event: &DragMoveEvent<ResizePanel>, window, cx| {
                    let viewport_w = f32::from(window.viewport_size().width);
                    let width = viewport_w - f32::from(event.event.position.x) - EDGE;
                    let width = clamp_panel_width(width, viewport_w);
                    if (prefs::get(cx).detail_width - width).abs() >= 1. {
                        prefs::set_detail_width(cx, width);
                        cx.notify();
                    }
                },
            ))
            .on_drop::<ResizePanel>(|_, _, cx| prefs::save(cx))
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
                            // Notes manages its own panes and scrolling across the whole area.
                            .when(notes_page, |this| {
                                this.child(div().flex_1().min_w_0().mr_3().mb_3().child(fx::enter(
                                    div().size_full().child(self.notes.clone()),
                                    ("page", page_id),
                                    0,
                                )))
                            })
                            .when(!full_page && !notes_page, |this| {
                                this.child(
                                    div()
                                        .id("content")
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_y_scroll()
                                        .child(fx::enter(page, ("page", page_id), 0)),
                                )
                            })
                            .when(full_page, |this| {
                                this.child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .ml_3()
                                        .mr_3()
                                        .mb_3()
                                        .child(self.detail.clone()),
                                )
                            })
                            .when(show_detail && !full_page && docked, |this| {
                                this.child(
                                    div()
                                        .flex_none()
                                        .w(px(panel_w))
                                        .mr_3()
                                        .mb_3()
                                        .child(self.detail.clone()),
                                )
                            })
                            // Too narrow to share: float the panel over the content instead.
                            .when(show_detail && !full_page && !docked, |this| {
                                this.child(
                                    div()
                                        .absolute()
                                        .top_0()
                                        .bottom_3()
                                        .right_3()
                                        .w(px(panel_w))
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
            .children(lightbox::render(window, cx))
    }
}

/// Keeps the side panel between its minimum and what leaves the list usable (or, when the
/// window is too narrow to dock, what still fits beside the sidebar).
fn clamp_panel_width(width: f32, viewport_w: f32) -> f32 {
    let docked_max = viewport_w - SIDEBAR_W - MIN_CONTENT_W - EDGE;
    let max = if docked_max >= PANEL_MIN_W {
        docked_max
    } else {
        viewport_w - SIDEBAR_W - 2. * EDGE
    };
    width.min(max).max(PANEL_MIN_W)
}
