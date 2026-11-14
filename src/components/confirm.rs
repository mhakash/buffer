//! Modal "are you sure?" before destructive actions (kit `AlertDialog`).

use std::rc::Rc;

use gpui_kit::component::{WindowExt, button::ButtonVariant};
use gpui_kit::{App, SharedString, Window};

/// Opens a centered confirm dialog; `on_confirm` runs only if the user clicks Delete.
pub fn confirm_delete(
    window: &mut Window,
    cx: &mut App,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    on_confirm: impl Fn(&mut Window, &mut App) + 'static,
) {
    let title = title.into();
    let description = description.into();
    let on_confirm = Rc::new(on_confirm);
    window.open_alert_dialog(cx, move |dialog, _, _| {
        let on_confirm = on_confirm.clone();
        dialog
            .title(title.clone())
            .description(description.clone())
            .show_cancel(true)
            .cancel_text("Cancel")
            .ok_text("Delete")
            .ok_variant(ButtonVariant::Danger)
            .on_ok(move |_, window, cx| {
                on_confirm(window, cx);
                true
            })
    });
}
