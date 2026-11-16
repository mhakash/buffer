//! Notion-style block editor over Markdown, shared by notes and task notes.
//!
//! The body is a list of blocks (`model`). Every block renders formatted; the one being edited
//! swaps in a borderless textarea holding its inline Markdown (one `TextareaState`, shared).
//! Blocks can be selected (Esc, shift-arrows, a drag across blocks, the handle), then copied,
//! cut, pasted, deleted, duplicated, moved (⌘⇧↑↓ or by dragging the handle) and undone.
//!
//! | File | Concern |
//! |---|---|
//! | `model` | Markdown ⇄ blocks, range operations (pure, tested) |
//! | `history` | undo/redo snapshots |
//! | `keymap` | actions and key bindings |
//! | `keys` | routes key actions to editing or selection handlers |
//! | `editing` | keys inside the active block, Markdown shortcuts, inline marks |
//! | `slash` | the `/` block menu (its own search field) |
//! | `table` | table cells: navigation, rows and columns |
//! | `selection` | block selection by keyboard and mouse |
//! | `ops` | structural changes on block ranges |
//! | `clipboard` | copy / cut / paste |
//! | `drag` | drag handle, drop indicator |
//! | `block_menu` | the handle's menu |
//! | `render` | how blocks look |

mod block_menu;
mod clipboard;
mod drag;
mod editing;
mod history;
mod keymap;
mod keys;
pub mod model;
mod ops;
mod render;
mod selection;
mod slash;
mod table;

use std::{cell::RefCell, path::PathBuf, rc::Rc};

use gpui_kit::component::{
    ActiveTheme,
    input::{InputEvent, InputState, TextareaState},
};
use gpui_kit::{
    AnyElement, App, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, Modifiers,
    MouseButton, MouseDownEvent, MouseUpEvent, Pixels, PlatformInput, Point, ScrollHandle,
    Subscription, Window, div, point, prelude::*, px,
};

use block_menu::BlockMenu;
use history::History;
use model::{Doc, Kind, Table};
use selection::{Press, Selection};
use slash::kind_label;

pub use keymap::init;

pub enum BlockEditorEvent {
    /// The body changed; the Markdown to save.
    Changed(String),
    /// Esc with nothing left to step back from: the page takes focus back.
    Exit,
}

#[derive(Clone, Copy)]
enum Caret {
    Start,
    End,
    At(usize),
}

