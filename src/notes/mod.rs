//! Standalone Markdown notes (not tied to a task). Index in `notes.json`, each body in
//! `notes/<id>.md`, attached files in `notes/<id>/` (see [`files`]). All mutations go through
//! `NoteStore`, which saves and notifies.

pub mod files;

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use gpui_kit::{Context, SharedString};
use serde::{Deserialize, Serialize};

use crate::{storage, tasks};
pub use files::Attachment;

pub type NoteId = u64;

const FILE: &str = "notes.json";
const DATA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Note {
    pub id: NoteId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// The whole Markdown file (`notes/<id>.md`), never in the index. Its first non-empty
    /// line is the title; see [`split`].
    #[serde(skip)]
    pub text: String,
    /// Shared task categories, shown as tags.
    #[serde(default)]
    pub tags: Vec<tasks::Id>,
    /// Free-form properties (Notion-style), in display order.
    #[serde(default)]
    pub props: Vec<Property>,
    /// Attached files (uploaded, linked or made in the app), in display order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<Attachment>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Property {
    pub name: String,
    #[serde(default)]
    pub value: String,
}

impl Note {
    fn file(id: NoteId) -> String {
        format!("notes/{id}.md")
    }

    pub fn split(&self) -> Split<'_> {
        split(&self.text)
    }

    /// Title for lists and dialogs.
    pub fn title(&self) -> String {
        let title = plain(self.split().title);
        if title.is_empty() {
            "Untitled".into()
        } else {
            title
        }
    }

    /// First prose line of the content (code blocks skipped), for the list preview.
    pub fn snippet(&self) -> String {
        let mut in_code = false;
        self.split()
            .content
            .lines()
            .filter(|l| {
                let fence = l.trim_start().starts_with("```");
                in_code ^= fence;
                !fence && !in_code && !l.trim_start().starts_with("![")
            })
            .map(plain)
            .find(|l| !l.is_empty())
            .unwrap_or_default()
    }

    /// Text, a property value or a file name contains `query` (case-insensitive).
    pub fn matches(&self, query: &str) -> bool {
        let query = query.to_lowercase();
        query.is_empty()
            || std::iter::once(&self.text)
                .chain(self.props.iter().map(|p| &p.value))
                .chain(self.files.iter().map(|f| &f.name))
                .any(|s| s.to_lowercase().contains(&query))
    }

    /// Nothing worth keeping: no text, tags, properties or files.
    pub fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
            && self.tags.is_empty()
            && self.props.is_empty()
            && self.files.is_empty()
    }

    /// Stored file named `name` (not a linked one).
    pub fn stored(&self, name: &str) -> Option<usize> {
        self.files
            .iter()
            .position(|f| !f.linked() && f.name == name)
    }
}

/// A note's text cut into its title line and the Markdown after it.
#[derive(Debug, PartialEq, Eq)]
pub struct Split<'a> {
    /// Markdown marker in front of the title (`# `, `- `, …), kept when the title is edited.
    pub prefix: &'a str,
    pub title: &'a str,
    /// Everything after the title line, leading blank lines dropped.
    pub content: &'a str,
}

pub fn split(text: &str) -> Split<'_> {
    let start = text
        .lines()
        .take_while(|l| l.trim().is_empty())
        .map(|l| l.len() + 1)
        .sum::<usize>()
        .min(text.len());
    let rest = &text[start..];
    let line_end = rest.find('\n').unwrap_or(rest.len());
    let line = &rest[..line_end];
    let title = strip_marker(line);
    Split {
        prefix: &line[..line.len() - title.len()],
        title: title.trim_end(),
        content: rest[line_end..].trim_start_matches(['\n', '\r']),
    }
}

