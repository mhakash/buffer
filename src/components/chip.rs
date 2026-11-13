use gpui::{App, Div, ElementId, Hsla, SharedString, Stateful, div, prelude::*};

use super::{IconName, icon};
use crate::theme::ActiveTheme;

/// Flat, clickable pill with an optional colored dot.
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
        .px_3()
        .py_1()
        .rounded_full()
        .text_xs()
        .cursor_pointer()
        .map(|this| {
            if selected {
                this.bg(theme.selected).text_color(theme.text)
            } else {
                this.text_color(theme.text_muted)
                    .hover(move |s| s.bg(hover))
            }
        })
        .when_some(dot, |this, color| {
            this.child(div().size_2().rounded_full().bg(color))
        })
        .child(label.into())
}

/// Round, icon-only button for row actions.
pub fn icon_button(
    id: impl Into<ElementId>,
    name: IconName,
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
        .size_7()
        .rounded_full()
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(icon(name, color).size_3p5())
}
