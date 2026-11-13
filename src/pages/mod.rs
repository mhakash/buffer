mod settings;
mod tasks;

use gpui_kit::assets::IconName;

pub use settings::SettingsPage;
pub use tasks::TasksPage;

/// Every top-level page shown in the sidebar. Add a variant here to add a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Tasks,
    Settings,
}

impl Page {
    pub const ALL: [Page; 2] = [Page::Tasks, Page::Settings];

    pub fn label(self) -> &'static str {
        match self {
            Page::Tasks => "Tasks",
            Page::Settings => "Settings",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Page::Tasks => IconName::ListTodo,
            Page::Settings => IconName::Settings,
        }
    }
}
