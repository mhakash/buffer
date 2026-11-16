//! Structural changes on a range of blocks: delete, duplicate, move, turn into, indent,
//! insert. Each records an undo step, saves, and leaves the affected blocks selected
//! (or keeps editing the active block where it ends up).

use std::ops::Range;

use gpui_kit::{Context, Window};

use super::{
    BlockEditor,
    model::{Block, Kind},
    selection::Selection,
};

impl BlockEditor {
    /// What a block operation applies to: the selection, else the block being edited.
    pub(super) fn target(&self) -> Option<Range<usize>> {
        self.selection
            .map(|s| s.range())
            .or_else(|| self.active.map(|ix| ix..ix + 1))
    }

    /// Smallest range covering `ids` (ids are contiguous for selections and drags).
    pub(super) fn range_of_ids(&self, ids: &[u64]) -> Option<Range<usize>> {
        let ixs: Vec<usize> = ids.iter().filter_map(|id| self.index_of(*id)).collect();
        Some(*ixs.iter().min()?..*ixs.iter().max()? + 1)
    }

    /// After an operation: select `range`, or follow the edited block if one is active.
    fn settle(&mut self, range: Range<usize>, window: &mut Window, cx: &mut Context<Self>) {
        if self.active.is_some() {
            self.active = Some(range.start);
        } else if !range.is_empty() {
            self.select(Selection::span(range), window, cx);
        }
        self.changed(cx);
    }

    pub(super) fn delete_range(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.checkpoint();
        self.deactivate(cx);
        self.doc.blocks.drain(range);
        self.doc.tidy();
        self.selection = None;
        // Keep focus so ⌘Z can bring them back.
        self.focus_handle.focus(window, cx);
        self.changed(cx);
    }

    pub(super) fn duplicate_range(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.checkpoint();
        let copies = self.doc.adopt(self.doc.blocks[range.clone()].to_vec());
        let at = range.end;
        let len = copies.len();
        self.doc.blocks.splice(at..at, copies);
        self.doc.tidy();
        if self.active.is_some() {
            self.changed(cx); // keep editing the original
        } else {
            self.settle(at..at + len, window, cx);
        }
    }

    /// Move `range` to insertion index `to` (counted before the move).
    pub(super) fn move_range_to(
        &mut self,
        range: Range<usize>,
        to: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.checkpoint();
        let landed = self.doc.move_range(range, to);
        self.settle(landed, window, cx);
    }

    /// One block up (`-1`) or down (`1`), past the neighbouring block.
    pub(super) fn move_by(
        &mut self,
        range: Range<usize>,
        step: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let to = if step < 0 {
            match range.start.checked_sub(1) {
                Some(to) => to,
                None => return,
            }
        } else if range.end < self.doc.blocks.len() {
            range.end + 1
        } else {
            return;
        };
        self.move_range_to(range, to, window, cx);
    }

    pub(super) fn turn_range_into(
        &mut self,
        range: Range<usize>,
        kind: Kind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.checkpoint();
        self.deactivate(cx);
        for block in &mut self.doc.blocks[range.clone()] {
            block.set_kind(kind.clone());
        }
        self.doc.tidy();
        self.settle(range, window, cx);
    }

    /// Nest (`1`) or un-nest (`-1`) the list items in `range`; an item nests at most one level
    /// under the item above it.
    pub(super) fn indent_range(
        &mut self,
        range: Range<usize>,
        step: isize,
        cx: &mut Context<Self>,
    ) {
        self.checkpoint();
        for ix in range {
            if !self.doc.blocks[ix].kind.is_list() {
                continue;
            }
            let max = match ix.checked_sub(1).map(|i| &self.doc.blocks[i]) {
                Some(prev) if prev.kind.is_list() => prev.indent + 1,
                _ => 0,
            };
            let indent = (self.doc.blocks[ix].indent as isize + step).clamp(0, max as isize);
            self.doc.blocks[ix].set_indent(indent as usize);
        }
        self.changed(cx);
    }

    /// Insert `blocks` (already adopted) at `at`; returns where they went.
    pub(super) fn insert_blocks(&mut self, at: usize, blocks: Vec<Block>) -> Range<usize> {
        let len = blocks.len();
        self.doc.blocks.splice(at..at, blocks);
        self.doc.tidy();
        at..at + len
    }
}
