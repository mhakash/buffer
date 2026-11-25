//! Draws a `RichTextState`: the formatted display text (GPUI `StyledText`, which also gives the
//! layout used for hit-testing), the selection behind it and the caret on top. Sizes and
//! weights come from the parent's text style, so it lines up with the rendered block.

use std::ops::Range;

use gpui_kit::component::{ActiveTheme, Theme};
use gpui_kit::{
    App, Bounds, ElementInputHandler, Entity, FontStyle, FontWeight, HighlightStyle,
    InteractiveElement, IntoElement, MouseButton, MouseMoveEvent, MouseUpEvent, ParentElement,
    Pixels, RenderOnce, SharedString, StrikethroughStyle, Styled, StyledText, UnderlineStyle,
    Window, canvas, div, fill, point, px, size,
};

use super::inline::Marks;
use super::state::RichTextState;

const CARET_W: f32 = 2.;

#[derive(IntoElement)]
pub struct RichText {
    state: Entity<RichTextState>,
}

impl RichText {
    pub fn new(state: &Entity<RichTextState>) -> Self {
        Self {
            state: state.clone(),
        }
    }
}

/// `source` drawn exactly as `RichText` draws it while editing, so opening a block for
/// editing doesn't change how it looks.
pub fn rich_text_view(source: &str, theme: &Theme) -> impl IntoElement + use<> {
    let inline = super::inline::Inline::parse(source);
    let (runs, fonts) = highlights(&inline.display, &inline.styles, None, theme);
    div().relative().w_full().child(
        StyledText::new(inline.display)
            .with_highlights(runs)
            .with_font_family_overrides(fonts),
    )
}

impl RenderOnce for RichText {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let state = self.state.read(cx);
        let display = state.display().to_string();
        let focus = state.focus_handle.clone();
        let (text, highlights, fonts): (SharedString, _, _) = if display.is_empty() {
            let placeholder = if state.placeholder.is_empty() {
                SharedString::from(" ")
            } else {
                state.placeholder.clone()
            };
            let muted = HighlightStyle {
                color: Some(theme.muted_foreground),
                ..HighlightStyle::default()
            };
            let len = placeholder.len();
            (placeholder, vec![(0..len, muted)], Vec::new())
        } else {
            let (highlights, fonts) =
                highlights(&display, &state.inline.styles, state.marked.clone(), &theme);
            (display.clone().into(), highlights, fonts)
        };
        let styled = StyledText::new(text)
            .with_highlights(highlights)
            .with_font_family_overrides(fonts);
        let layout = styled.layout().clone();
        self.state
            .update(cx, |s, _| s.layout = Some((layout.clone(), display)));

        let s = &self.state;
        let selection = self.state.clone();
        let caret = self.state.clone();
        let selection_color = theme.selection;
        let caret_color = theme.caret;
        div()
            .relative()
            .w_full()
            .key_context("Input")
            .track_focus(&focus)
            .cursor_text()
            .on_action(window.listener_for(s, RichTextState::backspace))
            .on_action(window.listener_for(s, RichTextState::delete))
            .on_action(window.listener_for(s, RichTextState::delete_to_line_start))
            .on_action(window.listener_for(s, RichTextState::delete_to_line_end))
            .on_action(window.listener_for(s, RichTextState::delete_word_back))
            .on_action(window.listener_for(s, RichTextState::delete_word_forward))
            .on_action(window.listener_for(s, RichTextState::left))
            .on_action(window.listener_for(s, RichTextState::right))
            .on_action(window.listener_for(s, RichTextState::up))
            .on_action(window.listener_for(s, RichTextState::down))
            .on_action(window.listener_for(s, RichTextState::select_left))
            .on_action(window.listener_for(s, RichTextState::select_right))
            .on_action(window.listener_for(s, RichTextState::select_up))
            .on_action(window.listener_for(s, RichTextState::select_down))
            .on_action(window.listener_for(s, RichTextState::home))
            .on_action(window.listener_for(s, RichTextState::end))
            .on_action(window.listener_for(s, RichTextState::line_start))
            .on_action(window.listener_for(s, RichTextState::line_end))
            .on_action(window.listener_for(s, RichTextState::to_start))
            .on_action(window.listener_for(s, RichTextState::to_end))
            .on_action(window.listener_for(s, RichTextState::word_left))
            .on_action(window.listener_for(s, RichTextState::word_right))
            .on_action(window.listener_for(s, RichTextState::select_all))
            .on_action(window.listener_for(s, RichTextState::select_line_start))
            .on_action(window.listener_for(s, RichTextState::select_line_end))
            .on_action(window.listener_for(s, RichTextState::select_start))
            .on_action(window.listener_for(s, RichTextState::select_end))
            .on_action(window.listener_for(s, RichTextState::select_word_left))
            .on_action(window.listener_for(s, RichTextState::select_word_right))
            .on_action(window.listener_for(s, RichTextState::copy))
            .on_action(window.listener_for(s, RichTextState::cut))
            .on_action(window.listener_for(s, RichTextState::paste))
            .on_action(window.listener_for(s, RichTextState::undo))
            .on_action(window.listener_for(s, RichTextState::redo))
            .on_action(window.listener_for(s, RichTextState::enter))
            .on_action(window.listener_for(s, RichTextState::tab))
            .on_action(window.listener_for(s, RichTextState::character_palette))
            .on_mouse_down(
                MouseButton::Left,
                window.listener_for(s, RichTextState::on_mouse_down),
            )
            // Behind the text: the selection (prepaint has laid the text out by now).
            .child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, cx| {
                        paint_selection(&selection, selection_color, window, cx)
                    },
                )
                .absolute()
                .size_full(),
            )
            .child(styled)
            // On top: the caret, platform text input and drag-selection.
            .child(
                canvas(
                    move |_, _, cx| {
                        let bounds = layout.bounds();
                        caret.update(cx, |s, _| s.bounds = Some(bounds));
                        (caret, bounds)
                    },
                    move |_, (state, bounds), window, cx| {
                        paint_caret(&state, bounds, caret_color, window, cx)
                    },
                )
                .absolute()
                .size_full(),
            )
    }
}

