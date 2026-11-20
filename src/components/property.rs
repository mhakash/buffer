use gpui_kit::component::{Icon, Sizable, Theme, h_flex};
use gpui_kit::{Div, ParentElement, SharedString, Styled, assets::IconName, div, px};

/// Notion-style property row: muted icon + label column, value appended with `.child(...)`.
pub fn property_row(icon: IconName, label: impl Into<SharedString>, theme: &Theme) -> Div {
    h_flex().min_h(px(32.)).gap_2().child(
        h_flex()
            .flex_none()
            .w(px(112.))
            .gap_2()
            .text_sm()
            .text_color(theme.muted_foreground)
            .child(Icon::new(icon).small().text_color(theme.muted_foreground))
            .child(div().min_w_0().truncate().child(label.into())),
    )
}
