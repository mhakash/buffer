use gpui::{App, Global, Hsla, rgb};
use serde::{Deserialize, Serialize};

use crate::storage;

const SETTINGS_FILE: &str = "settings.json";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    Light,
    #[default]
    Dark,
}

#[derive(Default, Serialize, Deserialize)]
struct Settings {
    #[serde(default)]
    appearance: Appearance,
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub appearance: Appearance,
    pub background: Hsla,
    pub surface: Hsla,
    pub sidebar: Hsla,
    pub border: Hsla,
    pub hover: Hsla,
    pub selected: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub accent: Hsla,
    pub accent_text: Hsla,
    #[allow(dead_code)]
    pub success: Hsla,
    pub danger: Hsla,
}

impl Global for Theme {}

impl Theme {
    pub fn dark() -> Self {
        Self {
            appearance: Appearance::Dark,
            background: rgb(0x1e1f22).into(),
            surface: rgb(0x26282c).into(),
            sidebar: rgb(0x18191c).into(),
            border: rgb(0x34363c).into(),
            hover: rgb(0x2e3035).into(),
            selected: rgb(0x35373c).into(),
            text: rgb(0xe6e6e8).into(),
            text_muted: rgb(0x8b8e96).into(),
            accent: rgb(0x4f8cff).into(),
            accent_text: rgb(0xffffff).into(),
            success: rgb(0x22c55e).into(),
            danger: rgb(0xef4444).into(),
        }
    }

    pub fn light() -> Self {
        Self {
            appearance: Appearance::Light,
            background: rgb(0xffffff).into(),
            surface: rgb(0xf6f7f9).into(),
            sidebar: rgb(0xeef0f3).into(),
            border: rgb(0xe1e4e8).into(),
            hover: rgb(0xe9ecef).into(),
            selected: rgb(0xdde3ee).into(),
            text: rgb(0x1f2328).into(),
            text_muted: rgb(0x6e7781).into(),
            accent: rgb(0x2563eb).into(),
            accent_text: rgb(0xffffff).into(),
            success: rgb(0x16a34a).into(),
            danger: rgb(0xdc2626).into(),
        }
    }

    fn for_appearance(appearance: Appearance) -> Self {
        match appearance {
            Appearance::Light => Self::light(),
            Appearance::Dark => Self::dark(),
        }
    }

    pub fn init(cx: &mut App) {
        let settings: Settings = storage::load(SETTINGS_FILE);
        cx.set_global(Self::for_appearance(settings.appearance));
    }

    pub fn set(appearance: Appearance, cx: &mut App) {
        cx.set_global(Self::for_appearance(appearance));
        cx.refresh_windows();
        if let Err(err) = storage::save(SETTINGS_FILE, &Settings { appearance }) {
            eprintln!("failed to save settings: {err:#}");
        }
    }

    pub fn toggle(cx: &mut App) {
        let next = match cx.theme().appearance {
            Appearance::Light => Appearance::Dark,
            Appearance::Dark => Appearance::Light,
        };
        Self::set(next, cx);
    }
}

/// Lets any context read the theme as `cx.theme()`.
pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Theme {
        self.global::<Theme>()
    }
}
