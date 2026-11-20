//! Dialog editing a text file in place (line numbers, saved as you type). Opened by clicking a
//! link to a text file, or right after `/text file` makes one. With `on_rename` the title is a
//! name field: Enter or leaving it renames the file and reports old → new path so the caller
//! can rewrite its links.

use std::{path::PathBuf, rc::Rc};

use gpui_kit::component::{
    ActiveTheme, Icon, Sizable, WindowExt,
    dialog::DialogEntrance,
    h_flex,
    input::{Editor, EditorState, Enter, Input, InputEvent, InputState},
    v_flex,
};
use gpui_kit::{
    App, ClipboardItem, Context, Entity, FontWeight, SharedString, Subscription, Window,
    assets::IconName, div, prelude::*, px,
};

use super::icon_button;
use crate::files;

pub type OnRename = Rc<dyn Fn(PathBuf, PathBuf, &mut Window, &mut App)>;

/// A clicked file link: text files open in the editor dialog, anything else in its default
/// app; a missing file does nothing.
pub fn open_path(path: PathBuf, window: &mut Window, cx: &mut App) {
    if files::is_text(&path) {
        open(path, None, false, window, cx);
    } else if path.exists() {
        cx.open_with_system(&path);
    }
}

/// Opens `path` in the editor dialog; `rename_first` focuses the name field (a new file).
pub fn open(
    path: PathBuf,
    on_rename: Option<OnRename>,
    rename_first: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let view = cx.new(|cx| FileEditor::new(path, on_rename, window, cx));
    let dialog_view = view.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .w(px(860.))
            .entrance(DialogEntrance::FadeSlide)
            .child(dialog_view.clone())
    });
    view.update(cx, |v, cx| {
        if rename_first && v.on_rename.is_some() {
            v.name.update(cx, |i, cx| {
                i.focus(window, cx);
                i.select_all(window, cx);
            });
        } else {
            v.editor.update(cx, |e, cx| e.focus(window, cx));
        }
    });
}

struct FileEditor {
    path: PathBuf,
    name: Entity<InputState>,
    editor: Entity<EditorState>,
    /// Content last written, so re-emitted values aren't saved again.
    saved: SharedString,
    error: Option<SharedString>,
    on_rename: Option<OnRename>,
    _subscriptions: Vec<Subscription>,
}

impl FileEditor {
    fn new(
        path: PathBuf,
        on_rename: Option<OnRename>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let text: SharedString = std::fs::read_to_string(&path).unwrap_or_default().into();
        let file_name = file_name(&path);
        let editor = cx.new(|cx| {
            let mut state = EditorState::new(window, cx).language(language(&file_name));
            state.set_value(text.clone(), window, cx);
            state
        });
        let name = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("File name");
            state.set_value(file_name, window, cx);
            state
        });
        let subscriptions = vec![
            cx.subscribe(&editor, |this, editor, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = editor.read(cx).value();
                    if value != this.saved {
                        this.error = std::fs::write(&this.path, value.as_bytes())
                            .err()
                            .map(|err| format!("Couldn't save: {err}").into());
                        this.saved = value;
                        cx.notify();
                    }
                }
            }),
            cx.subscribe_in(
                &name,
                window,
                |this, input, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                        let value = input.read(cx).value().to_string();
                        this.rename(&value, window, cx);
                    }
                },
            ),
        ];
        Self {
            path,
            name,
            editor,
            saved: text,
            error: None,
            on_rename,
            _subscriptions: subscriptions,
        }
    }

    fn rename(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        let current = file_name(&self.path);
        if value.trim().is_empty() {
            self.name
                .update(cx, |i, cx| i.set_value(current, window, cx));
            return;
        }
        match files::rename(&self.path, value) {
            Ok(Some(new)) => {
                let old = std::mem::replace(&mut self.path, new.clone());
                let shown = file_name(&new);
                if shown != value {
                    self.name.update(cx, |i, cx| i.set_value(shown, window, cx));
                }
                self.error = None;
                if let Some(on_rename) = self.on_rename.clone() {
                    on_rename(old, new, window, cx);
                }
            }
            Ok(None) => {}
            Err(err) => self.error = Some(format!("{err:#}").into()),
        }
        cx.notify();
    }
}

impl Render for FileEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let path = self.path.clone();
        let editor = self.editor.clone();
        let title = if self.on_rename.is_some() {
            div()
                .flex_1()
                .min_w_0()
                // Enter renames (the input's event) and moves on to the content; it mustn't
                // reach the dialog, which would close.
                .on_action(cx.listener(|this, _: &Enter, window, cx| {
                    this.editor.update(cx, |e, cx| e.focus(window, cx));
                }))
                .child(
                    Input::new(&self.name)
                        .appearance(false)
                        .px_0()
                        .font_weight(FontWeight::SEMIBOLD),
                )
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .child(file_name(&self.path))
                .into_any_element()
        };
        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .w_full()
                    .pr_8()
                    .gap_2()
                    .child(
                        Icon::new(IconName::FileText)
                            .small()
                            .text_color(theme.muted_foreground),
                    )
                    .child(title)
                    .child(
                        icon_button(
                            "file-copy-text",
                            IconName::Copy,
                            theme.muted_foreground,
                            "Copy contents",
                        )
                        .on_click(move |_, _, cx| {
                            let text = editor.read(cx).value().to_string();
                            cx.write_to_clipboard(ClipboardItem::new_string(text));
                        }),
                    )
                    .child(
                        icon_button(
                            "file-reveal",
                            IconName::FolderOpen,
                            theme.muted_foreground,
                            "Show in Finder",
                        )
                        .on_click(move |_, _, cx| cx.reveal_path(&path)),
                    ),
            )
            .child(
                div()
                    .rounded_xl()
                    .border_1()
                    .border_color(theme.border)
                    .overflow_hidden()
                    .child(Editor::new(&self.editor).appearance(false).h(px(480.))),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(if self.error.is_some() {
                        theme.danger
                    } else {
                        theme.muted_foreground
                    })
                    .child(
                        self.error
                            .clone()
                            .unwrap_or_else(|| "Saved as you type".into()),
                    ),
            )
    }
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Editor language from a file name (used for highlighting when the kit has it).
fn language(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "json" | "jsonl" | "ndjson" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "md" | "markdown" => "markdown",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "py" => "python",
        "rs" => "rust",
        "go" => "go",
        "rb" => "ruby",
        "java" => "java",
        "sh" | "bash" | "zsh" => "bash",
        "sql" => "sql",
        "html" | "htm" => "html",
        "css" | "scss" => "css",
        "xml" => "xml",
        "c" | "h" => "c",
        "cpp" | "cc" | "hpp" => "cpp",
        _ => "text",
    }
}
