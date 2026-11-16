use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString, assets::icon_assets};

// gpui-kit embeds only its component icons; list any other Lucide icon the app uses here.
icon_assets!(
    ExtraIcons,
    [
        ArrowLeft,
        Brain,
        Calendar,
        CircleDot,
        Clock,
        Coffee,
        Eye,
        Flag,
        Flame,
        FolderOpen,
        GitPullRequest,
        GripVertical,
        Heading1,
        Heading2,
        Heading3,
        Link,
        List,
        ListChecks,
        ListOrdered,
        ListTodo,
        Maximize2,
        Minus,
        Moon,
        NotebookPen,
        PanelRightOpen,
        Pause,
        Pencil,
        Play,
        Quote,
        RotateCcw,
        ScrollText,
        SkipForward,
        SquareCode,
        Sparkles,
        SquarePen,
        StickyNote,
        Sun,
        Tags,
        Target,
        Ticket,
        Timer,
        Trash,
        Trophy,
        Type,
        Workflow,
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
    use gpui_kit::assets::IconName;

    use super::*;

    #[test]
    fn every_used_icon_has_an_asset() {
        for name in [
            IconName::ArrowLeft,
            IconName::ArrowUp,
            IconName::ArrowDown,
            IconName::Copy,
            IconName::GripVertical,
            IconName::Heading1,
            IconName::Heading2,
            IconName::Heading3,
            IconName::List,
            IconName::ListOrdered,
            IconName::ListTodo,
            IconName::Minus,
            IconName::Quote,
            IconName::SquareCode,
            IconName::Type,
            IconName::Workflow,
            IconName::BookOpen,
            IconName::Calendar,
            IconName::Clock,
            IconName::Eye,
            IconName::FolderOpen,
            IconName::Maximize2,
            IconName::PanelRightOpen,
            IconName::SquarePen,
            IconName::Tags,
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
            IconName::StickyNote,
            IconName::Trash,
            IconName::Search,
            IconName::FileText,
            IconName::Trophy,
            IconName::X,
            IconName::Zap,
        ] {
            let bytes = Assets.load(&name.path()).unwrap().unwrap();
            assert!(std::str::from_utf8(&bytes).unwrap().contains("<svg"));
        }
    }
}
