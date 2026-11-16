//! The block menu, opened by clicking a block's ⋮⋮ handle or right-clicking the block. It
//! acts on the selection when the block is part of it, else on that block alone.

use std::ops::Range;

use gpui_kit::component::{
    Icon,
    menu::{PopupMenu, PopupMenuItem},
};
use gpui_kit::{
    AnyElement, App, ClickEvent, ClipboardItem, Context, DismissEvent, Entity, Focusable, Pixels,
    Point, Subscription, WeakEntity, Window, anchored, assets::IconName, deferred, prelude::*, px,
};

use super::{BlockEditor, selection::Selection, slash::block_types};

pub(super) struct BlockMenu {
    menu: Entity<PopupMenu>,
    position: Point<Pixels>,
    _dismiss: Subscription,
}

type RangeOp = fn(&mut BlockEditor, Range<usize>, &mut Window, &mut Context<BlockEditor>);

impl BlockEditor {
    pub(super) fn open_block_menu(
        &mut self,
        ix: usize,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = match self.selection {
            Some(sel) if sel.contains(ix) => sel.range(),
            _ => ix..ix + 1,
        };
        self.select(Selection::span(range.clone()), window, cx);
        let ids: Vec<u64> = self.doc.blocks[range].iter().map(|b| b.id).collect();
        let this = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |menu, window, cx| {
            build(menu, this, ids, window, cx)
        });
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            this.block_menu = None;
            if this.selection.is_some() {
                this.focus_handle.focus(window, cx);
            }
            cx.notify();
        });
        // Set before focusing the menu so the blur doesn't drop the selection.
        self.block_menu = Some(BlockMenu {
            menu: menu.clone(),
            position,
            _dismiss: dismiss,
        });
        menu.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    pub(super) fn render_block_menu(&self) -> Option<AnyElement> {
        let open = self.block_menu.as_ref()?;
        Some(
            deferred(
                anchored()
                    .position(open.position)
                    .snap_to_window_with_margin(px(8.))
                    .child(open.menu.clone()),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}

fn build(
    menu: PopupMenu,
    this: WeakEntity<BlockEditor>,
    ids: Vec<u64>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    // A menu item running `op` on the blocks the menu was opened for.
    let act = |op: RangeOp| {
        let (this, ids) = (this.clone(), ids.clone());
        move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            this.update(cx, |editor, cx| {
                if let Some(range) = editor.range_of_ids(&ids) {
                    op(editor, range, window, cx);
                }
            })
            .ok();
        }
    };
    let item = |label: &'static str, icon: IconName, op: RangeOp| {
        PopupMenuItem::new(label)
            .icon(Icon::new(icon))
            .on_click(act(op))
    };
    let (turn_this, turn_ids) = (this.clone(), ids.clone());
    menu.submenu("Turn into", window, cx, move |mut menu, _, _| {
        for block_type in block_types() {
            let (this, ids, kind) = (turn_this.clone(), turn_ids.clone(), block_type.kind);
            menu = menu.item(
                PopupMenuItem::new(block_type.label)
                    .icon(Icon::new(block_type.icon))
                    .on_click(move |_, window, cx| {
                        this.update(cx, |editor, cx| {
                            if let Some(range) = editor.range_of_ids(&ids) {
                                editor.turn_range_into(range, kind.clone(), window, cx);
                            }
                        })
                        .ok();
                    }),
            );
        }
        menu.min_w(px(200.))
    })
    .separator()
    .item(item("Copy", IconName::Copy, |editor, range, _, cx| {
        let markdown = editor.doc.range_markdown(range);
        cx.write_to_clipboard(ClipboardItem::new_string(markdown));
    }))
    .item(item(
        "Duplicate",
        IconName::CopyPlus,
        BlockEditor::duplicate_range,
    ))
    .item(item(
        "Move up",
        IconName::ArrowUp,
        |editor, range, window, cx| editor.move_by(range, -1, window, cx),
    ))
    .item(item(
        "Move down",
        IconName::ArrowDown,
        |editor, range, window, cx| editor.move_by(range, 1, window, cx),
    ))
    .separator()
    // Undoable with ⌘Z, like deleting text, so no confirmation.
    .item(item("Delete", IconName::Trash, BlockEditor::delete_range))
    .min_w(px(200.))
}
