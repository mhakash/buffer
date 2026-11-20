//! The `/` block menu: typing `/` in an empty text block opens it with its own search field
//! (focused; the `/` and what follows move there). It lists block types, then the workspace's
//! custom blocks (`crate::blocks`), then file links (`files`); ↑↓ Enter or a click turns the
//! block into one. Esc, a space,
//! or a query with no match closes it and puts `/query` back into the block; Backspace on an
//! empty query closes it. Also the built-in block-type list for the handle's "Turn into".

use gpui_kit::component::{
    Icon, Sizable, Theme, h_flex,
    input::{Input, InputEvent, InputState},
    v_flex,
};
use gpui_kit::{
    AnyElement, App, Context, Entity, Focusable, MouseButton, SharedString, Window, anchored,
    assets::IconName, deferred, div, prelude::*, px,
};

use super::{
    BlockEditor,
    files::{FileAction, file_items},
    model::{Kind, Table},
};
use crate::blocks::{
    self,
    model::{FENCE, fence_type},
};

pub(super) struct BlockType {
    pub label: &'static str,
    keywords: &'static str,
    pub icon: IconName,
    pub kind: Kind,
}

pub(super) fn block_types() -> Vec<BlockType> {
    let item = |label, keywords, icon, kind| BlockType {
        label,
        keywords,
        icon,
        kind,
    };
    vec![
        item("Text", "paragraph plain", IconName::Type, Kind::Paragraph),
        item(
            "Heading 1",
            "h1 title",
            IconName::Heading1,
            Kind::Heading(1),
        ),
        item(
            "Heading 2",
            "h2 subtitle",
            IconName::Heading2,
            Kind::Heading(2),
        ),
        item("Heading 3", "h3", IconName::Heading3, Kind::Heading(3)),
        item("Bulleted list", "ul bullet", IconName::List, Kind::Bullet),
        item(
            "Numbered list",
            "ol ordered",
            IconName::ListOrdered,
            Kind::Numbered,
        ),
        item(
            "To-do list",
            "todo task checkbox check",
            IconName::ListTodo,
            Kind::Todo(false),
        ),
        item("Quote", "blockquote", IconName::Quote, Kind::Quote),
        item(
            "Code",
            "snippet fence",
            IconName::SquareCode,
            Kind::Code(String::new()),
        ),
        item(
            "Diagram",
            "mermaid chart flowchart",
            IconName::Workflow,
            Kind::Code("mermaid".into()),
        ),
        item("Table", "grid rows columns", IconName::Table, Kind::Table),
        item("Divider", "hr line rule", IconName::Minus, Kind::Divider),
    ]
}

/// Placeholder for an empty block of `kind`.
pub(super) fn kind_label(kind: &Kind) -> &'static str {
    match kind {
        Kind::Paragraph => "Text",
        Kind::Heading(1) => "Heading 1",
        Kind::Heading(2) => "Heading 2",
        Kind::Heading(_) => "Heading 3",
        Kind::Bullet | Kind::Numbered => "List",
        Kind::Todo(_) => "To-do",
        Kind::Quote => "Quote",
        Kind::Code(lang) if lang == "mermaid" => "Mermaid diagram source",
        Kind::Code(lang) if fence_type(lang).is_some() => "Config: one key: value per line",
        Kind::Code(_) => "Code",
        Kind::Table => "Table",
        Kind::Divider | Kind::Raw => "Markdown",
    }
}

/// `/query` typed into a text block opens the menu.
pub(super) fn slash_query(text: &str) -> Option<String> {
    let query = text.strip_prefix('/')?;
    (query.len() <= 24 && !query.contains(char::is_whitespace)).then(|| query.to_lowercase())
}

/// A row of the `/` menu: a built-in block type, or a custom block from the workspace.
struct SlashItem {
    label: SharedString,
    icon: Icon,
    kind: Kind,
    /// Starting text (a custom block's params at their defaults).
    text: String,
    custom: bool,
    /// A file item: runs instead of converting the block.
    file: Option<FileAction>,
}