pub struct BlockEditor {
    doc: Doc,
    /// Block whose textarea is shown.
    active: Option<usize>,
    /// Text was typed since the active block opened (⌘Z goes to its textarea first).
    typed: bool,
    input: Entity<TextareaState>,
    /// Highlighted row of the open `/` menu.
    slash: Option<usize>,
    slash_scroll: ScrollHandle,
    /// The `/` menu's search field; it has focus while the menu is open.
    slash_input: Entity<InputState>,
    /// Put back before the query when the menu closes unused (`/` when it was typed).
    slash_prefix: &'static str,
    /// (row, column) being edited when the active block is a table.
    cell: (usize, usize),
    selection: Option<Selection>,
    /// Left button held after pressing on a block (drag-select across blocks).
    press: Option<Press>,
    /// The press became a block selection; the click that ends it must not open a block.
    drag_selecting: bool,
    /// While dragging blocks: the ids dragged and the insertion index under the pointer.
    dragging: Option<Vec<u64>>,
    drop_at: Option<usize>,
    block_menu: Option<BlockMenu>,
    history: History,
    /// Each row's bounds from the last frame (pointer → block).
    row_bounds: Rc<RefCell<Vec<Bounds<Pixels>>>>,
    /// Holds focus while blocks are selected.
    focus_handle: FocusHandle,
    /// Folder relative image paths resolve against (the note's or task's).
    dir: PathBuf,
    /// Smaller type and a grip-only gutter, for the task side panel.
    compact: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<BlockEditorEvent> for BlockEditor {}

impl Focusable for BlockEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl BlockEditor {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 5000));
        let slash_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search blocks"));
        let focus_handle = cx.focus_handle();
        let subscriptions = vec![
            cx.subscribe_in(&input, window, Self::on_input_event),
            cx.subscribe_in(&slash_input, window, Self::on_slash_input_event),
            // Clicking away clears the selection (not while its menu has focus).
            cx.on_blur(&focus_handle, window, |this, _, cx| {
                if this.block_menu.is_none() && this.selection.take().is_some() {
                    cx.notify();
                }
            }),
        ];
        Self {
            doc: Doc::default(),
            active: None,
            typed: false,
            input,
            slash: None,
            slash_scroll: ScrollHandle::new(),
            slash_input,
            slash_prefix: "",
            cell: (0, 0),
            selection: None,
            press: None,
            drag_selecting: false,
            dragging: None,
            drop_at: None,
            block_menu: None,
            history: History::default(),
            row_bounds: Rc::default(),
            focus_handle,
            dir: PathBuf::new(),
            compact: false,
            _subscriptions: subscriptions,
        }
    }

    /// Show `markdown` (another note, or the source was edited elsewhere); relative images
    /// resolve against `dir`. Starts a fresh undo history.
    pub fn load(&mut self, markdown: &str, dir: PathBuf, cx: &mut Context<Self>) {
        self.doc = model::parse(markdown);
        self.dir = dir;
        self.active = None;
        self.slash = None;
        self.selection = None;
        self.block_menu = None;
        self.history.clear();
        cx.notify();
    }

    pub fn set_compact(&mut self, compact: bool, cx: &mut Context<Self>) {
        if self.compact != compact {
            self.compact = compact;
            cx.notify();
        }
    }

    /// Start writing at the top (Enter in the title).
    pub fn focus_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.doc.blocks.first().is_some_and(|b| b.kind.is_text()) {
            self.checkpoint();
            let block = self.doc.block(Kind::Paragraph, "");
            self.doc.blocks.insert(0, block);
        }
        self.activate(0, Caret::Start, window, cx);
    }

    /// Keep focus in the editor if it had it (page navigation). False when it didn't.
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.slash.is_some() {
            self.slash_input.update(cx, |i, cx| i.focus(window, cx));
        } else if self.active.is_some() {
            self.input.update(cx, |i, cx| i.focus(window, cx));
        } else if self.selection.is_some() {
            self.focus_handle.focus(window, cx);
        }
        self.active.is_some() || self.selection.is_some()
    }

    // ---- shared helpers ----

    /// Room left of the text for the "+" and drag handles.
    fn gutter(&self) -> f32 {
        if self.compact { 40. } else { 48. }
    }

    /// Size of each gutter handle ("+" and grip); both fit in `gutter() - 4`.
    fn handle_size(&self) -> f32 {
        if self.compact { 18. } else { 22. }
    }

    /// How far the editor reaches into its host's left padding; the rest of the gutter
    /// indents the text (the side panel's padding is too narrow for both handles).
    fn reach(&self) -> f32 {
        if self.compact { 14. } else { 48. }
    }

    /// Save: emit the Markdown and repaint.
    fn changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(BlockEditorEvent::Changed(self.doc.to_markdown()));
        cx.notify();
    }

    /// Remember the current state for ⌘Z; call before a structural change.
    fn checkpoint(&mut self) {
        self.history.record(&self.doc);
    }

    fn restore(&mut self, redo: bool, window: &mut Window, cx: &mut Context<Self>) {
        let doc = if redo {
            self.history.redo(&self.doc)
        } else {
            self.history.undo(&self.doc)
        };
        if let Some(doc) = doc {
            self.doc = doc;
            self.active = None;
            self.slash = None;
            self.selection = None;
            self.focus_handle.focus(window, cx);
            self.changed(cx);
        }
    }

    fn text(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    fn index_of(&self, id: u64) -> Option<usize> {
        self.doc.blocks.iter().position(|b| b.id == id)
    }

    /// Edit block `ix`; a table opens its first cell (`Caret::End`: its last).
    fn activate(&mut self, ix: usize, caret: Caret, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(b) = self.doc.blocks.get(ix)
            && b.kind == Kind::Table
        {
            let table = Table::parse(&b.text);
            self.cell = match caret {
                Caret::End => (table.rows.len() - 1, table.cols() - 1),
                _ => (0, 0),
            };
        }
        self.open(ix, caret, window, cx);
    }

    /// Edit cell `(row, col)` of table block `ix`.
    fn activate_cell(
        &mut self,
        ix: usize,
        cell: (usize, usize),
        caret: Caret,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cell = cell;
        self.open(ix, caret, window, cx);
    }

    fn open(&mut self, ix: usize, caret: Caret, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .doc
            .blocks
            .get(ix)
            .is_none_or(|b| b.kind == Kind::Divider)
        {
            return;
        }
        // Edits made in the block being left become one undo step.
        self.checkpoint();
        let block = &self.doc.blocks[ix];
        self.active = Some(ix);
        self.typed = false;
        self.slash = None;
        self.selection = None;
        let text = if block.kind == Kind::Table {
            let table = Table::parse(&block.text);
            self.cell = (
                self.cell.0.min(table.rows.len() - 1),
                self.cell.1.min(table.cols() - 1),
            );
            table.cell(self.cell.0, self.cell.1).to_string()
        } else {
            block.text.clone()
        };
        let offset = match caret {
            Caret::Start => 0,
            Caret::End => text.len(),
            Caret::At(n) => n.min(text.len()),
        };
        let placeholder = placeholder(&block.kind);
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
            self.slash = None;
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
        // The input is shared: its last layout is the previous block's until it re-lays out.
        let stale = self.input.read(cx).text_bounds();
        self.activate(ix, Caret::End, window, cx);
        replay_click(self.input.clone(), stale, position, 5, window);
    }

    /// Click under the last block: continue writing there.
    fn click_tail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.drag_selecting {
            return;
        }
        let last_empty = self
            .doc
            .blocks
            .last()
            .is_some_and(|b| b.kind == Kind::Paragraph && b.text.is_empty());
        if !last_empty {
            self.checkpoint();
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
            InputEvent::Blur => cx.defer_in(window, Self::deactivate_if_unfocused),
            _ => {}
        }
    }
}

