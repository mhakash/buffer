//! Moving blocks by dragging the ⋮⋮ handle. Dragging a selected block carries the whole
//! selection; an accent line shows where they'll land. Clicking the handle opens its menu.

use gpui_kit::component::{ActiveTheme as _, Icon, Sizable, Theme, h_flex};
use gpui_kit::{
    AnyElement, ClickEvent, Context, DragMoveEvent, EntityId, Pixels, Render, SharedString, Window,
    assets::IconName, div, prelude::*, px,
};

use super::{
    BlockEditor,
    model::{Block, Kind},
};

/// What's being dragged: blocks of one editor.
#[derive(Clone)]
pub(super) struct DraggedBlocks {
    editor: EntityId,
    ids: Vec<u64>,
    label: SharedString,
}

/// The chip that follows the pointer.
struct DragPreview {
    label: SharedString,
    count: usize,
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        h_flex()
            .gap_2()
            .max_w(px(360.))
            .px_3()
            .py_1p5()
            .rounded_lg()
            .bg(theme.popover)
            .border_1()
            .border_color(theme.primary.opacity(0.45))
            .text_sm()
            .opacity(0.92)
            .child(
                Icon::new(IconName::GripVertical)
                    .small()
                    .text_color(theme.muted_foreground),
            )
            .child(div().min_w_0().truncate().child(self.label.clone()))
            .when(self.count > 1, |this| {
                this.child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("+{}", self.count - 1)),
                )
            })
    }
}

fn preview_label(block: &Block) -> SharedString {
    match &block.kind {
        Kind::Divider => "Divider".into(),
        Kind::Code(lang) if lang == "mermaid" => "Diagram".into(),
        Kind::Code(lang) if let Some(name) = crate::blocks::model::fence_type(lang) => {
            format!("Block · {name}").into()
        }
        Kind::Code(_) => "Code".into(),
        Kind::Raw => "Markdown".into(),
        _ => {
            let line = block.text.lines().next().unwrap_or("").trim();
            if line.is_empty() {
                "Empty block".into()
            } else {
                line.chars().take(60).collect::<String>().into()
            }
        }
    }
}

impl BlockEditor {
    pub(super) fn render_grip(&self, ix: usize, theme: &Theme, cx: &Context<Self>) -> AnyElement {
        let block = &self.doc.blocks[ix];
        let id = block.id;
        // Dragging a selected block drags the whole selection.
        let ids: Vec<u64> = match self.selection {
            Some(sel) if sel.contains(ix) => sel.range().map(|i| self.doc.blocks[i].id).collect(),
            _ => vec![id],
        };
        let drag = DraggedBlocks {
            editor: cx.entity_id(),
            ids,
            label: preview_label(block),
        };
        h_flex()
            .id(("blk-grip", id))
            .size(px(self.handle_size()))
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .cursor_grab()
            .hover(|s| s.bg(theme.secondary))
            .child(
                Icon::new(IconName::GripVertical)
                    .small()
                    .text_color(theme.muted_foreground),
            )
            .on_drag(drag, |drag: &DraggedBlocks, _, _, cx| {
                cx.new(|_| DragPreview {
                    label: drag.label.clone(),
                    count: drag.ids.len(),
                })
            })
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                // Not also a click on the block underneath (which would start editing it).
                cx.stop_propagation();
                if let Some(ix) = this.index_of(id) {
                    this.open_block_menu(ix, event.position(), window, cx);
                }
            }))
            .into_any_element()
    }

    /// Insertion index for a pointer at window `y`: before the first row whose middle is below.
    fn drop_index(&self, y: Pixels) -> usize {
        let rows = self.row_bounds.borrow();
        rows.iter()
            .position(|b| y < b.center().y)
            .unwrap_or(rows.len())
    }

    pub(super) fn on_drag_move(
        &mut self,
        event: &DragMoveEvent<DraggedBlocks>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = event.dragged_item().downcast_ref::<DraggedBlocks>() else {
            return;
        };
        if drag.editor != cx.entity_id() {
            return;
        }
        if self.dragging.is_none() {
            self.dragging = Some(drag.ids.clone());
            self.deactivate(cx);
        }
        let pos = event.event.position;
        let at = event.bounds.contains(&pos).then(|| self.drop_index(pos.y));
        if at != self.drop_at {
            self.drop_at = at;
            cx.notify();
        }
    }

    pub(super) fn on_drop_blocks(
        &mut self,
        drag: &DraggedBlocks,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let at = self.drop_at.take();
        self.dragging = None;
        cx.notify();
        if drag.editor != cx.entity_id() {
            return;
        }
        if let (Some(at), Some(range)) = (at, self.range_of_ids(&drag.ids)) {
            self.move_range_to(range, at, window, cx);
        }
    }

    pub(super) fn is_dragged(&self, id: u64) -> bool {
        self.dragging.as_ref().is_some_and(|ids| ids.contains(&id))
    }

    /// The accent line marking where dragged blocks will land (inside a `relative` parent).
    pub(super) fn drop_line(&self, left: f32, theme: &Theme) -> AnyElement {
        div()
            .absolute()
            .top(px(-1.))
            .left(px(left))
            .right_0()
            .h(px(2.))
            .rounded_full()
            .bg(theme.primary)
            .into_any_element()
    }
}
