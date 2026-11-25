//! The document's headings, for a page's outline (table of contents).

use gpui_kit::{Pixels, SharedString, px};

use super::{BlockEditor, model::Kind, rich::inline::Inline};

pub struct Heading {
    pub level: u8,
    /// Shown text (inline Markdown markers hidden).
    pub text: SharedString,
    /// Top of the heading's row below the editor's top (last frame).
    pub top: Pixels,
}

impl BlockEditor {
    /// Non-empty headings in document order.
    pub fn headings(&self) -> Vec<Heading> {
        let rows = self.row_bounds.borrow();
        let root = self.root_top.get();
        self.doc
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(ix, b)| {
                let Kind::Heading(level) = b.kind else {
                    return None;
                };
                let text = Inline::parse(&b.text).display;
                let text = text.trim();
                (!text.is_empty()).then(|| Heading {
                    level,
                    text: text.to_string().into(),
                    top: rows.get(ix).map_or(px(0.), |r| r.top() - root),
                })
            })
            .collect()
    }
}
