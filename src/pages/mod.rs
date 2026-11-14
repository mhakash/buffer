mod focus;
mod settings;
mod task_detail;
mod tasks;

use gpui_kit::assets::IconName;

pub use focus::FocusPage;
pub use settings::SettingsPage;
pub use task_detail::{DetailEvent, OpenTask, TaskDetail};
pub use tasks::TasksPage;

/// Every top-level page shown in the sidebar. Add a variant here to add a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Tasks,
    Focus,
    Settings,
}

impl Page {
    pub const ALL: [Page; 3] = [Page::Tasks, Page::Focus, Page::Settings];

    pub fn label(self) -> &'static str {
        match self {
            Page::Tasks => "Tasks",
            Page::Focus => "Focus",
            Page::Settings => "Settings",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Page::Tasks => IconName::ListTodo,
            Page::Focus => IconName::Timer,
            Page::Settings => IconName::Settings,
        }
    }

    pub fn shortcut(self) -> &'static str {
        match self {
            Page::Tasks => "⌘1",
            Page::Focus => "⌘2",
            Page::Settings => "⌘,",
        }
    }
}
