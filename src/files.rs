//! Files referenced from a note's content. They're plain Markdown links: an uploaded or
//! in-app text file is copied next to the note (`notes/<id>/…`, `tasks/<id>/files/…`) and
//! linked relatively; a linked file keeps its absolute path. Nothing is kept in an index: the
//! list at the end of a note is read back from its links ([`local_links`]).

use std::{
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::Context as _;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Image,
    Other,
}

/// Guess from the name (cheap, for icons); [`is_text`] looks inside.
pub fn kind(name: &str) -> Kind {
    let ext = ext(name);
    if IMAGE_EXT.contains(&ext.as_str()) {
        Kind::Image
    } else if TEXT_EXT.contains(&ext.as_str()) {
        Kind::Text
    } else {
        Kind::Other
    }
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
    match kind(name) {
        Kind::Text => return true,
        Kind::Image => return false,
        Kind::Other => {}
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

/// A name in `dir` not used yet (case-insensitive on macOS, so `exists` decides).
fn free_name(dir: &Path, name: &str) -> String {
    unique_name(&clean_name(name), |n| dir.join(n).exists())
}

/// Copies `from` into `dir` under a free name; returns that name.
pub fn copy_into(from: &Path, dir: &Path) -> anyhow::Result<String> {
    let name = from.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let name = free_name(dir, name);
    std::fs::copy(from, dir.join(&name)).with_context(|| format!("copying {}", from.display()))?;
    Ok(name)
}

/// Makes an empty file `name` (or a free variant) in `dir`; returns its name.
pub fn create_empty(dir: &Path, name: &str) -> anyhow::Result<String> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let name = free_name(dir, name);
    std::fs::write(dir.join(&name), b"").with_context(|| format!("creating {name}"))?;
    Ok(name)
}

/// Renames a file within its folder to a cleaned, free `new_name`; returns the new path, or
/// `None` when the name didn't change.
pub fn rename(path: &Path, new_name: &str) -> anyhow::Result<Option<PathBuf>> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let old = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let wanted = clean_name(new_name);
    if wanted == old {
        return Ok(None);
    }
    // A case-only rename is the same file on macOS: don't number it.
    let name = if wanted.eq_ignore_ascii_case(old) {
        wanted
    } else {
        free_name(dir, &wanted)
    };
    let to = dir.join(&name);
    std::fs::rename(path, &to).with_context(|| format!("renaming {old} to {name}"))?;
    Ok(Some(to))
}

/// Markdown reference to `target` shown as `name`: images embed, other files link. The target
/// sits in `<…>` so spaces need no escaping.
pub fn markdown_link(name: &str, target: &str) -> String {
    let bang = if kind(name) == Kind::Image { "!" } else { "" };
    let label = name.replace(['[', ']'], "");
    format!("{bang}[{label}](<{target}>)")
}

/// A link in Markdown that points at a file rather than a URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalLink {
    pub label: String,
    /// As written (relative, `~/`, absolute or `file://`), percent-decoded.
    pub target: String,
}

/// Local-file links (`[label](target)`, `![…](…)`, `<target>` form) in order, without
/// duplicates; URLs and in-page anchors are skipped, as is anything inside code.
pub fn local_links(markdown: &str) -> Vec<LocalLink> {
    let mut out: Vec<LocalLink> = Vec::new();
    let mut in_fence = false;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let mut rest = line;
        while let Some(open) = rest.find("](") {
            let Some(start) = rest[..open].rfind('[') else {
                rest = &rest[open + 2..];
                continue;
            };
            let label = rest[start + 1..open].to_string();
            let after = &rest[open + 2..];
            let (target, used) = if let Some(inner) = after.strip_prefix('<') {
                match inner.find(">)") {
                    Some(end) => (&inner[..end], end + 3),
                    None => break,
                }
            } else {
                match after.find(')') {
                    Some(end) => (after[..end].split(' ').next().unwrap_or(""), end + 1),
                    None => break,
                }
            };
            rest = &after[used..];
            let target = percent_decode(target.trim());
            if is_local(&target) && !out.iter().any(|l| l.target == target) {
                out.push(LocalLink { label, target });
            }
        }
    }
    out
}

/// Not a URL (`scheme:`) or anchor; `file://` counts as local.
pub fn is_local(target: &str) -> bool {
    if target.is_empty() || target.starts_with('#') {
        return false;
    }
    if target.starts_with("file://") {
        return true;
    }
    !target.split_once(':').is_some_and(|(scheme, _)| {
        scheme.len() > 1
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    })
}

/// Absolute path of a local link target, relative ones against `dir`.
pub fn resolve(target: &str, dir: &Path) -> PathBuf {
    let path = target.strip_prefix("file://").unwrap_or(target);
    match path.strip_prefix("~/") {
        Some(rest) => dirs::home_dir().unwrap_or_default().join(rest),
        None => dir.join(path), // `join` keeps absolute paths as they are
    }
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
    fn links() {
        assert_eq!(
            markdown_link("my spec.pdf", "7/my spec.pdf"),
            "[my spec.pdf](<7/my spec.pdf>)"
        );
        assert_eq!(markdown_link("a.PNG", "1/a.PNG"), "![a.PNG](<1/a.PNG>)");
        let md = "See [spec](<3/my spec.pdf>) and [site](https://x.io), ![](a%20b.png)\n\
                  ```\n[not](this.txt)\n```\n[again](<3/my spec.pdf>) [abs](/tmp/x.log) [top](#h)";
        let links = local_links(md);
        let targets: Vec<_> = links.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(targets, ["3/my spec.pdf", "a b.png", "/tmp/x.log"]);
        assert_eq!(links[0].label, "spec");
        assert!(is_local("file:///x"));
        assert!(!is_local("mailto:a@b.c"));
        assert_eq!(
            resolve("1/a.txt", Path::new("/n")),
            PathBuf::from("/n/1/a.txt")
        );
        assert_eq!(resolve("/abs", Path::new("/n")), PathBuf::from("/abs"));
    }

    #[test]
    fn files_on_disk() {
        let dir = std::env::temp_dir().join(format!("buffer-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = create_empty(&dir, "note.txt").unwrap();
        let b = create_empty(&dir, "note.txt").unwrap();
        assert_eq!((a.as_str(), b.as_str()), ("note.txt", "note (2).txt"));
        let moved = rename(&dir.join("note.txt"), "data.json").unwrap().unwrap();
        assert_eq!(moved, dir.join("data.json"));
        assert_eq!(rename(&moved, "data.json").unwrap(), None);
        assert_eq!(copy_into(&moved, &dir).unwrap(), "data (2).json");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn kinds_and_text() {
        assert_eq!(kind("a.json"), Kind::Text);
        assert_eq!(kind("Dockerfile"), Kind::Text);
        assert_eq!(kind("a.jpeg"), Kind::Image);
        assert_eq!(kind("a.pdf"), Kind::Other);
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
