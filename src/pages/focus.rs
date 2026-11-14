//! Focus page: pomodoro ring, controls, session length, next task and today's focus stats.

use gpui_kit::component::{ActiveTheme, Icon, Sizable, Theme, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, Entity, EventEmitter, FontWeight, Hsla, Subscription, Window,
    assets::IconName, div, prelude::*, px, rgb,
};

use super::OpenTask;
use crate::{
    components::{chip, fx, icon_button},
    focus::{FocusTimer, LENGTHS, Phase},
    prefs,
    tasks::{Task, TaskStore},
};

const RING: f32 = 300.;

pub struct FocusPage {
    timer: Entity<FocusTimer>,
    store: Entity<TaskStore>,
    _subscriptions: Vec<Subscription>,
}

impl FocusPage {
    pub fn new(
        timer: Entity<FocusTimer>,
        store: Entity<TaskStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            _subscriptions: vec![
                cx.observe(&timer, |_, _, cx| cx.notify()),
                cx.observe(&store, |_, _, cx| cx.notify()),
            ],
            timer,
            store,
        }
    }

    fn colors(phase: Phase, theme: &Theme) -> (Hsla, Hsla) {
        if phase.is_break() {
            (theme.chart_5, theme.chart_2)
        } else {
            fx::brand_pair(theme)
        }
    }

    fn render_dial(&self, theme: &Theme, ambient: bool, cx: &mut Context<Self>) -> AnyElement {
        let timer = self.timer.read(cx);
        let phase = timer.phase();
        let running = timer.is_running();
        let (from, to) = Self::colors(phase, theme);
        let progress = timer.progress();
        let clock = timer.clock();
        let total_min = timer.total().as_secs() / 60;

        // Soft halo behind the dial; breathes while running.
        let halo = div()
            .absolute()
            .top(px(RING / 2. - 4.))
            .left(px(RING / 2. - 4.))
            .size(px(8.))
            .rounded_full()
            .shadow(fx::glow(from.opacity(0.35), 140., 70.));
        let halo = fx::pulse(halo, "focus-halo", ambient && running, 0.35);

        let phase_icon = if phase.is_break() {
            IconName::Coffee
        } else {
            IconName::Brain
        };

        fx::ring_box(RING)
            .child(halo)
            .child(fx::ring(
                "focus-ring",
                progress,
                RING,
                14.,
                (from, to),
                theme,
            ))
            .child(
                v_flex()
                    .items_center()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_1p5()
                            .px_3()
                            .py_0p5()
                            .rounded_full()
                            .bg(from.opacity(0.16))
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(from)
                            .child(Icon::new(phase_icon).xsmall().text_color(from))
                            .child(phase.label()),
                    )
                    .child(
                        div()
                            .text_size(px(68.))
                            .font_weight(FontWeight::BOLD)
                            .line_height(px(80.))
                            .child(clock),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(if running {
                                format!("of {total_min} min · stay with it")
                            } else {
                                format!("{total_min} min session")
                            }),
                    ),
            )
            .into_any_element()
    }

    fn render_controls(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let timer = self.timer.read(cx);
        let running = timer.is_running();
        let active = timer.is_active();
        let (from, to) = Self::colors(timer.phase(), theme);

        let play = h_flex()
            .id("focus-toggle")
            .h(px(52.))
            .px_8()
            .gap_2()
            .rounded_full()
            .cursor_pointer()
            .bg(gpui_kit::linear_gradient(
                90.,
                gpui_kit::linear_color_stop(from, 0.),
                gpui_kit::linear_color_stop(to, 1.),
            ))
            .shadow(fx::glow(from.opacity(0.45), 24., 0.))
            .hover(|s| s.opacity(0.92))
            .active(|s| s.opacity(0.8))
            .text_color(theme.primary_foreground)
            .font_weight(FontWeight::SEMIBOLD)
            .child(
                Icon::new(if running {
                    IconName::Pause
                } else {
                    IconName::Play
                })
                .text_color(theme.primary_foreground),
            )
            .child(match (running, active) {
                (true, _) => "Pause",
                (false, true) => "Resume",
                (false, false) => "Start",
            })
            .on_click(cx.listener(|this, _, _, cx| {
                this.timer.update(cx, |t, cx| t.toggle(cx));
            }));

        h_flex()
            .gap_3()
            .items_center()
            .child(
                icon_button(
                    "focus-reset",
                    IconName::RotateCcw,
                    theme.muted_foreground,
                    "Reset",
                )
                .large()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.timer.update(cx, |t, cx| t.reset(cx));
                })),
            )
            .child(play)
            .child(
                icon_button(
                    "focus-skip",
                    IconName::SkipForward,
                    theme.muted_foreground,
                    "Skip to next phase",
                )
                .large()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.timer.update(cx, |t, cx| t.skip(cx));
                })),
            )
    }

    fn render_lengths(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.timer.read(cx).focus_minutes();
        h_flex().gap_1().children(LENGTHS.into_iter().map(|m| {
            chip(
                ("focus-len", m as u64),
                format!("{m} min"),
                None,
                m == current,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.timer.update(cx, |t, cx| t.set_focus_minutes(m, cx));
            }))
        }))
    }

    fn render_next(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        let next: Option<Task> = {
            let mut open: Vec<&Task> = self
                .store
                .read(cx)
                .tasks()
                .iter()
                .filter(|t| !t.is_done())
                .collect();
            open.sort_by_key(|t| (t.priority, t.sort_order, t.created_at));
            open.first().map(|t| (*t).clone())
        };
        let next_id = next.as_ref().map(|t| t.id);

        h_flex()
            .id("focus-next")
            .when_some(next_id, |this, id| {
                this.cursor_pointer()
                    .hover(|s| s.bg(theme.secondary_active))
                    .on_click(cx.listener(move |_, _, _, cx| cx.emit(OpenTask(id))))
            })
            .w_full()
            .gap_3()
            .p_4()
            .rounded_2xl()
            .bg(theme.group_box)
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_none()
                    .size(px(36.))
                    .rounded_xl()
                    .bg(theme.chart_1.opacity(0.16))
                    .child(Icon::new(IconName::Target).text_color(theme.chart_1)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Up next"),
                    )
                    .child(
                        div().text_sm().font_weight(FontWeight::SEMIBOLD).child(
                            next.as_ref()
                                .map_or("Nothing open — enjoy the calm.".to_string(), |t| {
                                    t.title.clone()
                                }),
                        ),
                    ),
            )
            .when_some(next.as_ref(), |this, task| {
                let color: Hsla = rgb(task.priority.color()).into();
                this.child(
                    div()
                        .flex_none()
                        .px_2()
                        .rounded_full()
                        .bg(color.opacity(0.16))
                        .text_xs()
                        .text_color(color)
                        .child(task.priority.label()),
                )
            })
    }

    fn render_stats(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let timer = self.timer.read(cx);
        let tiles = [
            (
                IconName::Zap,
                theme.chart_1,
                timer.sessions_today().to_string(),
                "sessions today",
            ),
            (
                IconName::Timer,
                theme.chart_2,
                format!("{}m", timer.minutes_today()),
                "focused today",
            ),
            (
                IconName::Trophy,
                theme.chart_4,
                timer.total_sessions().to_string(),
                "all-time sessions",
            ),
        ];
        h_flex()
            .w_full()
            .gap_3()
            .children(
                tiles
                    .into_iter()
                    .enumerate()
                    .map(|(ix, (icon, color, value, label))| {
                        fx::enter(
                            stat_tile(icon, color, value, label, theme),
                            ("focus-stat", ix as u64),
                            ix + 2,
                        )
                    }),
            )
    }
}

