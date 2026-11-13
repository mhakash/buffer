use gpui::{App, Div, ElementId, Hsla, SharedString, Stateful, div, prelude::*};

use crate::theme::ActiveTheme;

/// Small rounded, clickable pill with an optional colored dot.
pub fn chip(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    dot: Option<Hsla>,
    selected: bool,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let hover = theme.hover;
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .gap_1p5()
        .px_2p5()
        .py_0p5()
        .rounded_full()
        .text_xs()
        .cursor_pointer()
        .border_1()
        .map(|this| {
            if selected {
                this.bg(theme.selected)
                    .border_color(theme.border)
                    .text_color(theme.text)
            } else {
                this.border_color(gpui::transparent_black())
                    .text_color(theme.text_muted)
                    .hover(move |s| s.bg(hover))
            }
        })
        .when_some(dot, |this, color| {
            this.child(div().size_2().rounded_full().bg(color))
        })
        .child(label.into())
}

/// Square, icon-only button (glyph text) for row actions.
pub fn icon_button(
    id: impl Into<ElementId>,
    icon: &'static str,
    color: Hsla,
    cx: &App,
) -> Stateful<Div> {
    let hover = cx.theme().hover;
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size_6()
        .rounded_md()
        .text_xs()
        .text_color(color)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(icon)
}
