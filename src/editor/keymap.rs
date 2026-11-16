//! Editor actions and key bindings. Two key contexts on the editor root:
//! `BlockEditor` while a block's textarea has focus (inline formatting; the kit's `Input`
//! bindings handle the rest) and `BlockSelection` while the editor itself has focus (blocks
//! selected, or just left), where the kit's own actions are rebound to block operations.

use std::rc::Rc;

use gpui_kit::base::actions::{SelectDown, SelectUp};
use gpui_kit::component::input::{
    Backspace, Copy, Cut, Delete, Enter, Escape, IndentInline, MoveDown, MoveUp, OutdentInline,
    Paste, Redo, SelectAll, Undo,
};
use gpui_kit::{Action, App, KeyBinding, KeyBindingContextPredicate, actions};

pub const EDITING: &str = "BlockEditor";
pub const SELECTING: &str = "BlockSelection";

actions!(
    block_editor,
    [
        Bold,
        Italic,
        InlineCode,
        Strikethrough,
        Link,
        DuplicateBlocks,
        MoveBlocksUp,
        MoveBlocksDown,
    ]
);

pub fn init(cx: &mut App) {
    let both = format!("{EDITING} || {SELECTING}");
    let in_input = format!("{EDITING} > Input");
    let enter = Enter {
        secondary: false,
        shift: false,
    };
    cx.bind_keys([
        bind("secondary-b", Bold, EDITING, cx),
        bind("secondary-i", Italic, EDITING, cx),
        bind("secondary-e", InlineCode, EDITING, cx),
        bind("secondary-shift-x", Strikethrough, EDITING, cx),
        bind("secondary-k", Link, EDITING, cx),
        bind("secondary-d", DuplicateBlocks, &both, cx),
        bind("secondary-shift-up", MoveBlocksUp, &both, cx),
        bind("secondary-shift-down", MoveBlocksDown, &both, cx),
        // Inside the textarea these keys are the kit's select-to-start/end; ours win there.
        bind("secondary-shift-up", MoveBlocksUp, &in_input, cx),
        bind("secondary-shift-down", MoveBlocksDown, &in_input, cx),
        bind("up", MoveUp, SELECTING, cx),
        bind("down", MoveDown, SELECTING, cx),
        bind("shift-up", SelectUp, SELECTING, cx),
        bind("shift-down", SelectDown, SELECTING, cx),
        bind("enter", enter, SELECTING, cx),
        bind("backspace", Backspace, SELECTING, cx),
        bind("delete", Delete, SELECTING, cx),
        bind("escape", Escape, SELECTING, cx),
        bind("tab", IndentInline, SELECTING, cx),
        bind("shift-tab", OutdentInline, SELECTING, cx),
        bind("secondary-a", SelectAll, SELECTING, cx),
        bind("secondary-c", Copy, SELECTING, cx),
        bind("secondary-x", Cut, SELECTING, cx),
        bind("secondary-v", Paste, SELECTING, cx),
        bind("secondary-z", Undo, SELECTING, cx),
        bind("secondary-shift-z", Redo, SELECTING, cx),
    ]);
}

/// `secondary` is cmd on macOS and ctrl elsewhere, which only `load` resolves.
fn bind(keys: &str, action: impl Action, context: &str, cx: &App) -> KeyBinding {
    let predicate = KeyBindingContextPredicate::parse(context)
        .unwrap_or_else(|err| panic!("invalid key context {context}: {err:?}"));
    KeyBinding::load(
        keys,
        Box::new(action),
        Some(Rc::new(predicate)),
        false,
        None,
        cx,
    )
    .unwrap_or_else(|err| panic!("invalid key binding {keys}: {err:?}"))
}
