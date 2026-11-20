//! Files attached to a note. A stored file (uploaded, or a text file made in the app) lives at
//! `notes/<note id>/<name>`; a linked file stays where it is and only its path is kept.

use std::{
    io::Read,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::NoteId;
use crate::storage;

/// Largest file the in-app text editor opens; bigger ones open in their default app.
const TEXT_MAX: u64 = 4 * 1024 * 1024;

/// Extensions treated as text without looking inside.
const TEXT_EXT: &[&str] = &[
    "txt",
    "md",
    "markdown",
    "json",
    "jsonl",
    "ndjson",
    "yaml",
    "yml",
    "toml",
    "ini",
    "cfg",
    "conf",
    "env",
    "csv",
    "tsv",
    "log",
    "xml",
    "html",
    "htm",
    "css",
    "scss",
    "js",
    "mjs",
    "cjs",
    "jsx",
    "ts",
    "tsx",
    "py",
    "rb",
    "rs",
    "go",
    "java",
    "kt",
    "kts",
    "swift",
    "c",
    "h",
    "cc",
    "cpp",
    "hpp",
    "cs",
    "php",
    "sh",
    "bash",
    "zsh",
    "fish",
    "sql",
    "graphql",
    "gql",
    "proto",
    "lua",
    "r",
    "scala",
    "dart",
    "vue",
    "svelte",
    "tf",
    "hcl",
    "gradle",
    "properties",
    "diff",
    "patch",
    "dockerfile",
    "makefile",
];

const IMAGE_EXT: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "tiff"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// File name; for a stored file also its name on disk.
    pub name: String,
    /// Linked file: its absolute path (not copied into the workspace).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub added_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Image,
    Other,
}

impl Attachment {
    pub fn linked(&self) -> bool {
        self.path.is_some()
    }

    /// Stored file's path relative to the data dir.
    pub fn rel(&self, note: NoteId) -> Option<String> {
        (!self.linked()).then(|| format!("{}/{}", dir(note), self.name))
    }

    pub fn abs_path(&self, note: NoteId) -> PathBuf {
        match &self.path {
            Some(path) => PathBuf::from(path),
            None => storage::path(&format!("{}/{}", dir(note), self.name)),
        }
    }

    /// Link target as written in the note: relative to `notes/` for a stored file.
    pub fn target(&self, note: NoteId) -> String {
        match &self.path {
            Some(path) => path.clone(),
            None => format!("{note}/{}", self.name),
        }
    }

    /// Markdown reference for pasting into the note: images embed, other files link.
    pub fn markdown(&self, note: NoteId) -> String {
        let bang = if self.kind() == Kind::Image { "!" } else { "" };
        format!("{bang}[{}](<{}>)", self.name, self.target(note))
    }

    /// Guess from the name (cheap, for icons); [`is_text`] looks inside.
    pub fn kind(&self) -> Kind {
        let ext = ext(&self.name);
        if IMAGE_EXT.contains(&ext.as_str()) {
            Kind::Image
        } else if TEXT_EXT.contains(&ext.as_str()) || ext.is_empty() {
            Kind::Text
        } else {
            Kind::Other
        }
    }
}

/// Folder of a note's stored files, relative to the data dir.
pub fn dir(note: NoteId) -> String {
    format!("notes/{note}")
}

fn ext(name: &str) -> String {
    let lower = name.to_lowercase();
    if matches!(lower.as_str(), "dockerfile" | "makefile") {
        return lower;
    }
    Path::new(&lower)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_string()
}

