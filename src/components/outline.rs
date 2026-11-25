//! Heading outline over a scrolled document (notes, task notes), like a table of contents: a
//! dash per heading at the top right, the section being read highlighted; hovering lists the
//! titles and a click scrolls there.
//!
//! Positions come from the last frame. Each view measures its own part (the editor its rows
//! against its root, the page the editor against the scrolled content), so a view that wasn't
//! repainted can't skew the result.

use std::{cell::RefCell, rc::Rc};

use gpui_kit::component::{Theme, v_flex};
use gpui_kit::{
    AnyElement, FontWeight, Pixels, ScrollHandle, SharedString, canvas, div, point, prelude::*, px,
};

use crate::editor::Heading;

/// The outline marks the last heading above this line (from the top of the scroll area).
const READ_LINE: f32 = 96.;

#[derive(Clone, Default)]
pub struct Outline(Rc<RefCell<State>>);

#[derive(Default)]
struct State {
    /// Window y of the scrolled content's top and of the editor's top, same frame.
    content_top: Pixels,
    editor_top: Pixels,
    /// The pointer is over the outline: list the titles.
    open: bool,
    /// Entry clicked and the offset it scrolled to: it stays marked until the next scroll
    /// (the last sections can't scroll up to the read line).
    jump: Option<(usize, Pixels)>,
}

impl Outline {
    /// Child of the scrolled content's root (relative or flex): records its top.
    pub fn probe_content(&self) -> impl IntoElement {
        let this = self.clone();
        canvas(
            move |b, _, _| this.0.borrow_mut().content_top = b.top(),
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .size_0()
    }

    /// Wrap the block editor so its top is recorded.
    pub fn wrap_editor(&self, editor: impl IntoElement) -> impl IntoElement {
        let this = self.clone();
        div()
            .relative()
            .child(
                canvas(
                    move |b, _, _| this.0.borrow_mut().editor_top = b.top(),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .size_0(),
            )
            .child(editor)
    }

    /// The outline, positioned absolutely at the top right of the scroll area's (relative)
    /// parent; `None` without headings.
    pub fn render(
        &self,
        id: &'static str,
        title: SharedString,
        headings: Vec<Heading>,
        scroll: &ScrollHandle,
        theme: &Theme,
    ) -> Option<AnyElement> {
        if headings.is_empty() {
            return None;
        }
        let mut state = self.0.borrow_mut();
        let editor_y = state.editor_top - state.content_top;
        // (level, text, y in the content); the title (level 0) is the top of the document.
        let entries: Vec<(u8, SharedString, Pixels)> = std::iter::once((0, title, px(0.)))
            .chain(
                headings
                    .into_iter()
                    .map(|h| (h.level, h.text, editor_y + h.top)),
            )
            .collect();
        let offset = scroll.offset().y;
        let line = -offset + px(READ_LINE);
        let mut active = entries.iter().rposition(|e| e.2 <= line).unwrap_or(0);
        match state.jump {
            Some((ix, at)) if at == offset && ix < entries.len() => active = ix,
            _ => state.jump = None,
        }
        let open = state.open;
        drop(state);

        let body = if open {
            v_flex()
                .id((id, 0u64))
                .w(px(260.))
                .max_h(px(440.))
                .overflow_y_scroll()
                .p_1()
                .rounded_xl()
                .bg(theme.popover)
                .border_1()
                .border_color(theme.border)
                .children(
                    entries
                        .into_iter()
                        .enumerate()
                        .map(|(ix, (level, text, y))| {
                            let this = self.clone();
                            let scroll = scroll.clone();
                            div()
                                .id((id, ix as u64 + 1))
                                .py_1()
                                .pr_2()
                                .pl(px(8. + level.saturating_sub(1) as f32 * 14.))
                                .rounded_lg()
                                .text_sm()
                                .truncate()
                                .cursor_pointer()
                                .text_color(if ix == active {
                                    theme.primary
                                } else {
                                    theme.popover_foreground
                                })
                                .when(level <= 1, |d| d.font_weight(FontWeight::MEDIUM))
                                .hover(|s| s.bg(theme.secondary))
                                .on_click(move |_, window, _| {
                                    this.scroll_to(ix, y, &scroll);
                                    window.refresh();
                                })
                                .child(text)
                        }),
                )
                .into_any_element()
        } else {
            v_flex()
                .items_end()
                .gap(px(7.))
                .p_2()
                .max_h(px(440.))
                .overflow_hidden()
                .children(entries.iter().enumerate().map(|(ix, (level, ..))| {
                    let w = match level {
                        0 => 18.,
                        1 => 15.,
                        2 => 11.,
                        _ => 7.,
                    };
                    div().w(px(w)).h(px(2.)).rounded_full().bg(if ix == active {
                        theme.primary
                    } else {
                        theme.muted_foreground.opacity(0.45)
                    })
                }))
                .into_any_element()
        };
        let this = self.clone();
        Some(
            div()
                .id(id)
                .absolute()
                .top_3()
                .right_1()
                .occlude()
                .on_hover(move |hovered, window, _| {
                    let mut state = this.0.borrow_mut();
                    if state.open != *hovered {
                        state.open = *hovered;
                        window.refresh();
                    }
                })
                .child(body)
                .into_any_element(),
        )
    }

    /// Scroll so content y `y` (entry `ix`) sits just below the top.
    fn scroll_to(&self, ix: usize, y: Pixels, scroll: &ScrollHandle) {
        let target = (y - px(16.)).max(px(0.)).min(scroll.max_offset().y);
        scroll.set_offset(point(scroll.offset().x, -target));
        self.0.borrow_mut().jump = Some((ix, -target));
    }
}
