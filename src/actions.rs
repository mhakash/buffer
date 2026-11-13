//! App-wide actions, key bindings and the native menu bar.

use gpui::{App, KeyBinding, Menu, MenuItem, SystemMenuType, actions};

use crate::{APP_NAME, theme::Theme};

actions!(
    workdesk,
    [
        Quit,
        CloseWindow,
        ToggleTheme,
        NewTask,
        ShowTasks,
        ShowSettings,
    ]
);

pub fn init(cx: &mut App) {
    // `secondary` is cmd on macOS and ctrl on Linux/Windows.
    cx.bind_keys([
        KeyBinding::new("secondary-q", Quit, None),
        KeyBinding::new("secondary-w", CloseWindow, None),
        KeyBinding::new("secondary-shift-t", ToggleTheme, None),
        KeyBinding::new("secondary-n", NewTask, None),
        KeyBinding::new("secondary-1", ShowTasks, None),
        KeyBinding::new("secondary-,", ShowSettings, None),
    ]);

    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &ToggleTheme, cx| Theme::toggle(cx));

    cx.set_menus(vec![
        Menu {
            name: APP_NAME.into(),
            items: vec![
                MenuItem::action("Settings…", ShowSettings),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action(format!("Quit {APP_NAME}"), Quit),
            ],
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("New Task", NewTask),
                MenuItem::separator(),
                MenuItem::action("Close Window", CloseWindow),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action("Tasks", ShowTasks),
                MenuItem::action("Settings", ShowSettings),
                MenuItem::separator(),
                MenuItem::action("Toggle Light/Dark", ToggleTheme),
            ],
        },
    ]);
}
