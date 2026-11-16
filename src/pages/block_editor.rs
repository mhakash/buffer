//! Notion-style rich editing of a note body. The body is a list of blocks (`notes::blocks`);
//! every block renders formatted, and the one being edited swaps in a borderless textarea
//! holding its inline Markdown. One `TextareaState` is shared by whichever block is active.
//!
//! Keys (captured before the textarea sees them): Enter splits, Backspace at the start turns
//! the block into text or joins it to the one above, arrows cross block edges, Tab/⇧Tab nest
//! list items, `/` opens the block menu, Markdown prefixes (`# `, `- `, `[] `, `> `, ```` ``` ````,
//! `---`) convert the block. ⌘B/⌘I/⌘E/⌘⇧X/⌘K wrap the selection in Markdown marks.

use std::{cell::Cell, rc::Rc};

use gpui_kit::component::{
    ActiveTheme, Icon, Sizable, Size, Theme,
    button::{Button, ButtonRounded, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    input::{
        Backspace, BufferPoint, Delete, Enter, Escape, IndentInline, InputEvent, MoveDown,
        MoveLeft, MoveRight, MoveUp, OutdentInline, Textarea, TextareaState,
    },
    menu::{DropdownMenu, PopupMenuItem},
    v_flex,
};
use gpui_kit::{
    AnyElement, Bounds, Context, Entity, EventEmitter, Focusable, FontWeight, Hsla, Modifiers,
    MouseButton, MouseDownEvent, MouseUpEvent, Pixels, PlatformInput, Point, Subscription, Window,
    anchored, assets::IconName, canvas, deferred, div, point, prelude::*, px,
};

use crate::{
    actions::{Bold, InlineCode, Italic, Link, Strikethrough},
    components::{confirm::confirm_delete, document_style, markdown_view},
    notes::{
        NoteStore,
        blocks::{self, Doc, Kind, inline_markdown},
    },
};

/// Room left of the text for the hover handles (inside the document padding).
const GUTTER: f32 = 50.;
const INDENT: f32 = 24.;
const MARKER_W: f32 = 24.;

pub enum BlockEditorEvent {
    /// The body changed; the Markdown to save.
    Changed(String),
    /// Esc with no block menu open: the page takes focus back.
    Exit,
}

enum Caret {
    Start,
    End,
    At(usize),
}

struct MenuItem {
    label: &'static str,
    keywords: &'static str,
    icon: IconName,
    kind: Kind,
}

fn menu_items() -> Vec<MenuItem> {
    let item = |label, keywords, icon, kind| MenuItem {
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

fn kind_label(kind: &Kind) -> &'static str {
    match kind {
        Kind::Paragraph => "Text",
        Kind::Heading(1) => "Heading 1",
        Kind::Heading(2) => "Heading 2",
        Kind::Heading(_) => "Heading 3",
        Kind::Bullet => "List",
        Kind::Numbered => "List",
        Kind::Todo(_) => "To-do",
        Kind::Quote => "Quote",
        Kind::Code(lang) if lang == "mermaid" => "Mermaid diagram source",
        Kind::Code(_) => "Code",
        Kind::Divider | Kind::Raw => "Markdown",
    }
}

/// `/query` typed into a text block opens the block menu.
fn slash_query(text: &str) -> Option<String> {
    let query = text.strip_prefix('/')?;
    (query.len() <= 24 && !query.contains(char::is_whitespace)).then(|| query.to_lowercase())
}

/// Text size, weight and line height (px) per kind; the textarea and the rendered block use
/// the same values so swapping between them doesn't move the text.
fn metrics(kind: &Kind) -> (f32, FontWeight, f32) {
    match kind {
        Kind::Heading(1) => (26., FontWeight::BOLD, 34.),
        Kind::Heading(2) => (21., FontWeight::BOLD, 29.),
        Kind::Heading(_) => (18., FontWeight::SEMIBOLD, 26.),
        Kind::Code(_) | Kind::Raw => (14., FontWeight::NORMAL, 21.),
        _ => (16., FontWeight::NORMAL, 26.),
    }
}

/// Space above a block, so headings start sections.
fn space_above(kind: &Kind, first: bool) -> f32 {
    match kind {
        _ if first => 0.,
        Kind::Heading(1) => 20.,
        Kind::Heading(2) => 14.,
        Kind::Heading(_) => 10.,
        Kind::Code(_) | Kind::Raw | Kind::Divider => 6.,
        _ => 1.,
    }
}

pub struct BlockEditor {
    doc: Doc,
    active: Option<usize>,
    input: Entity<TextareaState>,
    /// Highlighted row of the open `/` menu.
    menu: Option<usize>,
    /// Where the active textarea was laid out, for replaying the click that opened it.
    active_bounds: Rc<Cell<Bounds<Pixels>>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<BlockEditorEvent> for BlockEditor {}

impl BlockEditor {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 5000));
        let subscriptions = vec![cx.subscribe_in(&input, window, Self::on_input_event)];
        Self {
            doc: Doc::default(),
            active: None,
            input,
            menu: None,
            active_bounds: Rc::default(),
            _subscriptions: subscriptions,
        }
    }

    /// Show `markdown` (a different note, or the source was edited elsewhere).
    pub fn load(&mut self, markdown: &str, cx: &mut Context<Self>) {
        self.doc = blocks::parse(markdown);
        self.active = None;
        self.menu = None;
        cx.notify();
    }

    /// Start writing at the top (Enter in the title).
    pub fn focus_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.doc.blocks.first().is_some_and(|b| b.kind.is_text()) {
            let block = self.doc.block(Kind::Paragraph, "");
            self.doc.blocks.insert(0, block);
        }
        self.activate(0, Caret::Start, window, cx);
    }

    /// Keep the active block focused (page navigation).
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.active.is_some() {
            self.input.update(cx, |i, cx| i.focus(window, cx));
        }
        self.active.is_some()
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(BlockEditorEvent::Changed(self.doc.to_markdown()));
        cx.notify();
    }

    fn text(&self, cx: &Context<Self>) -> String {
        self.input.read(cx).value().to_string()
    }

    fn activate(&mut self, ix: usize, caret: Caret, window: &mut Window, cx: &mut Context<Self>) {
        let Some(block) = self.doc.blocks.get(ix) else {
            return;
        };
        if block.kind == Kind::Divider {
            return;
        }
        self.active = Some(ix);
        self.menu = slash_query(&block.text).map(|_| 0);
        let text = block.text.clone();
        let offset = match caret {
            Caret::Start => 0,
            Caret::End => text.len(),
            Caret::At(n) => n.min(text.len()),
        };
        let placeholder = match &block.kind {
            Kind::Paragraph => "Type ‘/’ for commands",
            kind => kind_label(kind),
        };
        self.input.update(cx, |input, cx| {
            input.set_value(text, window, cx);
            input.set_placeholder(placeholder, window, cx);
            input.set_selected_range(offset..offset, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn deactivate(&mut self, cx: &mut Context<Self>) {
        if self.active.take().is_some() {
            self.menu = None;
            cx.notify();
        }
    }

    /// A click on a rendered block: edit it and put the caret where the click was, by
    /// replaying the click into the textarea once it's laid out in the block's place.
    fn click_block(
        &mut self,
        ix: usize,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate(ix, Caret::End, window, cx);
        let bounds = self.active_bounds.clone();
        bounds.set(Bounds::default());
        window.on_next_frame(move |window, cx| {
            let b = bounds.get();
            if b.size.width <= px(0.) {
                return;
            }
            let position = point(
                position.x.clamp(b.left() + px(1.), b.right() - px(1.)),
                position.y.clamp(b.top() + px(1.), b.bottom() - px(1.)),
            );
            let modifiers = Modifiers::default();
            window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers,
                    click_count: 1,
                    first_mouse: false,
                }),
                cx,
            );
            window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers,
                    click_count: 1,
                }),
                cx,
            );
        });
    }

    /// Click under the last block: continue writing there.
    fn click_tail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let last_empty = self
            .doc
            .blocks
            .last()
            .is_some_and(|b| b.kind == Kind::Paragraph && b.text.is_empty());
        if !last_empty {
            let block = self.doc.block(Kind::Paragraph, "");
            self.doc.blocks.push(block);
        }
        self.activate(self.doc.blocks.len() - 1, Caret::Start, window, cx);
    }

    fn on_input_event(
        &mut self,
        _: &Entity<TextareaState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => self.on_change(window, cx),
            // Clicking another block refocuses the same input; only a real focus loss ends
            // editing.
            InputEvent::Blur => cx.defer_in(window, |this, window, cx| {
                if !this.input.focus_handle(cx).is_focused(window) {
                    this.deactivate(cx);
                }
            }),
            _ => {}
        }
    }

    fn on_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        let text = self.text(cx);
        let kind = self.doc.blocks[ix].kind.clone();
        if let Some((new_kind, len)) = blocks::shortcut(&kind, &text) {
            let rest = text[len..].to_string();
            self.convert(ix, new_kind, rest, window, cx);
            return;
        }
        self.doc.blocks[ix].set_text(text.clone());
        self.menu = if kind.is_text() && slash_query(&text).is_some() {
            Some(self.menu.unwrap_or(0))
        } else {
            None
        };
        self.changed(cx);
    }

    /// Turn block `ix` into `kind` with `text` and keep editing it (a divider moves on to a
    /// new paragraph below).
    fn convert(
        &mut self,
        ix: usize,
        kind: Kind,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let block = &mut self.doc.blocks[ix];
        block.set_kind(kind.clone());
        if kind == Kind::Divider {
            let next = self.doc.block(Kind::Paragraph, text);
            self.doc.blocks.insert(ix + 1, next);
            self.activate(ix + 1, Caret::Start, window, cx);
        } else {
            block.set_text(text);
            self.activate(ix, Caret::Start, window, cx);
        }
        self.changed(cx);
    }

    fn filtered_menu(&self, cx: &Context<Self>) -> Vec<MenuItem> {
        let query = slash_query(&self.text(cx)).unwrap_or_default();
        menu_items()
            .into_iter()
            .filter(|i| {
                i.label.to_lowercase().contains(&query) || i.keywords.contains(query.as_str())
            })
            .collect()
    }

    fn apply_menu(&mut self, item: usize, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(ix), Some(item)) = (self.active, self.filtered_menu(cx).into_iter().nth(item))
        else {
            return;
        };
        self.menu = None;
        self.convert(ix, item.kind, String::new(), window, cx);
    }

    fn insert_after(&mut self, ix: usize, kind: Kind, window: &mut Window, cx: &mut Context<Self>) {
        let block = self.doc.block(kind, "");
        self.doc.blocks.insert(ix + 1, block);
        self.activate(ix + 1, Caret::Start, window, cx);
        self.changed(cx);
    }

    // ---- keys, captured on the editor before the textarea handles them ----

    fn on_enter(&mut self, action: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        if let Some(sel) = self.menu {
            cx.stop_propagation();
            self.apply_menu(sel, window, cx);
            return;
        }
        let block = &self.doc.blocks[ix];
        let (kind, indent) = (block.kind.clone(), block.indent);
        if !kind.is_text() {
            // Code keeps newlines; ⌘Enter leaves it.
            if action.secondary {
                cx.stop_propagation();
                self.insert_after(ix, Kind::Paragraph, window, cx);
            }
            return;
        }
        if action.shift {
            return; // line break inside the block
        }
        cx.stop_propagation();
        let text = self.text(cx);
        let range = self.input.read(cx).selected_range();
        let (before, after) = (&text[..range.start], &text[range.end..]);

        // Enter on an empty item leaves the list (outdents first), like Notion.
        if text.is_empty() && (kind.is_list() || kind == Kind::Quote) {
            let block = &mut self.doc.blocks[ix];
            if indent > 0 {
                block.set_indent(indent - 1);
            } else {
                block.set_kind(Kind::Paragraph);
            }
            self.activate(ix, Caret::Start, window, cx);
            self.changed(cx);
            return;
        }
        let next_kind = match kind.clone() {
            Kind::Todo(_) => Kind::Todo(false),
            k if k.is_list() => k,
            _ => Kind::Paragraph,
        };
        if before.is_empty() && !after.is_empty() {
            // At the start: push the block down, keep the caret.
            let mut block = self.doc.block(
                if kind.is_list() {
                    next_kind
                } else {
                    Kind::Paragraph
                },
                "",
            );
            block.indent = indent;
            self.doc.blocks.insert(ix, block);
            self.active = Some(ix + 1);
            self.changed(cx);
            return;
        }
        let after = after.to_string();
        self.doc.blocks[ix].set_text(before);
        let mut block = self.doc.block(next_kind, after);
        block.indent = indent;
        self.doc.blocks.insert(ix + 1, block);
        self.activate(ix + 1, Caret::Start, window, cx);
        self.changed(cx);
    }

    fn on_backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        let range = self.input.read(cx).selected_range();
        if !range.is_empty() || range.start > 0 {
            return;
        }
        let text = self.text(cx);
        let kind = self.doc.blocks[ix].kind.clone();
        if !kind.is_text() && !text.is_empty() {
            return;
        }
        cx.stop_propagation();
        if kind != Kind::Paragraph {
            self.doc.blocks[ix].set_kind(Kind::Paragraph);
            self.activate(ix, Caret::Start, window, cx);
            self.changed(cx);
            return;
        }
        if ix == 0 {
            return;
        }
        let prev = &mut self.doc.blocks[ix - 1];
        match &prev.kind {
            Kind::Divider => {
                self.doc.blocks.remove(ix - 1);
                self.active = Some(ix - 1);
            }
            k if k.is_text() => {
                let offset = prev.text.len();
                prev.set_text(format!("{}{text}", prev.text));
                self.doc.blocks.remove(ix);
                self.activate(ix - 1, Caret::At(offset), window, cx);
            }
            _ => {
                if text.is_empty() {
                    self.doc.blocks.remove(ix);
                }
                self.activate(ix - 1, Caret::End, window, cx);
            }
        }
        self.changed(cx);
    }

    /// Forward delete at the end pulls the next text block up.
    fn on_delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        let text = self.text(cx);
        let range = self.input.read(cx).selected_range();
        let joinable = |k: &Kind| k.is_text();
        if !range.is_empty()
            || range.start < text.len()
            || !joinable(&self.doc.blocks[ix].kind)
            || !self
                .doc
                .blocks
                .get(ix + 1)
                .is_some_and(|b| joinable(&b.kind))
        {
            return;
        }
        cx.stop_propagation();
        let next = self.doc.blocks.remove(ix + 1);
        self.doc.blocks[ix].set_text(format!("{text}{}", next.text));
        self.activate(ix, Caret::At(text.len()), window, cx);
        self.changed(cx);
    }

    /// (display row of the caret, display rows) of the active textarea.
    fn caret_row(&self, cx: &Context<Self>) -> (usize, usize) {
        let input = self.input.read(cx);
        let pos = input.cursor_position();
        let map = input.display_map();
        let row = map
            .buffer_pos_to_display_pos(BufferPoint::new(pos.line as usize, pos.character as usize))
            .row;
        (row, map.display_row_count().max(1))
    }

    /// Nearest editable block from `ix` going `step` (dividers are skipped).
    fn neighbour(&self, ix: usize, step: isize) -> Option<usize> {
        let mut i = ix as isize + step;
        while let Some(block) = usize::try_from(i).ok().and_then(|i| self.doc.blocks.get(i)) {
            if block.kind != Kind::Divider {
                return Some(i as usize);
            }
            i += step;
        }
        None
    }

    fn move_menu(&mut self, step: isize, cx: &mut Context<Self>) -> bool {
        let Some(sel) = self.menu else { return false };
        let n = self.filtered_menu(cx).len().max(1) as isize;
        self.menu = Some((sel as isize + step).rem_euclid(n) as usize);
        cx.notify();
        true
    }

    fn on_up(&mut self, _: &MoveUp, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        if self.move_menu(-1, cx) {
            cx.stop_propagation();
        } else if self.caret_row(cx).0 == 0
            && let Some(prev) = self.neighbour(ix, -1)
        {
            cx.stop_propagation();
            self.activate(prev, Caret::End, window, cx);
        }
    }

    fn on_down(&mut self, _: &MoveDown, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        if self.move_menu(1, cx) {
            cx.stop_propagation();
            return;
        }
        let (row, rows) = self.caret_row(cx);
        if row + 1 >= rows
            && let Some(next) = self.neighbour(ix, 1)
        {
            cx.stop_propagation();
            self.activate(next, Caret::Start, window, cx);
        }
    }

    fn on_left(&mut self, _: &MoveLeft, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        let range = self.input.read(cx).selected_range();
        if range.is_empty()
            && range.start == 0
            && let Some(prev) = self.neighbour(ix, -1)
        {
            cx.stop_propagation();
            self.activate(prev, Caret::End, window, cx);
        }
    }

    fn on_right(&mut self, _: &MoveRight, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        let range = self.input.read(cx).selected_range();
        if range.is_empty()
            && range.start >= self.text(cx).len()
            && let Some(next) = self.neighbour(ix, 1)
        {
            cx.stop_propagation();
            self.activate(next, Caret::Start, window, cx);
        }
    }

    fn on_indent(&mut self, step: isize, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        let block = &self.doc.blocks[ix];
        if !block.kind.is_text() {
            return; // Tab in code
        }
        cx.stop_propagation();
        if !block.kind.is_list() {
            return;
        }
        // An item nests at most one level under the item above it.
        let max = match ix.checked_sub(1).map(|i| &self.doc.blocks[i]) {
            Some(prev) if prev.kind.is_list() => prev.indent + 1,
            _ => 0,
        };
        let indent = (block.indent as isize + step).clamp(0, max as isize) as usize;
        self.doc.blocks[ix].set_indent(indent);
        self.changed(cx);
    }

    fn on_escape(&mut self, _: &Escape, _: &mut Window, cx: &mut Context<Self>) {
        if self.active.is_none() {
            return;
        }
        cx.stop_propagation();
        if self.menu.take().is_some() {
            cx.notify();
        } else {
            self.deactivate(cx);
            cx.emit(BlockEditorEvent::Exit);
        }
    }

    fn toggle_mark(&mut self, marker: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        if !self.doc.blocks[ix].kind.is_text() {
            return;
        }
        let text = self.text(cx);
        let (new, sel) = blocks::toggle_mark(&text, self.input.read(cx).selected_range(), marker);
        self.input.update(cx, |input, cx| {
            input.replace_all(new, window, cx);
            input.set_selected_range(sel, cx);
        });
    }

    /// ⌘K: `[selection](|)`.
    fn link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        if !self.doc.blocks[ix].kind.is_text() {
            return;
        }
        let text = self.text(cx);
        let range = self.input.read(cx).selected_range();
        let label = &text[range.clone()];
        let new = format!("{}[{label}](){}", &text[..range.start], &text[range.end..]);
        let caret = range.start + label.len() + 3;
        self.input.update(cx, |input, cx| {
            input.replace_all(new, window, cx);
            input.set_selected_range(caret..caret, cx);
        });
    }

    // ---- block handle menu ----

    fn index_of(&self, id: u64) -> Option<usize> {
        self.doc.blocks.iter().position(|b| b.id == id)
    }

    fn turn_into(&mut self, id: u64, kind: Kind, cx: &mut Context<Self>) {
        let Some(ix) = self.index_of(id) else { return };
        self.deactivate(cx);
        self.doc.blocks[ix].set_kind(kind);
        self.changed(cx);
    }

    fn move_block(&mut self, id: u64, step: isize, cx: &mut Context<Self>) {
        let Some(ix) = self.index_of(id) else { return };
        let to = ix as isize + step;
        if to < 0 || to as usize >= self.doc.blocks.len() {
            return;
        }
        self.deactivate(cx);
        self.doc.blocks.swap(ix, to as usize);
        // Renumber on save.
        self.doc.blocks[ix].touch();
        self.doc.blocks[to as usize].touch();
        self.changed(cx);
    }

    fn duplicate(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(ix) = self.index_of(id) else { return };
        self.deactivate(cx);
        let src = &self.doc.blocks[ix];
        let (kind, indent, text) = (src.kind.clone(), src.indent, src.text.clone());
        let mut copy = self.doc.block(kind, text);
        copy.indent = indent;
        self.doc.blocks.insert(ix + 1, copy);
        self.changed(cx);
    }

    fn delete(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(ix) = self.index_of(id) else { return };
        self.deactivate(cx);
        self.doc.blocks.remove(ix);
        self.changed(cx);
    }

    fn toggle_todo(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(ix) = self.index_of(id) else { return };
        if let Kind::Todo(done) = self.doc.blocks[ix].kind {
            self.doc.blocks[ix].set_kind(Kind::Todo(!done));
            self.changed(cx);
        }
    }

    /// "+": a new block below with the block menu open.
    fn add_below(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.index_of(id) else { return };
        let block = self.doc.block(Kind::Paragraph, "/");
        self.doc.blocks.insert(ix + 1, block);
        self.activate(ix + 1, Caret::End, window, cx);
        self.changed(cx);
    }

    // ---- rendering ----

    fn render_handles(&self, id: u64, top: f32, theme: &Theme, cx: &Context<Self>) -> AnyElement {
        let this = cx.entity().downgrade();
        let muted = theme.muted_foreground;
        let add = Button::new(("blk-add", id))
            .ghost()
            .xsmall()
            .rounded(ButtonRounded::Size(px(6.)))
            .icon(Icon::new(IconName::Plus).text_color(muted))
            .tooltip("Add a block below")
            .on_click(cx.listener(move |this, _, window, cx| this.add_below(id, window, cx)));
        let grip = Button::new(("blk-grip", id))
            .ghost()
            .xsmall()
            .rounded(ButtonRounded::Size(px(6.)))
            .icon(Icon::new(IconName::GripVertical).text_color(muted))
            .tooltip("Block options")
            .dropdown_menu(move |mut menu, window, cx| {
                let on = |f: fn(&mut BlockEditor, u64, &mut Context<BlockEditor>)| {
                    let this = this.clone();
                    move |_: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut gpui_kit::App| {
                        this.update(cx, |e, cx| f(e, id, cx)).ok();
                    }
                };
                let this2 = this.clone();
                menu = menu.submenu("Turn into", window, cx, move |mut menu, _, _| {
                    for item in menu_items() {
                        let this = this2.clone();
                        let kind = item.kind.clone();
                        menu = menu.item(
                            PopupMenuItem::new(item.label)
                                .icon(Icon::new(item.icon))
                                .on_click(move |_, _, cx| {
                                    this.update(cx, |e, cx| e.turn_into(id, kind.clone(), cx))
                                        .ok();
                                }),
                        );
                    }
                    menu.min_w(px(200.))
                });
                let this3 = this.clone();
                menu.separator()
                    .item(
                        PopupMenuItem::new("Move up")
                            .icon(Icon::new(IconName::ArrowUp))
                            .on_click(on(|e, id, cx| e.move_block(id, -1, cx))),
                    )
                    .item(
                        PopupMenuItem::new("Move down")
                            .icon(Icon::new(IconName::ArrowDown))
                            .on_click(on(|e, id, cx| e.move_block(id, 1, cx))),
                    )
                    .item(
                        PopupMenuItem::new("Duplicate")
                            .icon(Icon::new(IconName::Copy))
                            .on_click(on(BlockEditor::duplicate)),
                    )
                    .separator()
                    .item(
                        PopupMenuItem::new("Delete")
                            .icon(Icon::new(IconName::Trash))
                            .on_click(move |_, window, cx| {
                                let this = this3.clone();
                                confirm_delete(
                                    window,
                                    cx,
                                    "Delete block?",
                                    "This block will be removed from the note.",
                                    move |_, cx| {
                                        this.update(cx, |e, cx| e.delete(id, cx)).ok();
                                    },
                                );
                            }),
                    )
                    .min_w(px(180.))
            });
        h_flex()
            .absolute()
            .left(px(-GUTTER))
            .top(px(top))
            .w(px(GUTTER - 4.))
            .justify_end()
            .opacity(0.)
            .group_hover(format!("blk-{id}"), |s| s.opacity(1.))
            .child(add)
            .child(grip)
            .into_any_element()
    }

    fn render_marker(
        &self,
        kind: &Kind,
        id: u64,
        number: usize,
        line_h: f32,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let cell = || {
            h_flex()
                .flex_none()
                .w(px(MARKER_W))
                .h(px(line_h))
                .items_center()
        };
        Some(match kind {
            Kind::Bullet => cell()
                .justify_center()
                .child(div().size(px(5.)).rounded_full().bg(theme.foreground))
                .into_any_element(),
            Kind::Numbered => cell()
                .justify_end()
                .pr_1p5()
                .child(format!("{number}."))
                .into_any_element(),
            Kind::Todo(done) => {
                cell()
                    .child(Checkbox::new(("blk-todo", id)).checked(*done).on_click(
                        cx.listener(move |this, _: &bool, _, cx| this.toggle_todo(id, cx)),
                    ))
                    .into_any_element()
            }
            _ => return None,
        })
    }

    fn render_menu(&self, theme: &Theme, cx: &Context<Self>) -> AnyElement {
        let items = self.filtered_menu(cx);
        let selected = self.menu.unwrap_or(0).min(items.len().saturating_sub(1));
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
                        this.apply_menu(i, window, cx);
                    }),
                )
        });
        let body = v_flex()
            .id("slash-menu")
            .occlude()
            .w(px(240.))
            .max_h(px(320.))
            .overflow_y_scroll()
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

    fn render_block(
        &self,
        ix: usize,
        number: usize,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        let block = &self.doc.blocks[ix];
        let id = block.id;
        let kind = &block.kind;
        let active = self.active == Some(ix);
        let (size, weight, line_h) = metrics(kind);
        let above = space_above(kind, ix == 0);
        let dir = NoteStore::dir();

        let row = h_flex()
            .id(("blk-row", id))
            .group(format!("blk-{id}"))
            .relative()
            .items_start()
            .pt(px(above))
            .pl(px(block.indent as f32 * INDENT));

        if *kind == Kind::Divider {
            return row
                .h(px(above + 25.))
                .items_center()
                .child(div().flex_1().h(px(1.)).bg(theme.border))
                .child(self.render_handles(id, above, theme, cx))
                .into_any_element();
        }

        let code_like = !kind.is_text();
        let done = matches!(kind, Kind::Todo(true));
        let body: AnyElement = if active {
            let bounds = self.active_bounds.clone();
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .relative()
                        .when(code_like, |d| {
                            d.px_3().py_2().rounded_xl().bg(theme.secondary)
                        })
                        .child(
                            Textarea::new(&self.input)
                                .appearance(false)
                                .with_size(Size::XSmall)
                                .ml(px(-4.))
                                .text_size(px(size))
                                .line_height(px(line_h))
                                .font_weight(weight)
                                .when(code_like, |t| t.font_family(theme.mono_font_family.clone())),
                        )
                        .child(
                            canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                                .absolute()
                                .size_full(),
                        ),
                )
                .when(self.menu.is_some(), |this| {
                    this.child(self.render_menu(theme, cx))
                })
                .into_any_element()
        } else if code_like {
            div()
                .flex_1()
                .min_w_0()
                .cursor_text()
                .text_size(px(16.))
                .child(
                    markdown_view(("blk-md", id), block.source(), document_style(), dir)
                        .selectable(false),
                )
                .into_any_element()
        } else if block.text.trim().is_empty() {
            div()
                .flex_1()
                .h(px(line_h))
                .cursor_text()
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_w_0()
                .cursor_text()
                .text_size(px(size))
                .line_height(px(line_h))
                .font_weight(weight)
                .when(done, |d| {
                    d.text_color(theme.muted_foreground).line_through()
                })
                .child(
                    markdown_view(
                        ("blk-md", id),
                        inline_markdown(&block.text),
                        document_style().paragraph_gap(gpui_kit::rems(0.)),
                        dir,
                    )
                    .selectable(false),
                )
                .into_any_element()
        };

        let quote = *kind == Kind::Quote;
        let content = h_flex()
            .flex_1()
            .min_w_0()
            .items_start()
            .when(quote, |d| {
                d.pl_3()
                    .border_l_2()
                    .border_color(Hsla::from(theme.foreground).opacity(0.7))
            })
            .children(self.render_marker(kind, id, number, line_h, theme, cx))
            .child(body);

        let handle_top = above + if code_like { 4. } else { (line_h - 22.) / 2. };
        row.child(content)
            .child(self.render_handles(id, handle_top, theme, cx))
            .when(!active, |row| {
                row.on_click(
                    cx.listener(move |this, event: &gpui_kit::ClickEvent, window, cx| {
                        if let Some(ix) = this.index_of(id) {
                            this.click_block(ix, event.position(), window, cx);
                        }
                    }),
                )
            })
            .into_any_element()
    }
}

