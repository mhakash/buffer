use gpui::{App, ClickEvent, ElementId, SharedString, Window, div, prelude::*};

use crate::theme::ActiveTheme;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
}

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    variant: ButtonVariant,
    selected: bool,
    on_click: Option<ClickHandler>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            variant: ButtonVariant::default(),
            selected: false,
            on_click: None,
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, fg) = match (self.variant, self.selected) {
            (ButtonVariant::Primary, _) | (_, true) => (theme.accent, theme.accent_text),
            (ButtonVariant::Secondary, _) => (theme.hover, theme.text),
        };
        let hover = theme.selected;
        let is_filled = self.variant == ButtonVariant::Primary || self.selected;

        div()
            .id(self.id)
            .flex()
            .items_center()
            .justify_center()
            .px_4()
            .py_1()
            .rounded_full()
            .bg(bg)
            .text_color(fg)
            .text_sm()
            .cursor_pointer()
            .map(|this| {
                if is_filled {
                    this.hover(|s| s.opacity(0.9))
                } else {
                    this.hover(move |s| s.bg(hover))
                }
            })
            .active(|s| s.opacity(0.8))
            .child(self.label)
            .when_some(self.on_click, |this, on_click| {
                this.on_click(move |event, window, cx| on_click(event, window, cx))
            })
    }
}