pub fn stat_tile(
    icon: IconName,
    color: Hsla,
    value: String,
    label: &'static str,
    theme: &Theme,
) -> gpui_kit::Div {
    h_flex()
        .flex_1()
        .gap_3()
        .p_4()
        .rounded_2xl()
        .bg(theme.group_box)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .flex_none()
                .size(px(36.))
                .rounded_xl()
                .bg(color.opacity(0.16))
                .child(Icon::new(icon).text_color(color)),
        )
        .child(
            v_flex()
                .child(div().text_xl().font_weight(FontWeight::BOLD).child(value))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(label),
                ),
        )
}

impl EventEmitter<OpenTask> for FocusPage {}

impl Render for FocusPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let ambient = prefs::get(cx).ambient;

        v_flex()
            .gap_6()
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_3xl()
                            .font_weight(FontWeight::BOLD)
                            .child("Focus"),
                    )
                    .child(div().text_sm().text_color(theme.muted_foreground).child(
                        "Deep work, one session at a time. ⌘⇧F starts or pauses from anywhere.",
                    )),
            )
            .child(
                v_flex()
                    .items_center()
                    .gap_6()
                    .py_8()
                    .rounded_2xl()
                    .bg(fx::wash(&theme, 0.10))
                    .border_1()
                    .border_color(theme.border)
                    .child(self.render_dial(&theme, ambient, cx))
                    .child(self.render_controls(&theme, cx))
                    .child(self.render_lengths(cx)),
            )
            .child(fx::enter(self.render_next(&theme, cx), "focus-next", 1))
            .child(self.render_stats(&theme, cx))
    }
}
