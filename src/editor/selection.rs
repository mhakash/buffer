//! Block selection, like Notion: Esc (or ⇧↑/⇧↓ at a block's edge) selects the block being
//! edited; ↑/↓ move and ⇧↑/⇧↓ extend; pressing on one block and dragging onto another selects
//! the span; shift-click extends; ⌘A selects all. While blocks are selected the editor root
//! holds focus (key context `BlockSelection`).

use std::ops::Range;

use gpui_kit::{Context, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Window};

use super::{BlockEditor, BlockEditorEvent, Caret};

/// A contiguous run of blocks; `anchor` stays put while `head` moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Selection {
    anchor: usize,
    head: usize,
}

impl Selection {
    pub fn one(ix: usize) -> Self {
        Self {
            anchor: ix,
            head: ix,
        }
    }

    pub fn span(range: Range<usize>) -> Self {
        Self {
            anchor: range.start,
            head: range.end.saturating_sub(1),
        }
    }

    pub fn range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head) + 1
    }

    pub fn contains(&self, ix: usize) -> bool {
        self.range().contains(&ix)
    }
}

/// Left button went down over block `ix`.
#[derive(Clone, Copy)]
pub(super) struct Press {
    ix: usize,
}

impl BlockEditor {
    pub(super) fn select(&mut self, sel: Selection, window: &mut Window, cx: &mut Context<Self>) {
        let Some(last) = self.doc.blocks.len().checked_sub(1) else {
            return;
        };
        self.deactivate(cx);
        self.selection = Some(Selection {
            anchor: sel.anchor.min(last),
            head: sel.head.min(last),
        });
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(super) fn select_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let len = self.doc.blocks.len();
        self.select(Selection::span(0..len), window, cx);
    }

    pub(super) fn is_selected(&self, ix: usize) -> bool {
        self.selection.is_some_and(|s| s.contains(ix))
    }

    /// ↑/↓ (`extend` = with shift) while blocks are selected; with none, starts at an end.
    pub(super) fn sel_move(
        &mut self,
        step: isize,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(last) = self.doc.blocks.len().checked_sub(1) else {
            return;
        };
        let Some(sel) = self.selection else {
            let ix = if step < 0 { last } else { 0 };
            self.select(Selection::one(ix), window, cx);
            return;
        };
        let head = (sel.head as isize + step).clamp(0, last as isize) as usize;
        let next = if extend {
            Selection {
                anchor: sel.anchor,
                head,
            }
        } else {
            Selection::one(head)
        };
        self.select(next, window, cx);
    }

    /// Enter: edit the block under the head of the selection.
    pub(super) fn sel_enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(sel) = self.selection {
            self.activate(sel.head, Caret::End, window, cx);
        }
    }

    /// Esc: drop the selection and hand focus back to the page.
    pub(super) fn sel_escape(&mut self, cx: &mut Context<Self>) {
        self.selection = None;
        cx.emit(BlockEditorEvent::Exit);
        cx.notify();
    }

    // ---- mouse ----

    /// Block under a window y: rows above the first map to it, below the last to the last.
    pub(super) fn row_at(&self, y: Pixels) -> Option<usize> {
        let rows = self.row_bounds.borrow();
        let last = rows.len().checked_sub(1)?;
        Some(rows.iter().position(|b| y < b.bottom()).unwrap_or(last))
    }

    /// Captured before the blocks see it: shift-click extends, anything else may start a
    /// drag-selection.
    pub(super) fn on_press(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.drag_selecting = false;
        if event.button != MouseButton::Left || self.block_menu.is_some() {
            return;
        }
        let Some(ix) = self.row_at(event.position.y) else {
            return;
        };
        let anchor = self.selection.map(|s| s.anchor).or(self.active);
        if event.modifiers.shift
            && let Some(anchor) = anchor
        {
            cx.stop_propagation();
            self.drag_selecting = true; // the click that follows must not open a block
            self.select(Selection { anchor, head: ix }, window, cx);
            return;
        }
        self.press = Some(Press { ix });
    }

    /// Dragging from one block onto another turns into a block selection.
    pub(super) fn on_pointer_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.pressed_button != Some(MouseButton::Left) || cx.has_active_drag() {
            return;
        }
        let (Some(press), Some(ix)) = (self.press, self.row_at(event.position.y)) else {
            return;
        };
        if ix == press.ix && !self.drag_selecting {
            return; // still inside the first block: ordinary text selection
        }
        self.drag_selecting = true;
        let sel = Selection {
            anchor: press.ix,
            head: ix,
        };
        if self.selection != Some(sel) {
            self.select(sel, window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_cover_both_directions() {
        assert_eq!(Selection { anchor: 3, head: 1 }.range(), 1..4);
        assert_eq!(Selection::span(2..5).range(), 2..5);
        assert!(Selection::one(2).contains(2));
        assert!(!Selection::one(2).contains(3));
    }
}
