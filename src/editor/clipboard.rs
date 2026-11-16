//! Copy, cut and paste of blocks. The clipboard carries plain Markdown, so blocks paste into
//! other apps and Markdown from anywhere pastes as blocks.

use gpui_kit::{ClipboardItem, Context, Window};

use super::{
    BlockEditor, Caret,
    model::{self, Block, Kind},
    selection::Selection,
};

impl BlockEditor {
    /// ⌘C / ⌘X on selected blocks.
    pub(super) fn copy_blocks(&mut self, cut: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(range) = self.selection.map(|s| s.range()) else {
            return;
        };
        let markdown = self.doc.range_markdown(range.clone());
        cx.write_to_clipboard(ClipboardItem::new_string(markdown));
        if cut {
            self.delete_range(range, window, cx);
        }
    }

    /// The clipboard as blocks ready to insert (fresh ids).
    fn clipboard_blocks(&mut self, cx: &Context<Self>) -> Vec<Block> {
        let text = cx
            .read_from_clipboard()
            .and_then(|c| c.text())
            .unwrap_or_default();
        let blocks = model::parse(&text).blocks;
        self.doc.adopt(blocks)
    }

    /// ⌘V with blocks selected: paste after them (at the end when nothing is selected) and
    /// select what was pasted.
    pub(super) fn paste_blocks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let at = self
            .selection
            .map_or(self.doc.blocks.len(), |s| s.range().end);
        let blocks = self.clipboard_blocks(cx);
        if blocks.is_empty() {
            return;
        }
        self.checkpoint();
        let range = self.insert_blocks(at, blocks);
        self.select(Selection::span(range), window, cx);
        self.changed(cx);
    }

    /// ⌘V while editing: Markdown with several blocks splits the block at the caret and goes
    /// in between; a single block is left to the textarea (pasted as text).
    pub(super) fn paste_into_block(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        if !self.doc.blocks[ix].kind.is_text() {
            return;
        }
        let text = cx
            .read_from_clipboard()
            .and_then(|c| c.text())
            .unwrap_or_default();
        let parsed = model::parse(&text).blocks;
        if parsed.len() < 2 {
            return;
        }
        cx.stop_propagation();
        self.checkpoint();
        let blocks = self.doc.adopt(parsed);
        let current = self.text(cx);
        let sel = self.input.read(cx).selected_range();
        let (before, after) = (
            current[..sel.start].to_string(),
            current[sel.end..].to_string(),
        );
        // An empty paragraph is replaced; otherwise its text before the caret stays above.
        let at = if before.trim().is_empty() && self.doc.blocks[ix].kind == Kind::Paragraph {
            self.doc.blocks.remove(ix);
            ix
        } else {
            self.doc.blocks[ix].set_text(before);
            ix + 1
        };
        let range = self.insert_blocks(at, blocks);
        let mut last = range.end - 1;
        if !after.is_empty() {
            let rest = self.doc.block(Kind::Paragraph, after);
            self.doc.blocks.insert(range.end, rest);
            last = range.end;
        }
        self.activate(last, Caret::End, window, cx);
        self.changed(cx);
    }
}
