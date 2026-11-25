//! Routes key actions on the editor root. Captured before the textarea: while a block is
//! being edited they go to `editing` (which may leave them to the textarea); otherwise
//! (blocks selected, or the editor just left) to `selection` / `ops` / `clipboard`.

use gpui_kit::base::TextSelection;
use gpui_kit::base::actions::{SelectDown, SelectUp};
use gpui_kit::component::input::{
    Backspace, Copy, Cut, Delete, Enter, Escape, IndentInline, MoveDown, MoveLeft, MoveRight,
    MoveUp, OutdentInline, Paste, Redo, SelectAll, Undo,
};
use gpui_kit::{Context, Div, Stateful, Window, prelude::*};

use super::{
    BlockEditor,
    keymap::{
        Bold, DuplicateBlocks, InlineCode, Italic, Link, MoveBlocksDown, MoveBlocksUp,
        Strikethrough,
    },
};

impl BlockEditor {
    fn editing(&self) -> bool {
        self.active.is_some()
    }

    pub(super) fn bind_keys(&self, root: Stateful<Div>, cx: &Context<Self>) -> Stateful<Div> {
        root.capture_action(cx.listener(|this, action: &Enter, window, cx| {
            if this.editing() {
                this.edit_enter(action, window, cx);
            } else {
                cx.stop_propagation();
                this.sel_enter(window, cx);
            }
        }))
        .capture_action(
            cx.listener(|this, _: &Backspace, window, cx| this.remove_key(false, window, cx)),
        )
        .capture_action(
            cx.listener(|this, _: &Delete, window, cx| this.remove_key(true, window, cx)),
        )
        .capture_action(
            cx.listener(|this, _: &MoveUp, window, cx| this.vertical(-1, false, window, cx)),
        )
        .capture_action(
            cx.listener(|this, _: &MoveDown, window, cx| this.vertical(1, false, window, cx)),
        )
        .capture_action(
            cx.listener(|this, _: &SelectUp, window, cx| this.vertical(-1, true, window, cx)),
        )
        .capture_action(
            cx.listener(|this, _: &SelectDown, window, cx| this.vertical(1, true, window, cx)),
        )
        .capture_action(
            cx.listener(|this, _: &MoveLeft, window, cx| this.edit_horizontal(-1, window, cx)),
        )
        .capture_action(
            cx.listener(|this, _: &MoveRight, window, cx| this.edit_horizontal(1, window, cx)),
        )
        .capture_action(cx.listener(|this, _: &Escape, window, cx| {
            if this.editing() {
                this.edit_escape(window, cx);
            } else {
                cx.stop_propagation();
                this.sel_escape(window, cx);
            }
        }))
        .capture_action(
            cx.listener(|this, _: &IndentInline, window, cx| this.indent_key(1, window, cx)),
        )
        .capture_action(
            cx.listener(|this, _: &OutdentInline, window, cx| this.indent_key(-1, window, cx)),
        )
        .capture_action(cx.listener(|this, _: &SelectAll, window, cx| {
            if this.editing() {
                this.edit_select_all(window, cx);
            } else {
                cx.stop_propagation();
                this.select_all(window, cx);
            }
        }))
        .capture_action(cx.listener(|this, _: &Copy, window, cx| this.copy_key(false, window, cx)))
        .capture_action(cx.listener(|this, _: &Cut, window, cx| this.copy_key(true, window, cx)))
        .capture_action(cx.listener(|this, _: &Paste, window, cx| {
            if this.editing() {
                this.paste_into_block(window, cx);
            } else {
                cx.stop_propagation();
                this.paste_blocks(window, cx);
            }
        }))
        .capture_action(
            cx.listener(|this, _: &Undo, window, cx| this.history_key(false, window, cx)),
        )
        .capture_action(
            cx.listener(|this, _: &Redo, window, cx| this.history_key(true, window, cx)),
        )
        .on_action(cx.listener(|this, _: &DuplicateBlocks, window, cx| {
            if let Some(range) = this.target() {
                this.duplicate_range(range, window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &MoveBlocksUp, window, cx| {
            if let Some(range) = this.target() {
                this.move_by(range, -1, window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &MoveBlocksDown, window, cx| {
            if let Some(range) = this.target() {
                this.move_by(range, 1, window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &Bold, w, cx| this.toggle_mark("**", w, cx)))
        .on_action(cx.listener(|this, _: &Italic, w, cx| this.toggle_mark("_", w, cx)))
        .on_action(cx.listener(|this, _: &InlineCode, w, cx| this.toggle_mark("`", w, cx)))
        .on_action(cx.listener(|this, _: &Strikethrough, w, cx| this.toggle_mark("~~", w, cx)))
        .on_action(cx.listener(|this, _: &Link, w, cx| this.link(w, cx)))
    }

    /// Backspace / Delete: remove selected blocks, or join blocks at the caret.
    fn remove_key(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing() {
            if forward {
                self.edit_delete(window, cx);
            } else {
                self.edit_backspace(window, cx);
            }
        } else if let Some(sel) = self.selection {
            cx.stop_propagation();
            self.delete_range(sel.range(), window, cx);
        }
    }

    /// ↑/↓ (`extend` = with shift).
    fn vertical(&mut self, step: isize, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editing() {
            cx.stop_propagation();
            self.sel_move(step, extend, window, cx);
        } else if extend {
            self.edit_extend(step, window, cx);
        } else {
            self.edit_vertical(step, window, cx);
        }
    }

    fn indent_key(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing() {
            self.edit_indent(step, window, cx);
        } else if let Some(sel) = self.selection {
            cx.stop_propagation();
            self.indent_range(sel.range(), step, cx);
        }
    }

    /// ⌘C / ⌘X act on selected blocks; while editing the textarea copies its text, and text
    /// selected in rendered blocks is copied by its text view.
    fn copy_key(&mut self, cut: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.selection.is_none() && TextSelection::has_selection(window, cx) {
            return;
        }
        if !self.editing() {
            cx.stop_propagation();
            self.copy_blocks(cut, window, cx);
        }
    }

    /// ⌘Z / ⌘⇧Z: the textarea undoes its own typing first; structural changes and edits to
    /// blocks already left come from the editor's history.
    fn history_key(&mut self, redo: bool, window: &mut Window, cx: &mut Context<Self>) {
        // The textarea (or the `/` search field) undoes its own typing first.
        if self.editing() && (self.typed || self.slash.is_some()) {
            return;
        }
        cx.stop_propagation();
        self.restore(redo, window, cx);
    }
}
