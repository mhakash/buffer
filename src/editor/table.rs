//! Table blocks (`Kind::Table`): the shared textarea edits one cell at a time (`cell`).
//! Tab / ⇧Tab and ←→ at a cell's edge walk the cells (Tab past the last adds a row), ↑↓ and
//! Enter move between rows (Enter past the last adds one), ⌘Enter leaves the table. Rows and
//! columns are added from the hover bars and inserted / deleted from a cell's right-click menu.

use gpui_kit::component::{Icon, Sizable, Size, Theme, h_flex, input::Textarea, v_flex};
use gpui_kit::{
    AnyElement, ClickEvent, Context, FontWeight, MouseButton, MouseDownEvent, Pixels, Point,
    SharedString, Window, assets::IconName, div, prelude::*, px, rems,
};

use super::{
    BlockEditor, Caret,
    model::{Table, inline_markdown},
    replay_click,
};
use crate::components::{document_style, markdown_style, markdown_view};

impl BlockEditor {
    fn table(&self, ix: usize) -> Table {
        Table::parse(&self.doc.blocks[ix].text)
    }

    /// Typing in the active cell.
    pub(super) fn set_cell_text(&mut self, ix: usize, text: &str) {
        let mut table = self.table(ix);
        table.set_cell(self.cell.0, self.cell.1, text);
        self.doc.blocks[ix].set_text(table.to_markdown());
    }

    /// Change the table in block `id` as one undo step.
    pub(super) fn edit_table(
        &mut self,
        id: u64,
        edit: impl FnOnce(&mut Table),
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let ix = self.index_of(id)?;
        self.checkpoint();
        self.deactivate(cx);
        let mut table = self.table(ix);
        edit(&mut table);
        self.doc.blocks[ix].set_text(table.to_markdown());
        self.changed(cx);
        Some(ix)
    }

    /// The hover bar under the table.
    fn add_table_row(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let mut row = 0;
        if let Some(ix) = self.edit_table(
            id,
            |t| {
                row = t.rows.len();
                t.insert_row(row);
            },
            cx,
        ) {
            self.activate_cell(ix, (row, 0), Caret::Start, window, cx);
        }
    }

    /// The hover bar right of the table.
    fn add_table_col(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let mut col = 0;
        if let Some(ix) = self.edit_table(
            id,
            |t| {
                col = t.cols();
                t.insert_col(col);
            },
            cx,
        ) {
            self.activate_cell(ix, (0, col), Caret::Start, window, cx);
        }
    }

    /// Enter: the cell below, adding a row at the bottom.
    pub(super) fn cell_below(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let (row, col) = self.cell;
        if row + 1 >= self.table(ix).rows.len() {
            self.checkpoint();
            let mut table = self.table(ix);
            table.insert_row(row + 1);
            self.doc.blocks[ix].set_text(table.to_markdown());
            self.changed(cx);
        }
        self.activate_cell(ix, (row + 1, col), Caret::End, window, cx);
    }

    /// ↑/↓ from a cell: the row above/below, else out of the table.
    pub(super) fn cell_vertical(
        &mut self,
        ix: usize,
        step: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (row, col) = self.cell;
        let rows = self.table(ix).rows.len();
        match row.checked_add_signed(step).filter(|r| *r < rows) {
            Some(row) => self.activate_cell(ix, (row, col), Caret::End, window, cx),
            None => self.leave_table(ix, step, window, cx),
        }
    }

    /// The next/previous cell in reading order (Tab, or ←→ at a cell's edge). Past the last
    /// cell, Tab (`grow`) adds a row; arrows leave the table.
    pub(super) fn cell_step(
        &mut self,
        ix: usize,
        step: isize,
        grow: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let table = self.table(ix);
        let (rows, cols) = (table.rows.len(), table.cols());
        let at = self.cell.0 * cols + self.cell.1;
        let caret = if step < 0 { Caret::End } else { Caret::Start };
        match at.checked_add_signed(step) {
            Some(next) if next < rows * cols => {
                self.activate_cell(ix, (next / cols, next % cols), caret, window, cx)
            }
            Some(_) if grow => {
                self.checkpoint();
                let mut table = table;
                table.insert_row(rows);
                self.doc.blocks[ix].set_text(table.to_markdown());
                self.changed(cx);
                self.activate_cell(ix, (rows, 0), Caret::Start, window, cx);
            }
            None if grow => {}
            _ => self.leave_table(ix, step, window, cx),
        }
    }

