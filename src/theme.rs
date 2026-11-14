//! Light/dark appearance (persisted) on top of the gpui-kit `Theme` global.
//!
//! Colors live in `themes/buffer.json` (gpui-kit theme format) and are embedded at build time.
//! Debug builds also watch `themes/`, so palette edits apply to the running app on save.

use gpui_kit::{
    App, SharedString,
    component::{Theme, ThemeMode, ThemeRegistry},
};
use serde::{Deserialize, Serialize};

use crate::prefs;

const THEMES: &str = include_str!("../themes/buffer.json");
const LIGHT: &str = "Buffer Light";
const DARK: &str = "Buffer Dark";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    Light,
    #[default]
    Dark,
}

impl Appearance {
    pub fn current(cx: &App) -> Self {
        if Theme::global(cx).is_dark() {
            Self::Dark
        } else {
            Self::Light
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    fn mode(self) -> ThemeMode {
        match self {
            Self::Light => ThemeMode::Light,
            Self::Dark => ThemeMode::Dark,
        }
    }
}

/// Call after `gpui_kit::init` and `prefs::init`.
pub fn init(cx: &mut App) {
    if let Err(err) = ThemeRegistry::global_mut(cx).load_themes_from_str(THEMES) {
        eprintln!("failed to parse embedded themes: {err:#}");
    }
    select_app_themes(cx);
    Theme::change(prefs::get(cx).appearance.mode(), None, cx);
    tune_motion(cx);

    #[cfg(debug_assertions)]
    {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("themes");
        let watched = ThemeRegistry::watch_dir(dir, cx, |cx| {
            select_app_themes(cx);
            Theme::change(Appearance::current(cx).mode(), None, cx);
            tune_motion(cx);
        });
        if let Err(err) = watched {
            eprintln!("theme hot reload disabled: {err:#}");
        }
    }
}

fn select_app_themes(cx: &mut App) {
    let registry = ThemeRegistry::global(cx);
    let find = |name: &str| registry.themes().get(&SharedString::from(name)).cloned();
    let (light, dark) = (find(LIGHT), find(DARK));
    let theme = Theme::global_mut(cx);
    if let Some(light) = light {
        theme.light_theme = light;
    }
    if let Some(dark) = dark {
        theme.dark_theme = dark;
    }
}

/// Slightly slower, longer-travel kit motion (dialogs, popovers) so reveals read as calm.
fn tune_motion(cx: &mut App) {
    let motion = &mut Theme::global_mut(cx).motion;
    motion.duration_normal = std::time::Duration::from_millis(240);
    motion.distance_short = gpui_kit::rems(0.5);
}

pub fn set(appearance: Appearance, cx: &mut App) {
    Theme::change(appearance.mode(), None, cx);
    tune_motion(cx);
    prefs::update(cx, |p| p.appearance = appearance);
}

pub fn toggle(cx: &mut App) {
    let next = match Appearance::current(cx) {
        Appearance::Light => Appearance::Dark,
        Appearance::Dark => Appearance::Light,
    };
    set(next, cx);
}
