//! Full-window image viewer (Notion-style): click an image or diagram in Markdown to open it
//! over a dark backdrop; click anywhere or press Esc to close. State is a Global so Markdown
//! plugins (which only get `Window`/`App`) can open it; `Workspace` renders it on top.

use std::time::Duration;

use gpui_kit::component::Icon;
use gpui_kit::{
    Animation, AnimationExt, AnyElement, App, FocusHandle, Global, Hsla, ImageSource,
    InteractiveElement, IntoElement, KeyDownEvent, ObjectFit, ParentElement,
    StatefulInteractiveElement, Styled, StyledImage, Window, assets::IconName, black, div,
    ease_out_quint, img, px, white,
};

/// Space kept between the image and the window edge.
const MARGIN: f32 = 48.;

/// Padding around a diagram on its surface.
const PAD: f32 = 32.;

/// How to frame a diagram: its natural size in points and the opaque surface it's drawn for
/// (its own canvas is transparent).
#[derive(Clone, Copy)]
pub struct Canvas {
    pub size: (f32, f32),
    pub surface: Hsla,
}

#[derive(Default)]
struct Lightbox {
    /// The image, plus its canvas for diagrams.
    open: Option<(ImageSource, Option<Canvas>)>,
    /// Bumped per open so the entrance animation replays.
    generation: u64,
    /// The viewer takes focus so keys don't reach the page; `restore` gets it back on close.
    focus: Option<FocusHandle>,
    restore: Option<FocusHandle>,
}

impl Global for Lightbox {}

/// Shows `source` full-window; a diagram's `canvas` lets it scale up to fill the view.
pub fn open(source: ImageSource, canvas: Option<Canvas>, window: &mut Window, cx: &mut App) {
    let restore = window.focused(cx);
    let focus = cx.focus_handle();
    let state = cx.default_global::<Lightbox>();
    state.open = Some((source, canvas));
    state.generation += 1;
    state.restore = restore;
    state.focus = Some(focus.clone());
    focus.focus(window, cx);
    window.refresh();
}

fn close(window: &mut Window, cx: &mut App) {
    let state = cx.default_global::<Lightbox>();
    state.open = None;
    state.focus = None;
    if let Some(handle) = state.restore.take() {
        handle.focus(window, cx);
    }
    window.refresh();
}

fn is_open(cx: &App) -> bool {
    cx.try_global::<Lightbox>()
        .is_some_and(|state| state.open.is_some())
}

/// Root `capture_key_down` handler: while open, Esc closes and no key reaches the page.
pub fn capture_key(event: &KeyDownEvent, window: &mut Window, cx: &mut App) {
    if !is_open(cx) {
        return;
    }
    cx.stop_propagation();
    if event.keystroke.key == "escape" {
        close(window, cx);
    }
}

/// The overlay, or nothing when closed. Render as the last child of the root.
pub fn render(window: &Window, cx: &App) -> Option<AnyElement> {
    let state = cx.try_global::<Lightbox>()?;
    let (source, canvas) = state.open.clone()?;
    let focus = state.focus.clone()?;
    let generation = state.generation;

    let view = window.viewport_size();
    let (max_w, max_h) = (
        (f32::from(view.width) - 2. * MARGIN).max(1.),
        (f32::from(view.height) - 2. * MARGIN).max(1.),
    );
    let image = img(source).object_fit(ObjectFit::Contain);
    let image = match canvas {
        // Diagrams: fit the window, up to 2× their natural size, on their own surface.
        Some(Canvas {
            size: (w, h),
            surface,
        }) => {
            let scale = ((max_w - 2. * PAD) / w)
                .min((max_h - 2. * PAD) / h)
                .clamp(0.1, 2.);
            div()
                .p(px(PAD))
                .rounded_2xl()
                .bg(surface)
                .child(image.w(px(w * scale)).h(px(h * scale)))
                .into_any_element()
        }
        None => image
            .max_w(px(max_w))
            .max_h(px(max_h))
            .rounded_lg()
            .into_any_element(),
    };

    Some(
        div()
            .id("lightbox")
            .track_focus(&focus)
            .occlude()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(black().opacity(0.85))
            .cursor_pointer()
            .on_click(|_, window, cx| close(window, cx))
            .child(
                div()
                    .absolute()
                    .top(px(16.))
                    .right(px(16.))
                    .p_2()
                    .rounded_full()
                    .hover(|s| s.bg(white().opacity(0.12)))
                    .child(Icon::new(IconName::Close).text_color(white().opacity(0.85))),
            )
            .child(image)
            .with_animation(
                ("lightbox-in", generation),
                Animation::new(Duration::from_millis(180)).with_easing(ease_out_quint()),
                |el, t| el.opacity(t),
            )
            .into_any_element(),
    )
}
