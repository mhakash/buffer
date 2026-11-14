use gpui_kit::{
    ElementId, Hsla, ParentElement, SharedString, Styled, div, prelude::FluentBuilder, px,
};
use gpui_kit::{
    assets::IconName,
    component::{
        Icon, Selectable, Sizable,
        button::{Button, ButtonRounded, ButtonVariants},
        tag::Tag,
    },
};

pub(super) const PILL: ButtonRounded = ButtonRounded::Size(px(999.));

/// Flat, selectable pill with an optional colored dot.
pub fn chip(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    dot: Option<Hsla>,
    selected: bool,
) -> Button {
    Button::new(id)
        .ghost()
        .small()
        .rounded(PILL)
        .selected(selected)
        .when_some(dot, |this, color| {
            this.child(div().size_2().rounded_full().bg(color))
        })
        .child(label.into())
}

/// Round, icon-only ghost button for row actions.
pub fn icon_button(
    id: impl Into<ElementId>,
    name: IconName,
    color: Hsla,
    tooltip: impl Into<SharedString>,
) -> Button {
    Button::new(id)
        .ghost()
        .small()
        .rounded(PILL)
        .icon(Icon::new(name).text_color(color))
        .tooltip(tooltip)
}

/// Tinted, non-interactive pill for a category or status (data-driven `color`).
pub fn category_tag(label: impl Into<SharedString>, color: Hsla) -> Tag {
    let bg = color.opacity(0.15);
    Tag::custom(bg, color, bg)
        .rounded_full()
        .child(label.into())
}
