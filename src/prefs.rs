//! User preferences (`settings.json`), held as a GPUI global. App-level (in `storage::app_dir`),
//! so they stay the same across workspaces.

use gpui_kit::{App, Global};
use serde::{Deserialize, Serialize};

use crate::{storage, theme::Appearance};

const FILE: &str = "settings.json";

/// On-disk shape of `settings.json`. New fields need `#[serde(default)]` so old files load.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default)]
    pub appearance: Appearance,
    /// Shown in the greeting; empty = no name.
    #[serde(default)]
    pub name: String,
    /// Drifting background glow + idle animations. Off saves CPU.
    #[serde(default = "yes")]
    pub ambient: bool,
    /// Task detail side-panel width (px), set by dragging its left edge.
    #[serde(default = "detail_width")]
    pub detail_width: f32,
}

fn detail_width() -> f32 {
    crate::pages::PANEL_DEFAULT_W
}

fn yes() -> bool {
    true
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
            name: String::new(),
            ambient: true,
            detail_width: detail_width(),
        }
    }
}

impl Global for Prefs {}

pub fn init(cx: &mut App) {
    let prefs: Prefs = storage::load_app(FILE);
    cx.set_global(prefs);
}

pub fn get(cx: &App) -> &Prefs {
    cx.global::<Prefs>()
}

/// Live-updates the panel width while dragging; [`save`] persists it on drop.
pub fn set_detail_width(cx: &mut App, width: f32) {
    cx.global_mut::<Prefs>().detail_width = width;
}

pub fn save(cx: &App) {
    if let Err(err) = storage::save_app(FILE, cx.global::<Prefs>()) {
        eprintln!("failed to save settings: {err:#}");
    }
}

/// Mutates, persists and repaints every window.
pub fn update(cx: &mut App, f: impl FnOnce(&mut Prefs)) {
    f(cx.global_mut::<Prefs>());
    save(cx);
    cx.refresh_windows();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_file_still_loads() {
        let prefs: Prefs = serde_json::from_str(r#"{"appearance":"light"}"#).unwrap();
        assert_eq!(prefs.appearance, Appearance::Light);
        assert!(prefs.ambient);
        assert!(prefs.name.is_empty());
    }
}
