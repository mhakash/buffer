mod blocks;
mod focus;
mod notes;
mod settings;
mod task_detail;
mod tasks;

use gpui_kit::assets::IconName;

pub use blocks::BlocksPage;
pub use focus::FocusPage;
pub use notes::NotesPage;
pub use settings::SettingsPage;
pub use task_detail::{
    DetailEvent, OpenTask, PANEL_DEFAULT_W, PANEL_MIN_W, ResizePanel, TaskDetail,
};
pub use tasks::TasksPage;

/// Every top-level page shown in the sidebar. Add a variant here to add a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Tasks,
    Focus,
    Notes,
    Blocks,
    Settings,
}

impl Page {
    pub const ALL: [Page; 5] = [
        Page::Tasks,
        Page::Focus,
        Page::Notes,
        Page::Blocks,
        Page::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Page::Tasks => "Tasks",
            Page::Focus => "Focus",
            Page::Notes => "Notes",
            Page::Blocks => "Blocks",
            Page::Settings => "Settings",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Page::Tasks => IconName::ListTodo,
            Page::Focus => IconName::Timer,
            Page::Notes => IconName::StickyNote,
            Page::Blocks => IconName::Blocks,
            Page::Settings => IconName::Settings,
        }
    }

    /// Fills the content area with its own panes and scrolling (no max-width wrapper).
    pub fn owns_panes(self) -> bool {
        matches!(self, Page::Notes | Page::Blocks)
    }

    /// The task detail panel can sit beside this page.
    pub fn shows_task_detail(self) -> bool {
        matches!(self, Page::Tasks | Page::Focus)
    }

    pub fn shortcut(self) -> &'static str {
        match self {
            Page::Tasks => "⌘1",
            Page::Focus => "⌘2",
            Page::Notes => "⌘3",
            Page::Blocks => "⌘4",
            Page::Settings => "⌘,",
        }
    }
}
