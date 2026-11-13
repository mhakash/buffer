use gpui_kit::component::{ActiveTheme, v_flex};
use gpui_kit::{App, Div, FontWeight, ParentElement, SharedString, Styled, div};

/// A flat, rounded surface with a title; add body content with `.child(...)`.
pub fn card(title: impl Into<SharedString>, cx: &App) -> Div {
    let theme = cx.theme();
    v_flex()
        .gap_2()
        .p_5()
        .rounded_2xl()
        .bg(theme.group_box)
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.foreground)
                .child(title.into()),
        )
}
