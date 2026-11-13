use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString, assets::icon_assets};

// gpui-kit embeds only its component icons; list any other Lucide icon the app uses here.
icon_assets!(ExtraIcons, [Flag, GripVertical, ListTodo, Pencil, X]);

/// gpui-kit's component icons plus `ExtraIcons`; register with `Application::with_assets`.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::assets::{IconName, IconNamed};

    use super::*;

    #[test]
    fn every_used_icon_has_an_asset() {
        for name in [
            IconName::Check,
            IconName::Flag,
            IconName::GripVertical,
            IconName::ListTodo,
            IconName::Pencil,
            IconName::Plus,
            IconName::Settings,
            IconName::Sun,
            IconName::X,
        ] {
            let bytes = Assets.load(&name.path()).unwrap().unwrap();
            assert!(std::str::from_utf8(&bytes).unwrap().contains("<svg"));
        }
    }
}
