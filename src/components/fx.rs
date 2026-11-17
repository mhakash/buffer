//! Visual effects: ambient glow, gradients, progress ring, entrance animations.
//!
//! Colors come from the theme's `chart.1`–`chart.5` slots (`themes/buffer.json`), so the whole
//! look follows the palette. Every animation goes through `with_animation`/`with_spring`, which
//! honour `App::reduce_motion`.

use std::{f32::consts::TAU, time::Duration};

use gpui_kit::component::{ActiveTheme, Theme};
use gpui_kit::{
    Animation, AnimationExt, AnyElement, App, Background, BoxShadow, Div, ElementId, Hsla,
    IntoElement, ParentElement, PathBuilder, SpringAnimation, SpringConfig, Styled, canvas, div,
    ease_out_quint, linear_color_stop, linear_gradient, point, px,
};

/// Snappy, slightly bouncy spring for UI that tracks state (indicators, rings, bars).
pub const SPRING: SpringConfig = SpringConfig::new(220., 22., 1.);

/// The brand gradient (chart.1 → chart.3) at `angle` degrees.
pub fn brand_gradient(theme: &Theme, angle: f32) -> Background {
    linear_gradient(
        angle,
        linear_color_stop(theme.chart_1, 0.),
        linear_color_stop(theme.chart_3, 1.),
    )
}

/// A faint wash of the brand gradient for large surfaces.
pub fn wash(theme: &Theme, strength: f32) -> Background {
    linear_gradient(
        135.,
        linear_color_stop(theme.chart_1.opacity(strength), 0.),
        linear_color_stop(theme.chart_2.opacity(strength * 0.5), 1.),
    )
}

/// Soft colored glow around an element (box-shadow with a large blur).
pub fn glow(color: Hsla, blur: f32, spread: f32) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color,
        offset: point(px(0.), px(0.)),
        blur_radius: px(blur),
        spread_radius: px(spread),
        inset: false,
    }]
}

/// Whether ambient (looping) motion should run: the user's setting, and only while the window is
/// focused — every looping frame re-renders its view, so a window in the background stays still.
pub fn ambient(cx: &App) -> bool {
    crate::prefs::get(cx).ambient
        && !cx.reduce_motion()
        && cx.try_global::<WindowActive>().is_none_or(|a| a.0)
}

/// Whether the main window is focused; set by `Workspace` on activation changes.
pub struct WindowActive(pub bool);
impl gpui_kit::Global for WindowActive {}

/// Full-window layer of slowly drifting colored light, drawn at `seconds` of drift
/// (`components::aurora::Aurora` advances it).
pub fn aurora(seconds: f32, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let strength = if theme.is_dark() { 0.30 } else { 0.22 };
    // (color, anchor x/y as fraction of window, drift radius px, phase offset)
    let blobs = [
        (theme.chart_1, 0.18, 0.12, 60., 0.0),
        (theme.chart_2, 0.88, 0.30, 80., 0.33),
        (theme.chart_3, 0.55, 0.95, 70., 0.66),
    ];
    div()
        .absolute()
        .inset_0()
        .size_full()
        .overflow_hidden()
        .children(
            blobs
                .into_iter()
                .enumerate()
                .map(|(ix, (color, ax, ay, drift, phase))| {
                    let period = (28 + ix * 7) as f32;
                    let a = (seconds / period + phase) * TAU;
                    div()
                        .absolute()
                        .left(gpui_kit::relative(ax))
                        .top(gpui_kit::relative(ay))
                        .ml(px(a.cos() * drift))
                        .mt(px((a * 2.).sin() * drift * 0.6))
                        .size(px(8.))
                        .rounded_full()
                        .shadow(glow(color.opacity(strength), 260., 170.))
                }),
        )
        .into_any_element()
}

/// Easing for the `ix`-th item of a staggered list: waits, then eases out.
pub fn stagger(ix: usize) -> (Duration, impl Fn(f32) -> f32) {
    const STEP_MS: u64 = 35;
    const BASE_MS: u64 = 380;
    let delay = STEP_MS * ix.min(12) as u64;
    let total = BASE_MS + delay;
    let start = delay as f32 / total as f32;
    let ease = ease_out_quint();
    (Duration::from_millis(total), move |t: f32| {
        ease(((t - start) / (1. - start)).clamp(0., 1.))
    })
}