/// Rows of the `/` menu matching `query`: built-in types first, then file items (`files`:
/// the host stores files; linking works anywhere), then custom blocks.
fn slash_items(query: &str, files: bool, cx: &App) -> Vec<SlashItem> {
    let builtin = block_types()
        .into_iter()
        .filter(|i| i.label.to_lowercase().contains(query) || i.keywords.contains(query))
        .map(|i| SlashItem {
            label: i.label.into(),
            icon: Icon::new(i.icon),
            text: if i.kind == Kind::Table {
                Table::new(3, 3).to_markdown()
            } else {
                String::new()
            },
            kind: i.kind,
            custom: false,
            file: None,
        });
    let file = file_items()
        .into_iter()
        .filter(|(action, ..)| files || *action == FileAction::Link)
        .filter(|(_, label, keywords, _)| {
            label.to_lowercase().contains(query) || keywords.contains(query)
        })
        .map(|(action, label, _, icon)| SlashItem {
            label: label.into(),
            icon: Icon::new(icon),
            kind: Kind::Paragraph,
            text: String::new(),
            custom: false,
            file: Some(action),
        });
    let custom = blocks::types(cx)
        .iter()
        .filter(|t| t.name.contains(query) || t.label().to_lowercase().contains(query))
        .map(|t| SlashItem {
            label: t.label().to_string().into(),
            icon: Icon::empty().path(blocks::icon_path(&t.icon, cx)),
            kind: Kind::Code(format!("{FENCE} {}", t.name)),
            text: t.starter(),
            custom: true,
            file: None,
        });
    builtin.chain(file).chain(custom).collect()
}

impl BlockEditor {
    fn slash_query(&self, cx: &App) -> String {
        self.slash_input.read(cx).value().to_lowercase()
    }

    fn slash_items(&self, cx: &Context<Self>) -> Vec<SlashItem> {
        slash_items(&self.slash_query(cx), self.files.is_some(), cx)
    }

    /// After the active block's text changed: a new `/query` opens the menu. Text put back by
    /// closing it (already `/…`) doesn't reopen it.
    pub(super) fn update_slash(
        &mut self,
        was_query: bool,
        text: &str,
        kind: &Kind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(query) = slash_query(text)
            && kind.is_text()
            && !was_query
        {
            self.open_slash("/", &query, window, cx);
        }
    }

    /// Open the menu on the active block (emptied: its `/query` moves to the search field).
    pub(super) fn open_slash(
        &mut self,
        prefix: &'static str,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.active else { return };
        self.doc.blocks[ix].set_text("");
        // The menu may cover the block; nothing of it should show through.
        self.input.update(cx, |i, cx| {
            i.set_value("", window, cx);
            i.set_placeholder("", window, cx);
        });
        self.slash = Some(0);
        self.slash_prefix = prefix;
        self.slash_scroll.scroll_to_item(0);
        let query = query.to_string();
        self.slash_input.update(cx, |i, cx| {
            i.set_value(query, window, cx);
            i.focus(window, cx);
        });
        cx.notify();
    }

    /// Close the menu and go back to writing in the block; `restore` puts `/query` back.
    pub(super) fn close_slash(
        &mut self,
        restore: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.slash.take().is_none() {
            return;
        }
        let Some(ix) = self.active else { return };
        let text = if restore {
            format!("{}{}", self.slash_prefix, self.slash_input.read(cx).value())
        } else {
            String::new()
        };
        // Set first, so the textarea's change doesn't reopen the menu.
        self.doc.blocks[ix].set_text(text.clone());
        let end = text.len();
        let hint = super::placeholder(&self.doc.blocks[ix].kind);
        self.input.update(cx, |i, cx| {
            i.set_placeholder(hint, window, cx);
            i.set_value(text, window, cx);
            i.set_selected_range(end..end, cx);
            i.focus(window, cx);
        });
        self.changed(cx);
    }

