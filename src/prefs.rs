//! User preferences (`settings.json`), held as a GPUI global.

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
        }
    }
}

impl Global for Prefs {}

pub fn init(cx: &mut App) {
    let prefs: Prefs = storage::load(FILE);
    cx.set_global(prefs);
}

pub fn get(cx: &App) -> &Prefs {
    cx.global::<Prefs>()
}

/// Mutates, persists and repaints every window.
pub fn update(cx: &mut App, f: impl FnOnce(&mut Prefs)) {
    f(cx.global_mut::<Prefs>());
    if let Err(err) = storage::save(FILE, cx.global::<Prefs>()) {
        eprintln!("failed to save settings: {err:#}");
    }
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
