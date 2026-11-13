use std::borrow::Cow;

use anyhow::Result;
use gpui::{AssetSource, Hsla, SharedString, Svg, prelude::*, svg};

/// Line icons embedded from `assets/icons/`. Add a variant + an `ICONS` entry to add one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconName {
    Check,
    Flag,
    Grip,
    Pencil,
    Plus,
    Settings,
    Sun,
    Tasks,
    X,
}

const ICONS: &[(IconName, &str, &[u8])] = &[
    (
        IconName::Check,
        "icons/check.svg",
        include_bytes!("../../assets/icons/check.svg"),
    ),
    (
        IconName::Flag,
        "icons/flag.svg",
        include_bytes!("../../assets/icons/flag.svg"),
    ),
    (
        IconName::Grip,
        "icons/grip.svg",
        include_bytes!("../../assets/icons/grip.svg"),
    ),
    (
        IconName::Pencil,
        "icons/pencil.svg",
        include_bytes!("../../assets/icons/pencil.svg"),
    ),
    (
        IconName::Plus,
        "icons/plus.svg",
        include_bytes!("../../assets/icons/plus.svg"),
    ),
    (
        IconName::Settings,
        "icons/settings.svg",
        include_bytes!("../../assets/icons/settings.svg"),
    ),
    (
        IconName::Sun,
        "icons/sun.svg",
        include_bytes!("../../assets/icons/sun.svg"),
    ),
    (
        IconName::Tasks,
        "icons/tasks.svg",
        include_bytes!("../../assets/icons/tasks.svg"),
    ),
    (
        IconName::X,
        "icons/x.svg",
        include_bytes!("../../assets/icons/x.svg"),
    ),
];

impl IconName {
    fn path(self) -> &'static str {
        ICONS
            .iter()
            .find(|(name, ..)| *name == self)
            .map_or("", |(_, path, _)| path)
    }
}

/// 16px icon tinted with `color`; resize with `.size_*()`. SVGs don't inherit text color.
pub fn icon(name: IconName, color: Hsla) -> Svg {
    svg()
        .path(name.path())
        .flex_none()
        .size_4()
        .text_color(color)
}

/// Serves the embedded icons to GPUI; register with `Application::with_assets`.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(_, p, _)| *p == path)
            .map(|(.., bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(_, p, _)| p.starts_with(path))
            .map(|(_, p, _)| SharedString::from(*p))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_has_an_asset() {
        for name in [
            IconName::Check,
            IconName::Flag,
            IconName::Pencil,
            IconName::Plus,
            IconName::Settings,
            IconName::Sun,
            IconName::Tasks,
            IconName::X,
        ] {
            let bytes = Assets.load(name.path()).unwrap().unwrap();
            assert!(std::str::from_utf8(&bytes).unwrap().starts_with("<svg"));
        }
    }
}