    pub(super) fn on_slash_input_event(
        &mut self,
        _: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.slash.is_none() {
            return;
        }
        match event {
            InputEvent::Change => {
                let query = self.slash_query(cx);
                // Writing on (a space, or nothing matches): it was text, not a command.
                if query.contains(char::is_whitespace) || self.slash_items(cx).is_empty() {
                    self.close_slash(true, window, cx);
                } else {
                    self.slash = Some(0);
                    self.slash_scroll.scroll_to_item(0);
                    cx.notify();
                }
            }
            InputEvent::Blur => cx.defer_in(window, |this, window, cx| {
                if !this.slash_input.focus_handle(cx).is_focused(window) {
                    this.slash = None;
                    this.deactivate_if_unfocused(window, cx);
                    cx.notify();
                }
            }),
            _ => {}
        }
    }

    /// Backspace in the search field: on an empty query it closes the menu (drops the `/`).
    pub(super) fn slash_backspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.slash_input.read(cx).value().is_empty() {
            cx.stop_propagation();
            self.close_slash(false, window, cx);
        }
    }

    pub(super) fn apply_slash(&mut self, item: usize, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(ix), Some(item)) = (self.active, self.slash_items(cx).into_iter().nth(item))
        else {
            return;
        };
        self.slash = None;
        match item.file {
            Some(action) => self.file_action(ix, action, window, cx),
            None => self.convert(ix, item.kind, item.text, window, cx),
        }
    }

    /// ↑/↓ while the menu is open; false when it isn't.
    pub(super) fn move_slash(&mut self, step: isize, cx: &mut Context<Self>) -> bool {
        let Some(sel) = self.slash else { return false };
        let n = self.slash_items(cx).len().max(1) as isize;
        let sel = (sel as isize + step).rem_euclid(n) as usize;
        self.slash = Some(sel);
        // Child 0 is the "Blocks" label; the "Custom" label sits inside its first row.
        self.slash_scroll.scroll_to_item(sel + 1);
        cx.notify();
        true
    }

    pub(super) fn render_slash(&self, theme: &Theme, cx: &Context<Self>) -> AnyElement {
        let items = self.slash_items(cx);
        let selected = self.slash.unwrap_or(0).min(items.len().saturating_sub(1));
        let first_custom = items.iter().position(|i| i.custom);
        let first_file = items.iter().position(|i| i.file.is_some());
        let empty = items.is_empty();
        let section = |label: &'static str| {
            div()
                .px_2()
                .py_1()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(label)
        };
        let rows = items.into_iter().enumerate().map(|(i, item)| {
            let row = h_flex()
                .id(("slash-item", i))
                .gap_2()
                .px_2()
                .py_1p5()
                .rounded_lg()
                .text_sm()
                .cursor_pointer()
                .when(i == selected, |s| s.bg(theme.secondary_active))
                .when(i != selected, |s| s.hover(|s| s.bg(theme.secondary)))
                .child(item.icon.small().text_color(theme.muted_foreground))
                .child(item.label)
                // Mouse down, so the search field keeps focus.
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.apply_slash(i, window, cx);
                    }),
                );
            v_flex()
                .when(first_file == Some(i), |d| d.child(section("Files")))
                .when(first_custom == Some(i), |d| d.child(section("Custom")))
                .child(row)
        });
        let list = v_flex()
            .id("slash-list")
            .max_h(px(280.))
            .overflow_y_scroll()
            .track_scroll(&self.slash_scroll)
            .child(section(if empty {
                "No matching blocks"
            } else {
                "Blocks"
            }))
            .children(rows);
        let search = Input::new(&self.slash_input)
            .small()
            .appearance(false)
            .prefix(
                Icon::new(IconName::Search)
                    .small()
                    .text_color(theme.muted_foreground),
            );
        let body = v_flex()
            .id("slash-menu")
            .occlude()
            .w(px(240.))
            .mt_1()
            .p_1()
            .rounded_xl()
            .bg(theme.popover)
            .border_1()
            .border_color(theme.border)
            .text_color(theme.popover_foreground)
            // A press on the menu's padding mustn't take focus from the search field.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .pb_1()
                    .mb_1()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(search),
            )
            .child(list);
        deferred(anchored().snap_to_window_with_margin(px(8.)).child(body))
            .with_priority(1)
            .into_any_element()
    }
}
