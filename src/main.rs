mod actions;
mod components;
mod pages;
mod storage;
mod tasks;
mod theme;
mod workspace;

use gpui_kit::{App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};

use crate::workspace::Workspace;

pub const APP_NAME: &str = "Workdesk";

fn main() {
    let app = gpui_kit::application().with_assets(components::Assets);
    // macOS convention: closing the window keeps the app in the Dock; clicking the Dock icon reopens it.
    app.on_reopen(|cx| {
        if cx.windows().is_empty() {
            open_main_window(cx);
        }
    });

    app.run(|cx: &mut App| {
        gpui_kit::init(cx);
        theme::init(cx);
        actions::init(cx);

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
    let bounds = Bounds::centered(None, size(px(1000.), px(760.)), cx);
    let result = gpui_kit::open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(720.), px(480.))),
            titlebar: Some(TitlebarOptions {
                title: Some(APP_NAME.into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
        |window, cx| cx.new(|cx| Workspace::new(window, cx)),
    );
    if let Err(err) = result {
        eprintln!("failed to open main window: {err:?}");
        cx.quit();
    }
}
