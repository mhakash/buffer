//! The `/` block menu: typing `/` in a text block lists block types; ↑↓ Enter or a click
//! turns the block into one. Also the block-type list for the handle's "Turn into".

use gpui_kit::component::{Icon, Sizable, Theme, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, MouseButton, Window, anchored, assets::IconName, deferred, div,
    prelude::*, px,
};

use super::{BlockEditor, model::Kind};

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
        Kind::Code(_) => "Code",
        Kind::Divider | Kind::Raw => "Markdown",
    }
}

/// `/query` typed into a text block opens the menu.
pub(super) fn slash_query(text: &str) -> Option<String> {
    let query = text.strip_prefix('/')?;
    (query.len() <= 24 && !query.contains(char::is_whitespace)).then(|| query.to_lowercase())
}

impl BlockEditor {
    fn slash_items(&self, cx: &Context<Self>) -> Vec<BlockType> {
        let query = slash_query(&self.text(cx)).unwrap_or_default();
        block_types()
            .into_iter()
            .filter(|i| {
                i.label.to_lowercase().contains(&query) || i.keywords.contains(query.as_str())
            })
            .collect()
    }

    /// After the active block's text changed: open on a new `/query`, keep open (back at the
    /// top) while it's refined. Once dismissed it stays closed until a new `/` is typed.
    pub(super) fn update_slash(&mut self, was_query: bool, text: &str, kind: &Kind) {
        let open =
            kind.is_text() && slash_query(text).is_some() && (self.slash.is_some() || !was_query);
        self.slash = open.then_some(0);
        self.slash_scroll.scroll_to_item(0);
    }

    pub(super) fn apply_slash(&mut self, item: usize, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(ix), Some(item)) = (self.active, self.slash_items(cx).into_iter().nth(item))
        else {
            return;
        };
        self.slash = None;
        self.convert(ix, item.kind, String::new(), window, cx);
    }

    /// ↑/↓ while the menu is open; false when it isn't.
    pub(super) fn move_slash(&mut self, step: isize, cx: &mut Context<Self>) -> bool {
        let Some(sel) = self.slash else { return false };
        let n = self.slash_items(cx).len().max(1) as isize;
        let sel = (sel as isize + step).rem_euclid(n) as usize;
        self.slash = Some(sel);
        // Child 0 is the "Blocks" label.
        self.slash_scroll.scroll_to_item(sel + 1);
        cx.notify();
        true
    }

    pub(super) fn render_slash(&self, theme: &Theme, cx: &Context<Self>) -> AnyElement {
        let items = self.slash_items(cx);
        let selected = self.slash.unwrap_or(0).min(items.len().saturating_sub(1));
        let rows = items.iter().enumerate().map(|(i, item)| {
            h_flex()
                .id(("slash-item", i))
                .gap_2()
                .px_2()
                .py_1p5()
                .rounded_lg()
                .text_sm()
                .cursor_pointer()
                .when(i == selected, |s| s.bg(theme.secondary_active))
                .when(i != selected, |s| s.hover(|s| s.bg(theme.secondary)))
                .child(
                    Icon::new(item.icon.clone())
                        .small()
                        .text_color(theme.muted_foreground),
                )
                .child(item.label)
                // Mouse down, so the textarea keeps focus.
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.apply_slash(i, window, cx);
                    }),
                )
        });
        let body = v_flex()
            .id("slash-menu")
            .occlude()
            .w(px(240.))
            .max_h(px(320.))
            .overflow_y_scroll()
            .track_scroll(&self.slash_scroll)
            .mt_1()
            .p_1()
            .rounded_xl()
            .bg(theme.popover)
            .border_1()
            .border_color(theme.border)
            .text_color(theme.popover_foreground)
            .child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(if items.is_empty() {
                        "No matching blocks"
                    } else {
                        "Blocks"
                    }),
            )
            .children(rows);
        deferred(anchored().snap_to_window_with_margin(px(8.)).child(body))
            .with_priority(1)
            .into_any_element()
    }
}
