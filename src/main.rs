mod actions;
mod components;
mod editor;
mod focus;
mod net;
mod notes;
mod pages;
mod prefs;
mod quotes;
mod storage;
mod tasks;
mod theme;
mod workspace;
mod workspaces;

use gpui_kit::component::TitleBar;
use gpui_kit::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};

use crate::workspace::Workspace;

pub const APP_NAME: &str = "Buffer";

fn main() {
    let app = gpui_kit::application().with_assets(components::Assets);
    // macOS convention: closing the window keeps the app in the Dock; clicking the Dock icon reopens it.
    app.on_reopen(|cx| {
        if cx.windows().is_empty() {
            open_main_window(cx);
        }
    });

    storage::migrate_legacy_dir();
    app.run(|cx: &mut App| {
        gpui_kit::init(cx);
        cx.set_http_client(net::Client::new());
        workspaces::init(cx);
        prefs::init(cx);
        theme::init(cx);
        actions::init(cx);
        editor::init(cx);

        if !cfg!(target_os = "macos") {
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
        }

        open_main_window(cx);
        cx.activate(true);
    });
}

fn open_main_window(cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(1180.), px(800.)), cx);
    let mut titlebar = TitleBar::title_bar_options();
    titlebar.title = Some(APP_NAME.into());
    let result = gpui_kit::open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(860.), px(560.))),
            // Transparent title bar: the workspace draws edge-to-edge and owns dragging.
            titlebar: Some(titlebar),
            ..TitleBar::window_options()
        },
        cx,
        |window, cx| cx.new(|cx| Workspace::new(window, cx)),
    );
    if let Err(err) = result {
        eprintln!("failed to open main window: {err:?}");
        cx.quit();
    }
}