/// Small enough and looks like UTF-8 text (known extension, or no NUL and valid UTF-8 in the
/// first 8 KB).
pub fn is_text(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() || meta.len() > TEXT_MAX {
        return false;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let ext = ext(name);
    if TEXT_EXT.contains(&ext.as_str()) {
        return true;
    }
    if IMAGE_EXT.contains(&ext.as_str()) {
        return false;
    }
    let mut head = Vec::with_capacity(8192);
    if std::fs::File::open(path)
        .and_then(|f| f.take(8192).read_to_end(&mut head))
        .is_err()
    {
        return false;
    }
    looks_like_text(&head)
}

fn looks_like_text(head: &[u8]) -> bool {
    !head.contains(&0)
        && match std::str::from_utf8(head) {
            Ok(_) => true,
            // Cut mid-character at the 8 KB boundary is still text.
            Err(err) => err.error_len().is_none(),
        }
}

/// A file name safe to store: no folders, not hidden-only, not empty.
pub fn clean_name(name: &str) -> String {
    let name: String = name
        .trim()
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':') {
                '-'
            } else {
                c
            }
        })
        .collect();
    let name = name.trim_start_matches('.').trim().to_string();
    if name.is_empty() {
        "untitled.txt".into()
    } else {
        name
    }
}

/// `name`, or `name (2).ext`, `name (3).ext`, … — the first not `taken`.
pub fn unique_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(name) {
        return name.to_string();
    }
    let (stem, ext) = match name.rfind('.') {
        Some(ix) if ix > 0 => (&name[..ix], &name[ix..]),
        _ => (name, ""),
    };
    (2..)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|candidate| !taken(candidate))
        .expect("unbounded")
}

/// "12 KB" style size, for the file list.
pub fn human_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024. && unit < UNITS.len() - 1 {
        size /= 1024.;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if size < 10. {
        format!("{size:.1} {}", UNITS[unit])
    } else {
        format!("{size:.0} {}", UNITS[unit])
    }
}

/// Decodes `%XX` escapes in a Markdown link target (`my%20file.pdf`).
pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit()
        {
            let hex = |b: u8| (b as char).to_digit(16).unwrap_or(0) as u8;
            out.push(hex(bytes[i + 1]) * 16 + hex(bytes[i + 2]));
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> Attachment {
        Attachment {
            name: name.into(),
            path: None,
            added_at: Utc::now(),
        }
    }

    #[test]
    fn names() {
        assert_eq!(clean_name(" a/b.txt "), "a-b.txt");
        assert_eq!(clean_name("..."), "untitled.txt");
        let taken = ["spec.pdf", "spec (2).pdf", "README"];
        let is_taken = |n: &str| taken.contains(&n);
        assert_eq!(unique_name("spec.pdf", is_taken), "spec (3).pdf");
        assert_eq!(unique_name("README", is_taken), "README (2)");
        assert_eq!(unique_name("new.json", is_taken), "new.json");
    }

    #[test]
    fn references() {
        let f = file("my spec.pdf");
        assert_eq!(f.markdown(7), "[my spec.pdf](<7/my spec.pdf>)");
        assert_eq!(f.rel(7).as_deref(), Some("notes/7/my spec.pdf"));
        assert_eq!(file("a.PNG").markdown(1), "![a.PNG](<1/a.PNG>)");
        let linked = Attachment {
            path: Some("/Users/me/x.docx".into()),
            ..file("x.docx")
        };
        assert_eq!(linked.target(1), "/Users/me/x.docx");
        assert_eq!(linked.rel(1), None);
    }

    #[test]
    fn kinds_and_text() {
        assert_eq!(file("a.json").kind(), Kind::Text);
        assert_eq!(file("Dockerfile").kind(), Kind::Text);
        assert_eq!(file("a.jpeg").kind(), Kind::Image);
        assert_eq!(file("a.pdf").kind(), Kind::Other);
        assert!(looks_like_text("héllo".as_bytes()));
        assert!(looks_like_text(&"é".as_bytes()[..1]));
        assert!(!looks_like_text(&[0x25, 0x50, 0x00, 0x01]));
    }

    #[test]
    fn helpers() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(50 * 1024 * 1024), "50 MB");
        assert_eq!(percent_decode("my%20file.pdf"), "my file.pdf");
        assert_eq!(percent_decode("100%"), "100%");
    }
}
