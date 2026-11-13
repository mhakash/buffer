mod actions;
mod components;
mod pages;
mod storage;
mod tasks;
mod theme;
mod workspace;

use gpui::{
    App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};

use crate::{theme::Theme, workspace::Workspace};

pub const APP_NAME: &str = "Workdesk";

fn main() {
    let app = Application::new();
    // macOS convention: closing the window keeps the app in the Dock; clicking the Dock icon reopens it.
    app.on_reopen(|cx| {
        if cx.windows().is_empty() {
            open_main_window(cx);
        }
    });

    app.run(|cx: &mut App| {
        Theme::init(cx);
        actions::init(cx);
        components::text_input::init(cx);

        if !cfg!(target_os = "macos") {
            cx.on_window_closed(|cx| {
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
    let result = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(720.), px(480.))),
            titlebar: Some(TitlebarOptions {
                title: Some(APP_NAME.into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        |window, cx| cx.new(|cx| Workspace::new(window, cx)),
    );
    if let Err(err) = result {
        eprintln!("failed to open main window: {err:?}");
        cx.quit();
    }
}