impl gpui_kit::Render for BlockEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let numbers = self.doc.numbers();
        let rows: Vec<AnyElement> = (0..self.doc.blocks.len())
            .map(|ix| self.render_block(ix, numbers[ix], &theme, cx))
            .collect();
        v_flex()
            .key_context("BlockEditor")
            .capture_action(cx.listener(Self::on_enter))
            .capture_action(cx.listener(Self::on_backspace))
            .capture_action(cx.listener(Self::on_delete))
            .capture_action(cx.listener(Self::on_up))
            .capture_action(cx.listener(Self::on_down))
            .capture_action(cx.listener(Self::on_left))
            .capture_action(cx.listener(Self::on_right))
            .capture_action(cx.listener(Self::on_escape))
            .capture_action(cx.listener(|this, _: &IndentInline, _, cx| this.on_indent(1, cx)))
            .capture_action(cx.listener(|this, _: &OutdentInline, _, cx| this.on_indent(-1, cx)))
            .on_action(cx.listener(|this, _: &Bold, w, cx| this.toggle_mark("**", w, cx)))
            .on_action(cx.listener(|this, _: &Italic, w, cx| this.toggle_mark("_", w, cx)))
            .on_action(cx.listener(|this, _: &InlineCode, w, cx| this.toggle_mark("`", w, cx)))
            .on_action(cx.listener(|this, _: &Strikethrough, w, cx| this.toggle_mark("~~", w, cx)))
            .on_action(cx.listener(|this, _: &Link, w, cx| this.link(w, cx)))
            .text_base()
            .children(rows)
            .child(
                div()
                    .id("blk-tail")
                    .w_full()
                    .min_h(px(160.))
                    .cursor_text()
                    .on_click(cx.listener(|this, _, window, cx| this.click_tail(window, cx))),
            )
    }
}
