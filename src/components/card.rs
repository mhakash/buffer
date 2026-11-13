use gpui::{App, Div, SharedString, div, prelude::*};

use crate::theme::ActiveTheme;

/// A bordered surface with a title; add body content with `.child(...)`.
pub fn card(title: impl Into<SharedString>, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .flex()
        .flex_col()
        .gap_2()
        .p_4()
        .rounded_lg()
        .border_1()
        .border_color(theme.border)
        .bg(theme.surface)
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.text)
                .child(title.into()),
        )
}
