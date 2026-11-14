//! App-wide actions, key bindings and the native menu bar.

use gpui_kit::{Action, App, KeyBinding, Menu, MenuItem, SystemMenuType, actions};

use crate::{APP_NAME, theme};

actions!(
    workdesk,
    [
        Quit,
        CloseWindow,
        ToggleTheme,
        NewTask,
        ShowTasks,
        ShowFocus,
        ShowSettings,
        ToggleTimer,
    ]
);

pub fn init(cx: &mut App) {
    let bindings = [
        bind("secondary-q", Quit, cx),
        bind("secondary-w", CloseWindow, cx),
        bind("secondary-shift-t", ToggleTheme, cx),
        bind("secondary-n", NewTask, cx),
        bind("secondary-1", ShowTasks, cx),
        bind("secondary-2", ShowFocus, cx),
        bind("secondary-shift-f", ToggleTimer, cx),
        bind("secondary-,", ShowSettings, cx),
    ];
    cx.bind_keys(bindings);

    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &ToggleTheme, cx| theme::toggle(cx));

    cx.set_menus(vec![
        Menu::new(APP_NAME).items([
            MenuItem::action("Settings…", ShowSettings),
            MenuItem::separator(),
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action(format!("Quit {APP_NAME}"), Quit),
        ]),
        Menu::new("File").items([
            MenuItem::action("New Task", NewTask),
            MenuItem::action("Start/Pause Focus Timer", ToggleTimer),
            MenuItem::separator(),
            MenuItem::action("Close Window", CloseWindow),
        ]),
        Menu::new("View").items([
            MenuItem::action("Tasks", ShowTasks),
            MenuItem::action("Focus", ShowFocus),
            MenuItem::action("Settings", ShowSettings),
            MenuItem::separator(),
            MenuItem::action("Toggle Light/Dark", ToggleTheme),
        ]),
    ]);
}

/// Global binding; `secondary` is cmd on macOS and ctrl elsewhere, which only `load` resolves.
fn bind(keys: &str, action: impl Action, cx: &App) -> KeyBinding {
    KeyBinding::load(keys, Box::new(action), None, false, None, cx)
        .unwrap_or_else(|err| panic!("invalid key binding {keys}: {err:?}"))
}
