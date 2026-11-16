//! Editing inside the active block: Markdown shortcuts as you type, Enter/Backspace/Delete
//! across block boundaries, arrows between blocks, list nesting, inline marks. Handlers call
//! `cx.stop_propagation()` when they take a key; otherwise the textarea gets it.

use gpui_kit::component::input::{BufferPoint, Enter};
use gpui_kit::{Context, Window};

use super::{
    BlockEditor, Caret,
    model::{self, Kind},
    selection::Selection,
    slash::slash_query,
};

impl BlockEditor {
    pub(super) fn on_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        let text = self.text(cx);
        let kind = self.doc.blocks[ix].kind.clone();
        if kind == Kind::Table {
            self.set_cell_text(ix, &text);
            self.typed = true;
            self.changed(cx);
            return;
        }
        if let Some((new_kind, len)) = model::shortcut(&kind, &text) {
            let rest = text[len..].to_string();
            self.convert(ix, new_kind, rest, window, cx);
            return;
        }
        let was_query = slash_query(&self.doc.blocks[ix].text).is_some();
        self.doc.blocks[ix].set_text(text.clone());
        self.typed = true;
        self.update_slash(was_query, &text, &kind, window, cx);
        self.changed(cx);
    }

    /// Turn block `ix` into `kind` with `text` and keep editing it (a divider moves on to a
    /// new paragraph below).
    pub(super) fn convert(
        &mut self,
        ix: usize,
        kind: Kind,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.checkpoint();
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

    pub(super) fn insert_after(
        &mut self,
        ix: usize,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.checkpoint();
        let block = self.doc.block(Kind::Paragraph, text);
        self.doc.blocks.insert(ix + 1, block);
        self.activate(ix + 1, Caret::End, window, cx);
        self.changed(cx);
    }

    /// "+": a new block below with the `/` menu open.
    pub(super) fn add_below(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.index_of(id) else { return };
        self.insert_after(ix, "", window, cx);
        self.open_slash("", "", window, cx);
    }

    pub(super) fn toggle_todo(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(ix) = self.index_of(id) else { return };
        if let Kind::Todo(done) = self.doc.blocks[ix].kind {
            self.checkpoint();
            self.doc.blocks[ix].set_kind(Kind::Todo(!done));
            self.changed(cx);
        }
    }

    pub(super) fn edit_enter(
        &mut self,
        action: &Enter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.active else { return };
        if let Some(sel) = self.slash {
            cx.stop_propagation();
            self.apply_slash(sel, window, cx);
            return;
        }
        let block = &self.doc.blocks[ix];
        let (kind, indent) = (block.kind.clone(), block.indent);
        if kind == Kind::Table {
            cx.stop_propagation();
            if action.secondary {
                self.insert_after(ix, "", window, cx);
            } else {
                self.cell_below(ix, window, cx);
            }
            return;
        }
        if !kind.is_text() {
            // Code keeps newlines; ⌘Enter leaves it.
            if action.secondary {
                cx.stop_propagation();
                self.insert_after(ix, "", window, cx);
            }
            return;
        }
        if action.shift {
            return; // line break inside the block
        }
        cx.stop_propagation();
        self.checkpoint();
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
            let kind = if kind.is_list() {
                next_kind
            } else {
                Kind::Paragraph
            };
            let mut block = self.doc.block(kind, "");
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

    /// At the start of a block: turn it into text, then join it to the block above.
    pub(super) fn edit_backspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        if self.slash.is_some() {
            self.slash_backspace(window, cx);
            return;
        }
        if self.doc.blocks[ix].kind == Kind::Table {
            return; // only ever edits the cell
        }
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
            self.checkpoint();
            self.doc.blocks[ix].set_kind(Kind::Paragraph);
            self.activate(ix, Caret::Start, window, cx);
            self.changed(cx);
            return;
        }
        if ix == 0 {
            return;
        }
        self.checkpoint();
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
    pub(super) fn edit_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        if self.slash.is_some() {
            return;
        }
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
        self.checkpoint();
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

    fn on_first_row(&self, cx: &Context<Self>) -> bool {
        self.caret_row(cx).0 == 0
    }

    fn on_last_row(&self, cx: &Context<Self>) -> bool {
        let (row, rows) = self.caret_row(cx);
        row + 1 >= rows
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

    /// ↑/↓: move in the `/` menu, else to the next block from the first/last line.
    pub(super) fn edit_vertical(
        &mut self,
        step: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.active else { return };
        if self.move_slash(step, cx) {
            cx.stop_propagation();
            return;
        }
        let at_edge = if step < 0 {
            self.on_first_row(cx)
        } else {
            self.on_last_row(cx)
        };
        if at_edge && self.doc.blocks[ix].kind == Kind::Table {
            cx.stop_propagation();
            self.cell_vertical(ix, step, window, cx);
            return;
        }
        if at_edge && let Some(next) = self.neighbour(ix, step) {
            cx.stop_propagation();
            let caret = if step < 0 { Caret::End } else { Caret::Start };
            self.activate(next, caret, window, cx);
        }
    }

    /// From a table's first/last row, or a cell's edge: the block above/below.
    pub(super) fn leave_table(
        &mut self,
        ix: usize,
        step: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(next) = self.neighbour(ix, step) {
            let caret = if step < 0 { Caret::End } else { Caret::Start };
            self.activate(next, caret, window, cx);
        }
    }

    /// ←/→ at the very start/end crosses into the neighbouring block.
    pub(super) fn edit_horizontal(
        &mut self,
        step: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.active else { return };
        if self.slash.is_some() {
            return;
        }
        let range = self.input.read(cx).selected_range();
        let at_edge = range.is_empty()
            && if step < 0 {
                range.start == 0
            } else {
                range.start >= self.text(cx).len()
            };
        if at_edge && self.doc.blocks[ix].kind == Kind::Table {
            cx.stop_propagation();
            self.cell_step(ix, step, false, window, cx);
            return;
        }
        if at_edge && let Some(next) = self.neighbour(ix, step) {
            cx.stop_propagation();
            let caret = if step < 0 { Caret::End } else { Caret::Start };
            self.activate(next, caret, window, cx);
        }
    }

    /// ⇧↑/⇧↓ on the first/last line selects the block (then extends like Notion).
    pub(super) fn edit_extend(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        if self.slash.is_some() {
            return;
        }
        let at_edge = if step < 0 {
            self.on_first_row(cx)
        } else {
            self.on_last_row(cx)
        };
        if at_edge {
            cx.stop_propagation();
            self.select(Selection::one(ix), window, cx);
        }
    }

    /// ⌘A selects the block's text first, then every block.
    pub(super) fn edit_select_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.slash.is_some() {
            return;
        }
        let range = self.input.read(cx).selected_range();
        let len = self.text(cx).len();
        if range.start == 0 && range.end == len {
            cx.stop_propagation();
            self.select_all(window, cx);
        }
    }

    pub(super) fn edit_indent(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        if self.slash.is_some() {
            cx.stop_propagation();
            self.move_slash(step, cx);
            return;
        }
        if self.doc.blocks[ix].kind == Kind::Table {
            cx.stop_propagation();
            self.cell_step(ix, step, true, window, cx);
            return;
        }
        if !self.doc.blocks[ix].kind.is_text() {
            return; // Tab in code
        }
        cx.stop_propagation();
        self.indent_range(ix..ix + 1, step, cx);
    }

    /// Esc closes the `/` menu, else selects the block being edited (like Notion).
    pub(super) fn edit_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        cx.stop_propagation();
        if self.slash.is_some() {
            self.close_slash(true, window, cx);
        } else {
            self.select(Selection::one(ix), window, cx);
        }
    }

    pub(super) fn toggle_mark(
        &mut self,
        marker: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.active else { return };
        if self.slash.is_some() || !self.edits_inline(ix) {
            return;
        }
        let text = self.text(cx);
        let (new, sel) = model::toggle_mark(&text, self.input.read(cx).selected_range(), marker);
        self.input.update(cx, |input, cx| {
            input.replace_all(new, window, cx);
            input.set_selected_range(sel, cx);
        });
    }

    /// ⌘K: `[selection](|)`.
    pub(super) fn link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        if self.slash.is_some() || !self.edits_inline(ix) {
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

    /// The active text is inline Markdown (a text block, or a table cell).
    fn edits_inline(&self, ix: usize) -> bool {
        let kind = &self.doc.blocks[ix].kind;
        kind.is_text() || *kind == Kind::Table
    }
}
