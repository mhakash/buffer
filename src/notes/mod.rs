//! Standalone Markdown notes (not tied to a task). Index in `notes.json`, each body in
//! `notes/<id>.md`. All mutations go through `NoteStore`, which saves and notifies.

use chrono::{DateTime, Utc};
use gpui_kit::{Context, SharedString};
use serde::{Deserialize, Serialize};

use crate::{storage, tasks::set_checkbox};

pub type NoteId = u64;

const FILE: &str = "notes.json";
const DATA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Note {
    pub id: NoteId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Lives in `notes/<id>.md`, never in the index.
    #[serde(skip)]
    pub body: String,
}

impl Note {
    fn file(id: NoteId) -> String {
        format!("notes/{id}.md")
    }

    /// First non-empty line, Markdown markers stripped.
    pub fn title(&self) -> String {
        self.lines()
            .next()
            .unwrap_or_else(|| "New note".to_string())
    }

    /// The line after the title, for the list preview.
    pub fn snippet(&self) -> String {
        self.lines().nth(1).unwrap_or_default()
    }

    fn lines(&self) -> impl Iterator<Item = String> + '_ {
        self.body.lines().map(plain).filter(|l| !l.is_empty())
    }

    pub fn matches(&self, query: &str) -> bool {
        query.is_empty() || self.body.to_lowercase().contains(&query.to_lowercase())
    }
}

/// A Markdown line without its heading/list/quote/checkbox prefix.
fn plain(line: &str) -> String {
    let mut s = line.trim();
    loop {
        let t = s.trim_start_matches(['#', '>', '-', '*', '+']).trim_start();
        let t = ["[ ]", "[x]", "[X]"]
            .iter()
            .find_map(|m| t.strip_prefix(m))
            .unwrap_or(t)
            .trim_start();
        if t == s {
            break;
        }
        s = t;
    }
    s.replace(['*', '`'], "")
}

#[derive(Default, Serialize, Deserialize)]
struct NoteData {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    next_id: NoteId,
    #[serde(default)]
    notes: Vec<Note>,
}

pub struct NoteStore {
    data: NoteData,
    pub save_error: Option<SharedString>,
}

impl NoteStore {
    pub fn load(_cx: &mut Context<Self>) -> Self {
        let mut data: NoteData = storage::load(FILE);
        data.version = DATA_VERSION;
        for note in &mut data.notes {
            note.body = storage::load_text(&Note::file(note.id));
        }
        Self {
            data,
            save_error: None,
        }
    }

    /// Most recently edited first.
    pub fn notes(&self) -> Vec<Note> {
        let mut notes = self.data.notes.clone();
        notes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        notes
    }

    pub fn note(&self, id: NoteId) -> Option<&Note> {
        self.data.notes.iter().find(|n| n.id == id)
    }

    pub fn create(&mut self, cx: &mut Context<Self>) -> NoteId {
        self.data.next_id = self.data.next_id.max(1);
        let id = self.data.next_id;
        self.data.next_id += 1;
        let now = Utc::now();
        self.data.notes.push(Note {
            id,
            created_at: now,
            updated_at: now,
            body: String::new(),
        });
        self.commit(cx);
        id
    }

    pub fn set_body(&mut self, id: NoteId, body: &str, cx: &mut Context<Self>) {
        if let Some(note) = self.note_mut(id)
            && note.body != body
        {
            note.body = body.to_string();
            self.commit_body(id, cx);
        }
    }

    /// Ticks or unticks the `[ ]` / `[x]` marker at `marker` (byte range).
    pub fn set_checkbox(
        &mut self,
        id: NoteId,
        marker: std::ops::Range<usize>,
        checked: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(note) = self.note_mut(id)
            && let Some(body) = set_checkbox(&note.body, marker, checked)
        {
            note.body = body;
            self.commit_body(id, cx);
        }
    }

    pub fn delete(&mut self, id: NoteId, cx: &mut Context<Self>) {
        self.data.notes.retain(|n| n.id != id);
        if let Err(err) = storage::save_text(&Note::file(id), "") {
            self.save_error = Some(format!("Couldn't delete note: {err:#}").into());
        }
        self.commit(cx);
    }

    fn note_mut(&mut self, id: NoteId) -> Option<&mut Note> {
        self.data.notes.iter_mut().find(|n| n.id == id)
    }

    /// Saves the body and the index (its `updated_at` changed; the index is tiny).
    fn commit_body(&mut self, id: NoteId, cx: &mut Context<Self>) {
        let Some(note) = self.note_mut(id) else {
            return;
        };
        note.updated_at = Utc::now();
        let body = note.body.clone();
        self.save_error = storage::save_text(&Note::file(id), &body)
            .err()
            .map(|err| format!("Couldn't save note: {err:#}").into());
        self.commit(cx);
    }

    fn commit(&mut self, cx: &mut Context<Self>) {
        if let Err(err) = storage::save(FILE, &self.data) {
            self.save_error = Some(format!("Couldn't save notes: {err:#}").into());
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(body: &str) -> Note {
        Note {
            id: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            body: body.into(),
        }
    }

    #[test]
    fn title_and_snippet_skip_markdown() {
        let n = note("\n# Standup **notes**\n\n- [ ] ping `SRE`\n");
        assert_eq!(n.title(), "Standup notes");
        assert_eq!(n.snippet(), "ping SRE");
        assert_eq!(note("").title(), "New note");
        assert_eq!(note("> - quoted").title(), "quoted");
    }

    #[test]
    fn search_is_case_insensitive() {
        assert!(note("Novartis intake").matches("novartis"));
        assert!(!note("Novartis intake").matches("pfizer"));
    }
}