    /// A click on a cell: edit it with the caret where the click was.
    fn click_cell(
        &mut self,
        id: u64,
        cell: (usize, usize),
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.index_of(id) else { return };
        if self.drag_selecting || (self.active == Some(ix) && self.cell == cell) {
            return;
        }
        let stale = self.input.read(cx).text_bounds();
        self.activate_cell(ix, cell, Caret::End, window, cx);
        replay_click(self.input.clone(), stale, position, 5, window);
    }

    pub(super) fn render_table(&self, ix: usize, theme: &Theme, cx: &Context<Self>) -> AnyElement {
        let block = &self.doc.blocks[ix];
        let id = block.id;
        let table = Table::parse(&block.text);
        let active = (self.active == Some(ix)).then_some(self.cell);
        let (size, line_h) = if self.compact { (13., 20.) } else { (15., 23.) };
        let style = || {
            if self.compact {
                markdown_style()
            } else {
                document_style()
            }
        };
        let group = format!("blk-{id}");
        let muted = theme.muted_foreground;

        let rows = table.rows.iter().enumerate().map(|(r, row)| {
            let cells = row.iter().enumerate().map(|(c, text)| {
                let body = if active == Some((r, c)) {
                    Textarea::new(&self.input)
                        .appearance(false)
                        .with_size(Size::XSmall)
                        .ml(px(-4.))
                        .text_size(px(size))
                        .line_height(px(line_h))
                        .when(r == 0, |t| t.font_weight(FontWeight::SEMIBOLD))
                        .into_any_element()
                } else if text.is_empty() {
                    div().h(px(line_h)).into_any_element()
                } else {
                    markdown_view(
                        ("tbl-md", id << 24 | (r as u64) << 12 | c as u64),
                        inline_markdown(text),
                        style().paragraph_gap(rems(0.)),
                        self.dir.clone(),
                    )
                    .selectable(false)
                    .into_any_element()
                };
                div()
                    .id(SharedString::from(format!("tbl-{id}-{r}-{c}")))
                    .flex_1()
                    .min_w(px(72.))
                    .px_2()
                    .py_1()
                    .when(c > 0, |d| d.border_l_1().border_color(theme.border))
                    .cursor_text()
                    .text_size(px(size))
                    .line_height(px(line_h))
                    .child(body)
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.click_cell(id, (r, c), event.position(), window, cx);
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            if let Some(ix) = this.index_of(id) {
                                this.open_block_menu(ix, event.position, Some((r, c)), window, cx);
                            }
                        }),
                    )
            });
            h_flex()
                .items_stretch()
                .when(r > 0, |d| d.border_t_1().border_color(theme.border))
                .when(r == 0, |d| {
                    d.bg(theme.secondary).font_weight(FontWeight::SEMIBOLD)
                })
                .children(cells)
        });
        let grid = v_flex()
            .flex_1()
            .min_w_0()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .children(rows);

        // Hover bars: a row below, a column to the right.
        let bar = |name: &'static str| {
            div()
                .id((name, id))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .cursor_pointer()
                .opacity(0.)
                .group_hover(group.clone(), |s| s.opacity(1.))
                .hover(|s| s.bg(theme.secondary))
                .child(Icon::new(IconName::Plus).small().text_color(muted))
        };
        let add_col =
            bar("tbl-add-col")
                .w(px(18.))
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.add_table_col(id, window, cx);
                }));
        let add_row = bar("tbl-add-row")
            .h(px(18.))
            .mt_1()
            .mr(px(22.))
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.add_table_row(id, window, cx);
            }));
        v_flex()
            .flex_1()
            .min_w_0()
            .child(h_flex().items_stretch().gap_1().child(grid).child(add_col))
            .child(add_row)
            .into_any_element()
    }
}

/// Edits a cell's right-click menu offers: (label, icon, edit at (row, col)).
pub(super) type CellEdit = fn(&mut Table, usize, usize);

pub(super) fn cell_edits() -> [(&'static str, IconName, CellEdit); 6] {
    [
        (
            "Insert row above",
            IconName::BetweenHorizontalStart,
            |t, r, _| t.insert_row(r),
        ),
        (
            "Insert row below",
            IconName::BetweenHorizontalEnd,
            |t, r, _| t.insert_row(r + 1),
        ),
        (
            "Insert column left",
            IconName::BetweenVerticalStart,
            |t, _, c| t.insert_col(c),
        ),
        (
            "Insert column right",
            IconName::BetweenVerticalEnd,
            |t, _, c| t.insert_col(c + 1),
        ),
        ("Delete row", IconName::Trash, |t, r, _| t.remove_row(r)),
        ("Delete column", IconName::Trash, |t, _, c| t.remove_col(c)),
    ]
}
