//! Two-step delete: the first click arms the button (it turns into a red "Delete?" pill), a second
//! click within `ARM_SECS` confirms. Views hold one `armed` slot and implement `Confirmable`.

use std::time::Duration;

use gpui_kit::component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::{App, Context, ElementId, SharedString};

use super::{chip::PILL, icon_button};
use gpui_kit::assets::IconName;

const ARM_SECS: u64 = 3;

pub trait Confirmable: Sized + 'static {
    fn armed(&mut self) -> &mut Option<SharedString>;
}

/// Returns true when `key` was already armed (go ahead and delete); otherwise arms it.
pub fn confirm<V: Confirmable>(view: &mut V, key: SharedString, cx: &mut Context<V>) -> bool {
    if view.armed().as_ref() == Some(&key) {
        *view.armed() = None;
        cx.notify();
        return true;
    }
    *view.armed() = Some(key.clone());
    cx.notify();
    cx.spawn(async move |this, cx| {
        cx.background_executor()
            .timer(Duration::from_secs(ARM_SECS))
            .await;
        let _ = this.update(cx, |view, cx| {
            if view.armed().as_ref() == Some(&key) {
                *view.armed() = None;
                cx.notify();
            }
        });
    })
    .detach();
    false
}

/// X icon, or a red "Delete?" pill while armed.
pub fn delete_button(
    id: impl Into<ElementId>,
    armed: bool,
    tooltip: impl Into<SharedString>,
    cx: &App,
) -> Button {
    if armed {
        Button::new(id)
            .danger()
            .xsmall()
            .rounded(PILL)
            .label("Delete?")
    } else {
        icon_button(id, IconName::X, cx.theme().danger, tooltip)
    }
}