impl BlockEditor {
    /// After a blur: editing ends unless focus is still in the block or its `/` menu.
    fn deactivate_if_unfocused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.input.focus_handle(cx).is_focused(window)
            && !self.slash_input.focus_handle(cx).is_focused(window)
        {
            self.deactivate(cx);
        }
    }
}

impl Render for BlockEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A drag that ended outside the editor leaves no indicator behind.
        if !cx.has_active_drag() {
            self.dragging = None;
            self.drop_at = None;
        }
        let theme = cx.theme().clone();
        let numbers = self.doc.numbers();
        self.row_bounds
            .borrow_mut()
            .resize(self.doc.blocks.len(), Bounds::default());
        let rows: Vec<AnyElement> = (0..self.doc.blocks.len())
            .map(|ix| self.render_block(ix, numbers[ix], &theme, cx))
            .collect();
        let at_end = self.drop_at == Some(self.doc.blocks.len());

        let root = div()
            .id("block-editor")
            .flex()
            .flex_col()
            // Reach into the gutter like the rows do, so the handles are inside the editor
            // (drags over them count as over the editor).
            .ml(px(-self.reach()))
            .pl(px(self.gutter()))
            .track_focus(&self.focus_handle)
            .key_context(if self.active.is_some() {
                keymap::EDITING
            } else {
                keymap::SELECTING
            });
        self.bind_keys(root, cx)
            .capture_any_mouse_down(cx.listener(Self::on_press))
            .capture_any_mouse_up(cx.listener(|this, _: &MouseUpEvent, _, _| this.press = None))
            .on_mouse_move(cx.listener(Self::on_pointer_move))
            .on_drag_move(cx.listener(Self::on_drag_move))
            .on_drop(cx.listener(Self::on_drop_blocks))
            .children(rows)
            .child(
                div()
                    .id("blk-tail")
                    .relative()
                    .w_full()
                    .min_h(px(if self.compact { 40. } else { 160. }))
                    .cursor_text()
                    .when(at_end, |d| d.child(self.drop_line(0., &theme)))
                    .when(self.doc.blocks.is_empty(), |d| {
                        d.text_size(px(if self.compact { 14. } else { 16. }))
                            .text_color(theme.muted_foreground)
                            .child("Write something, or type ‘/’ for commands…")
                    })
                    .on_click(cx.listener(|this, _, window, cx| this.click_tail(window, cx))),
            )
            .children(self.render_block_menu())
    }
}

/// Hint in an empty block of `kind` being edited.
fn placeholder(kind: &Kind) -> &'static str {
    match kind {
        Kind::Paragraph => "Type ‘/’ for commands",
        Kind::Table => "",
        kind => kind_label(kind),
    }
}

/// Click into the active textarea at `position` once it has been laid out in the block's
/// place (its text bounds differ from `stale`), so the caret lands where the rendered text
/// was clicked. The click is clamped into the textarea so it can't hit another block.
fn replay_click(
    input: Entity<TextareaState>,
    stale: Option<Bounds<Pixels>>,
    position: Point<Pixels>,
    tries: u8,
    window: &mut Window,
) {
    window.refresh();
    window.on_next_frame(move |window, cx| {
        let Some(b) = input.read(cx).text_bounds().filter(|b| Some(*b) != stale) else {
            if tries > 0 {
                replay_click(input, stale, position, tries - 1, window);
            }
            return;
        };
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