/// Fade + rise into place on first render. `ix` staggers siblings.
pub fn enter<E: IntoElement + Styled + 'static>(
    el: E,
    id: impl Into<ElementId>,
    ix: usize,
) -> impl IntoElement {
    let (duration, easing) = stagger(ix);
    el.with_animation(id, Animation::new(duration).with_easing(easing), |el, t| {
        el.opacity(t).relative().top(px((1. - t) * 10.))
    })
}

/// Circular progress: a track plus a `from`→`to` gradient arc. `progress` springs to new values.
pub fn ring(
    id: impl Into<ElementId>,
    progress: f32,
    diameter: f32,
    stroke: f32,
    (from, to): (Hsla, Hsla),
    theme: &Theme,
) -> impl IntoElement {
    let track = theme.muted_foreground.opacity(0.14);
    div().absolute().inset_0().size(px(diameter)).with_spring(
        id,
        SpringAnimation::new(SPRING)
            .to(progress.clamp(0., 1.))
            .from(0.),
        move |el, value| el.child(ring_canvas(value, stroke, track, from, to)),
    )
}

fn ring_canvas(value: f32, stroke: f32, track: Hsla, from: Hsla, to: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let r = (bounds.size.width.min(bounds.size.height) - px(stroke)) / 2.;
            let c = bounds.center();
            let at = |turn: f32| {
                let a = turn * TAU - TAU / 4.;
                point(c.x + r * a.cos(), c.y + r * a.sin())
            };
            let radii = point(r, r);

            let mut full = PathBuilder::stroke(px(stroke));
            full.move_to(at(0.));
            full.arc_to(radii, px(0.), false, true, at(0.5));
            full.arc_to(radii, px(0.), false, true, at(0.9999));
            if let Ok(path) = full.build() {
                window.paint_path(path, track);
            }

            let value = value.clamp(0., 0.9999);
            if value > 0.002 {
                let mut arc = PathBuilder::stroke(px(stroke));
                arc.move_to(at(0.));
                arc.arc_to(radii, px(0.), value > 0.5, true, at(value));
                if let Ok(path) = arc.build() {
                    window.paint_path(
                        path,
                        linear_gradient(
                            135.,
                            linear_color_stop(from, 0.),
                            linear_color_stop(to, 1.),
                        ),
                    );
                }
                // Rounded cap at the head of the arc.
                let head = at(value);
                let cap = px(stroke);
                window.paint_quad(
                    gpui_kit::fill(
                        gpui_kit::Bounds::new(
                            point(head.x - cap / 2., head.y - cap / 2.),
                            gpui_kit::size(cap, cap),
                        ),
                        to,
                    )
                    .corner_radii(cap / 2.),
                );
                let start = at(0.);
                window.paint_quad(
                    gpui_kit::fill(
                        gpui_kit::Bounds::new(
                            point(start.x - cap / 2., start.y - cap / 2.),
                            gpui_kit::size(cap, cap),
                        ),
                        from,
                    )
                    .corner_radii(cap / 2.),
                );
            }
        },
    )
    .size_full()
}

/// The brand colors for `ring`.
pub fn brand_pair(theme: &Theme) -> (Hsla, Hsla) {
    (theme.chart_1, theme.chart_3)
}

/// Square container sized for `ring`, with centered children on top.
pub fn ring_box(diameter: f32) -> Div {
    div()
        .relative()
        .flex_none()
        .size(px(diameter))
        .flex()
        .items_center()
        .justify_center()
}

/// Horizontal bar that springs to `fraction` of its track width.
pub fn progress_bar(id: impl Into<ElementId>, fraction: f32, theme: &Theme) -> impl IntoElement {
    let fill = brand_gradient(theme, 90.);
    div()
        .h(px(6.))
        .w_full()
        .rounded_full()
        .bg(theme.muted_foreground.opacity(0.14))
        .child(
            div().h_full().rounded_full().bg(fill).with_spring(
                id,
                SpringAnimation::new(SPRING)
                    .to(fraction.clamp(0., 1.))
                    .from(0.),
                |el, v| el.w(gpui_kit::relative(v.max(0.))),
            ),
        )
}

/// Ambient gentle pulse (opacity between `min` and 1) for "live" indicators.
pub fn pulse<E: IntoElement + Styled + 'static>(
    el: E,
    id: impl Into<ElementId>,
    animate: bool,
    min: f32,
) -> AnyElement {
    if !animate {
        return el.into_any_element();
    }
    el.with_animation(
        id,
        Animation::new(Duration::from_millis(1600))
            .repeat()
            .with_max_fps(10.)
            .with_easing(gpui_kit::pulsating_between(min, 1.)),
        |el, t| el.opacity(t),
    )
    .into_any_element()
}
