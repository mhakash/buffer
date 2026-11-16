//! Undo/redo of whole-document snapshots. Text typed inside a block is undone by its textarea;
//! this covers everything structural (split, join, move, paste, delete, convert) and edits to
//! blocks you have since left.

use super::model::Doc;

const LIMIT: usize = 200;

#[derive(Default)]
pub struct History {
    undo: Vec<Doc>,
    redo: Vec<Doc>,
}

impl History {
    /// Remember `doc` as a state to come back to (call before changing it).
    pub fn record(&mut self, doc: &Doc) {
        if self.undo.last() == Some(doc) {
            return;
        }
        self.undo.push(doc.clone());
        if self.undo.len() > LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// The state before `current`, if any.
    pub fn undo(&mut self, current: &Doc) -> Option<Doc> {
        let prev = Self::step(&mut self.undo, current)?;
        self.redo.push(current.clone());
        Some(prev)
    }

    pub fn redo(&mut self, current: &Doc) -> Option<Doc> {
        let next = Self::step(&mut self.redo, current)?;
        self.undo.push(current.clone());
        Some(next)
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Pop the newest state that differs from `current`.
    fn step(stack: &mut Vec<Doc>, current: &Doc) -> Option<Doc> {
        while let Some(doc) = stack.pop() {
            if doc != *current {
                return Some(doc);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::model::parse;

    #[test]
    fn undoes_and_redoes() {
        let (a, b, c) = (parse("a"), parse("b"), parse("c"));
        let mut h = History::default();
        h.record(&a);
        h.record(&a); // duplicate snapshots collapse
        h.record(&b);
        assert_eq!(h.undo(&c), Some(b.clone()));
        assert_eq!(h.undo(&b), Some(a.clone()));
        assert_eq!(h.undo(&a), None);
        assert_eq!(h.redo(&a), Some(b.clone()));
        assert_eq!(h.redo(&b), Some(c.clone()));
        h.record(&a);
        assert_eq!(h.redo(&a), None, "a new edit drops the redo branch");
    }

    #[test]
    fn skips_states_equal_to_current() {
        let (a, b) = (parse("a"), parse("b"));
        let mut h = History::default();
        h.record(&a);
        h.record(&b);
        // Nothing changed since the last record: undo goes past it.
        assert_eq!(h.undo(&b), Some(a));
    }
}