fn paint_selection(
    state: &Entity<RichTextState>,
    color: gpui_kit::Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    let s = state.read(cx);
    if s.sel.is_empty() || !s.focus_handle.is_focused(window) {
        return;
    }
    let Some(bounds) = s.bounds else { return };
    let lh = s.line_height();
    let sel = s.sel.clone();
    for r in s.rows() {
        let (a, b) = (sel.start.max(r.start), sel.end.min(r.end));
        // A selection running past the end of a row (onto the next line) shows a sliver there.
        let past_end = sel.end > r.end && sel.start <= r.end;
        if a > b || (a == b && !past_end) {
            continue;
        }
        let x = |d: usize| {
            if d == r.start {
                bounds.left()
            } else {
                s.position_of(d).map_or(bounds.left(), |p| p.x)
            }
        };
        let left = x(a);
        let mut right = x(b);
        if past_end {
            right += px(4.);
        }
        window.paint_quad(fill(
            Bounds::new(point(left, r.top), size(right - left, lh)),
            color,
        ));
    }
}

fn paint_caret(
    state: &Entity<RichTextState>,
    bounds: Bounds<Pixels>,
    color: gpui_kit::Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    let s = state.read(cx);
    let focus = s.focus_handle.clone();
    window.handle_input(&focus, ElementInputHandler::new(bounds, state.clone()), cx);
    let s = state.read(cx);
    if s.selecting {
        let drag = state.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
            if phase.bubble() && event.pressed_button == Some(MouseButton::Left) {
                drag.update(cx, |s, cx| s.on_drag(event.position, cx));
            }
        });
    }
    if s.selecting || s.press_link_pending() {
        let up = state.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase.bubble() {
                up.update(cx, |s, cx| s.on_mouse_up(event, window, cx));
            }
        });
    }
    if !focus.is_focused(window) || !s.sel.is_empty() {
        return;
    }
    let Some(at) = s.position_of(s.cursor()) else {
        return;
    };
    window.paint_quad(fill(
        Bounds::new(at, size(px(CARET_W), s.line_height())),
        color,
    ));
}

/// Highlight runs (sorted, non-overlapping) and monospace ranges for the display text.
fn highlights(
    display: &str,
    styles: &[(Range<usize>, Marks)],
    marked: Option<Range<usize>>,
    theme: &Theme,
) -> (
    Vec<(Range<usize>, HighlightStyle)>,
    Vec<(Range<usize>, SharedString)>,
) {
    let mut cuts = vec![0, display.len()];
    for (r, _) in styles {
        cuts.extend([r.start, r.end]);
    }
    if let Some(m) = &marked {
        cuts.extend([m.start, m.end]);
    }
    cuts.retain(|&c| c <= display.len());
    cuts.sort_unstable();
    cuts.dedup();
    let mut runs = Vec::new();
    let mut fonts = Vec::new();
    for w in cuts.windows(2) {
        let range = w[0]..w[1];
        if range.is_empty() {
            continue;
        }
        let marks = styles
            .iter()
            .find(|(r, _)| r.start <= range.start && range.end <= r.end)
            .map(|(_, m)| *m)
            .unwrap_or_default();
        let in_marked = marked
            .as_ref()
            .is_some_and(|m| m.start <= range.start && range.end <= m.end);
        let mut h = HighlightStyle::default();
        if marks.bold {
            h.font_weight = Some(FontWeight::BOLD);
        }
        if marks.italic {
            h.font_style = Some(FontStyle::Italic);
        }
        if marks.strike {
            h.strikethrough = Some(StrikethroughStyle {
                thickness: px(1.),
                color: None,
            });
        }
        if marks.code {
            h.background_color = Some(theme.accent);
            fonts.push((range.clone(), theme.mono_font_family.clone()));
        }
        if marks.link {
            h.color = Some(theme.primary);
            h.underline = Some(UnderlineStyle {
                thickness: px(1.),
                color: Some(theme.primary.opacity(0.4)),
                wavy: false,
            });
        }
        if in_marked {
            h.underline = Some(UnderlineStyle {
                thickness: px(1.),
                color: None,
                wavy: false,
            });
        }
        if h != HighlightStyle::default() {
            runs.push((range, h));
        }
    }
    (runs, fonts)
}
