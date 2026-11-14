use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString, assets::icon_assets};

// gpui-kit embeds only its component icons; list any other Lucide icon the app uses here.
icon_assets!(
    ExtraIcons,
    [
        Brain,
        CircleDot,
        Coffee,
        Flag,
        Flame,
        GitPullRequest,
        GripVertical,
        Link,
        ListChecks,
        ListTodo,
        Moon,
        NotebookPen,
        Pause,
        Pencil,
        Play,
        Quote,
        RotateCcw,
        ScrollText,
        SkipForward,
        Sparkles,
        Sun,
        Target,
        Ticket,
        Timer,
        Trophy,
        X,
        Zap,
    ]
);

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
            IconName::BookOpen,
            IconName::Brain,
            IconName::Check,
            IconName::CircleDot,
            IconName::ExternalLink,
            IconName::GitPullRequest,
            IconName::Globe,
            IconName::Link,
            IconName::ListChecks,
            IconName::NotebookPen,
            IconName::ScrollText,
            IconName::Ticket,
            IconName::Coffee,
            IconName::Flag,
            IconName::Flame,
            IconName::GripVertical,
            IconName::ListTodo,
            IconName::Moon,
            IconName::Pause,
            IconName::Pencil,
            IconName::Play,
            IconName::Plus,
            IconName::Quote,
            IconName::RotateCcw,
            IconName::Settings,
            IconName::SkipForward,
            IconName::Sparkles,
            IconName::Sun,
            IconName::Target,
            IconName::Timer,
            IconName::Trophy,
            IconName::X,
            IconName::Zap,
        ] {
            let bytes = Assets.load(&name.path()).unwrap().unwrap();
            assert!(std::str::from_utf8(&bytes).unwrap().contains("<svg"));
        }
    }
}