/// Inverse of [`split`]. An empty title keeps a `# ` line so the content doesn't become the title.
pub fn compose(prefix: &str, title: &str, content: &str) -> String {
    if title.trim().is_empty() && content.trim().is_empty() {
        return String::new();
    }
    let prefix = if prefix.is_empty() && title.trim().is_empty() {
        "# "
    } else {
        prefix
    };
    let mut out = format!("{prefix}{title}");
    if !content.is_empty() {
        out.push_str("\n\n");
        out.push_str(content);
    }
    out
}

fn strip_marker(line: &str) -> &str {
    let mut s = line.trim_start();
    loop {
        let t = s.trim_start_matches(['#', '>', '-', '*', '+']).trim_start();
        let t = ["[ ]", "[x]", "[X]"]
            .iter()
            .find_map(|m| t.strip_prefix(m))
            .unwrap_or(t)
            .trim_start();
        if t == s {
            return s;
        }
        s = t;
    }
}

/// A Markdown line as plain text: no block marker, no emphasis or code ticks.
fn plain(line: &str) -> String {
    strip_marker(line).trim().replace(['*', '`'], "")
}

/// `name`, made unique among the note's stored files and anything already in its folder.
fn unique_stored(note: &Note, name: &str) -> String {
    files::unique_name(name, |candidate| {
        note.stored(candidate).is_some()
            || storage::path(&format!("{}/{candidate}", files::dir(note.id))).exists()
    })
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
            note.text = storage::load_text(&Note::file(note.id));
        }
        Self {
            data,
            save_error: None,
        }
    }

    /// Folder holding the note files; relative image paths in notes resolve against it.
    pub fn dir() -> std::path::PathBuf {
        storage::path("notes")
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
            text: String::new(),
            tags: Vec::new(),
            props: Vec::new(),
            files: Vec::new(),
        });
        self.commit(cx);
        id
    }

    pub fn set_title(&mut self, id: NoteId, title: &str, cx: &mut Context<Self>) {
        let title = title.replace('\n', " ");
        self.edit(
            id,
            |text| {
                let parts = split(text);
                // New notes start with a heading.
                let prefix = if text.is_empty() { "# " } else { parts.prefix };
                compose(prefix, &title, parts.content)
            },
            cx,
        );
    }

    /// Replaces everything after the title line.
    pub fn set_content(&mut self, id: NoteId, content: &str, cx: &mut Context<Self>) {
        self.edit(
            id,
            |text| {
                let parts = split(text);
                compose(parts.prefix, parts.title, content)
            },
            cx,
        );
    }

    pub fn toggle_tag(&mut self, id: NoteId, tag: tasks::Id, cx: &mut Context<Self>) {
        self.edit_meta(id, cx, |n| {
            if let Some(ix) = n.tags.iter().position(|t| *t == tag) {
                n.tags.remove(ix);
            } else {
                n.tags.push(tag);
            }
        });
    }

    pub fn add_prop(&mut self, id: NoteId, name: &str, cx: &mut Context<Self>) {
        let name = name.trim().to_string();
        if !name.is_empty() {
            self.edit_meta(id, cx, |n| {
                n.props.push(Property {
                    name,
                    value: String::new(),
                })
            });
        }
    }

    pub fn set_prop_value(&mut self, id: NoteId, ix: usize, value: &str, cx: &mut Context<Self>) {
        self.edit_meta(id, cx, |n| {
            if let Some(p) = n.props.get_mut(ix) {
                p.value = value.to_string();
            }
        });
    }

    pub fn remove_prop(&mut self, id: NoteId, ix: usize, cx: &mut Context<Self>) {
        self.edit_meta(id, cx, |n| {
            if ix < n.props.len() {
                n.props.remove(ix);
            }
        });
    }

    /// Copies files into the note's folder, or (`link`) references them where they are.
    pub fn attach(&mut self, id: NoteId, paths: &[PathBuf], link: bool, cx: &mut Context<Self>) {
        let mut errors = Vec::new();
        self.edit_meta(id, cx, |n| {
            for path in paths {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(files::clean_name)
                    .unwrap_or_else(|| "file".into());
                let path_str = path.to_string_lossy().to_string();
                if link {
                    if !n.files.iter().any(|f| f.path.as_deref() == Some(&path_str)) {
                        n.files.push(Attachment {
                            name,
                            path: Some(path_str),
                            added_at: Utc::now(),
                        });
                    }
                    continue;
                }
                let name = unique_stored(n, &name);
                let file = Attachment {
                    name,
                    path: None,
                    added_at: Utc::now(),
                };
                match storage::copy_in(path, &file.rel(id).unwrap_or_default()) {
                    Ok(()) => n.files.push(file),
                    Err(err) => errors.push(format!("{err:#}")),
                }
            }
        });
        if !errors.is_empty() {
            self.save_error = Some(format!("Couldn't attach: {}", errors.join("; ")).into());
            cx.notify();
        }
    }

    /// Makes an empty text file in the note's folder; returns its name.
    pub fn new_text_file(&mut self, id: NoteId, cx: &mut Context<Self>) -> Option<String> {
        let mut created = None;
        let mut error = None;
        self.edit_meta(id, cx, |n| {
            let file = Attachment {
                name: unique_stored(n, "untitled.txt"),
                path: None,
                added_at: Utc::now(),
            };
            match storage::save_bytes(&file.rel(id).unwrap_or_default(), b"") {
                Ok(()) => {
                    created = Some(file.name.clone());
                    n.files.push(file);
                }
                Err(err) => error = Some(format!("Couldn't create file: {err:#}")),
            }
        });
        if let Some(err) = error {
            self.save_error = Some(err.into());
            cx.notify();
        }
        created
    }

    /// Saves a stored file's content (the in-app text editor).
    pub fn write_file(&mut self, id: NoteId, name: &str, text: &str, cx: &mut Context<Self>) {
        let Some(rel) = self
            .note(id)
            .and_then(|n| n.files.get(n.stored(name)?))
            .and_then(|f| f.rel(id))
        else {
            return;
        };
        self.save_error = storage::save_bytes(&rel, text.as_bytes())
            .err()
            .map(|err| format!("Couldn't save {name}: {err:#}").into());
        if let Some(note) = self.note_mut(id) {
            note.updated_at = Utc::now();
        }
        self.commit(cx);
    }

    /// Renames a stored file on disk and fixes links to it in the note. Returns the name it got
    /// (cleaned and made unique), or `None` if nothing changed.
    pub fn rename_file(
        &mut self,
        id: NoteId,
        name: &str,
        new_name: &str,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let note = self.note(id)?;
        let ix = note.stored(name)?;
        let wanted = files::clean_name(new_name);
        if wanted == name {
            return None;
        }
        // A case-only rename is the same file on macOS: don't number it.
        let new_name = if wanted.eq_ignore_ascii_case(name) {
            wanted
        } else {
            unique_stored(note, &wanted)
        };
        let old = note.files[ix].clone();
        let new = Attachment {
            name: new_name.clone(),
            ..old.clone()
        };
        if let Err(err) = storage::rename(&old.rel(id)?, &new.rel(id)?) {
            self.save_error = Some(format!("Couldn't rename: {err:#}").into());
            cx.notify();
            return None;
        }
        let (old_target, new_target) = (old.target(id), new.target(id));
        self.edit_meta(id, cx, |n| n.files[ix] = new);
        self.edit(
            id,
            |text| {
                text.replace(&format!("](<{old_target}>)"), &format!("](<{new_target}>)"))
                    .replace(&format!("]({old_target})"), &format!("]({new_target})"))
            },
            cx,
        );
        Some(new_name)
    }

    /// Detaches a file; a stored one is deleted from disk.
    pub fn remove_file(&mut self, id: NoteId, ix: usize, cx: &mut Context<Self>) {
        let Some(file) = self.note(id).and_then(|n| n.files.get(ix)).cloned() else {
            return;
        };
        if let Some(rel) = file.rel(id)
            && let Err(err) = storage::remove_file(&rel)
        {
            self.save_error = Some(format!("Couldn't delete {}: {err:#}", file.name).into());
        }
        self.edit_meta(id, cx, |n| {
            n.files.remove(ix);
        });
    }

    /// Index-only change (tags, properties, file list): saves `notes.json`.
    fn edit_meta(&mut self, id: NoteId, cx: &mut Context<Self>, f: impl FnOnce(&mut Note)) {
        if let Some(note) = self.note_mut(id) {
            let before = (note.tags.clone(), note.props.clone(), note.files.clone());
            f(note);
            if (&note.tags, &note.props, &note.files) != (&before.0, &before.1, &before.2) {
                note.updated_at = Utc::now();
                self.commit(cx);
            }
        }
    }

    fn edit(&mut self, id: NoteId, f: impl FnOnce(&str) -> String, cx: &mut Context<Self>) {
        if let Some(note) = self.note_mut(id) {
            let text = f(&note.text);
            if note.text != text {
                note.text = text;
                self.commit_text(id, cx);
            }
        }
    }

    pub fn delete(&mut self, id: NoteId, cx: &mut Context<Self>) {
        self.data.notes.retain(|n| n.id != id);
        if let Err(err) = storage::save_text(&Note::file(id), "")
            .and_then(|()| storage::remove_dir(&files::dir(id)))
        {
            self.save_error = Some(format!("Couldn't delete note: {err:#}").into());
        }
        self.commit(cx);
    }

    fn note_mut(&mut self, id: NoteId) -> Option<&mut Note> {
        self.data.notes.iter_mut().find(|n| n.id == id)
    }

    /// Saves the file and the index (its `updated_at` changed; the index is tiny).
    fn commit_text(&mut self, id: NoteId, cx: &mut Context<Self>) {
        let Some(note) = self.note_mut(id) else {
            return;
        };
        note.updated_at = Utc::now();
        let text = note.text.clone();
        self.save_error = storage::save_text(&Note::file(id), &text)
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

    fn note(text: &str) -> Note {
        Note {
            id: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            text: text.into(),
            tags: Vec::new(),
            props: Vec::new(),
            files: Vec::new(),
        }
    }

    #[test]
    fn title_and_snippet_skip_markdown() {
        let n = note("\n# Standup **notes**\n\n- [ ] ping `SRE`\n");
        assert_eq!(n.title(), "Standup notes");
        assert_eq!(n.snippet(), "ping SRE");
        assert_eq!(note("").title(), "Untitled");
        assert_eq!(note("> - quoted").title(), "quoted");
        assert_eq!(
            note("# T\n```mermaid\nflowchart LR\n```\nafter").snippet(),
            "after"
        );
    }

    #[test]
    fn split_and_compose_round_trip() {
        let text = "# Plan\n\n- one\n- two\n";
        let parts = split(text);
        assert_eq!(
            parts,
            Split {
                prefix: "# ",
                title: "Plan",
                content: "- one\n- two\n"
            }
        );
        assert!(text.ends_with(parts.content));
        assert_eq!(compose(parts.prefix, parts.title, parts.content), text);
        // A plain first line stays plain.
        assert_eq!(compose("", "Ideas", "x"), "Ideas\n\nx");
        assert_eq!(compose("", "", "x"), "# \n\nx");
        assert_eq!(split("# \n\nx").content, "x");
        assert_eq!(compose("# ", "", ""), "");
    }

    #[test]
    fn search_is_case_insensitive() {
        assert!(note("Novartis intake").matches("novartis"));
        assert!(!note("Novartis intake").matches("pfizer"));
        let mut n = note("x");
        n.props.push(Property {
            name: "Owner".into(),
            value: "Girish".into(),
        });
        assert!(n.matches("girish"));
    }
}
