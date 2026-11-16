//! A Markdown body (note or task notes) as Notion-style blocks. `parse` cuts Markdown into blocks and
//! `Doc::to_markdown` writes them back; blocks that were never edited keep their source byte for
//! byte, so opening a note (or editing one block) never reformats the rest of the file.

use std::ops::Range;

/// What a block is. Text kinds (`is_text`) edit inline; `Code` and `Raw` edit their source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Paragraph,
    /// Level 1–6 (the editor offers 1–3).
    Heading(u8),
    Bullet,
    Numbered,
    Todo(bool),
    Quote,
    /// Fenced code; the string is the info string (language), e.g. `mermaid`.
    Code(String),
    Divider,
    /// Anything else (tables, HTML, frontmatter): shown rendered, edited as Markdown.
    Raw,
}

impl Kind {
    pub fn is_list(&self) -> bool {
        matches!(self, Kind::Bullet | Kind::Numbered | Kind::Todo(_))
    }

    /// Edited as inline Markdown with the block's own marker drawn by the editor.
    pub fn is_text(&self) -> bool {
        !matches!(self, Kind::Code(_) | Kind::Divider | Kind::Raw)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub id: u64,
    pub kind: Kind,
    /// Nesting level of list items.
    pub indent: usize,
    /// Inline Markdown for text kinds; the code for `Code`; the source for `Raw`.
    pub text: String,
    /// Original source while untouched; written back verbatim.
    src: Option<String>,
    /// Blank lines before the block in the source; `None` for new blocks.
    gap: Option<usize>,
}

impl Block {
    pub fn new(id: u64, kind: Kind, text: impl Into<String>) -> Self {
        Self {
            id,
            kind,
            indent: 0,
            text: text.into(),
            src: None,
            gap: None,
        }
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        let text = text.into();
        if text != self.text {
            self.text = text;
            self.src = None;
        }
    }

    pub fn set_kind(&mut self, kind: Kind) {
        if kind != self.kind {
            if !kind.is_list() {
                self.indent = 0;
            }
            if kind == Kind::Divider {
                self.text.clear();
            }
            self.kind = kind;
            self.src = None;
        }
    }

    /// Write the block from its fields on save (e.g. renumber after a move).
    pub fn touch(&mut self) {
        self.src = None;
    }

    pub fn set_indent(&mut self, indent: usize) {
        if indent != self.indent {
            self.indent = indent;
            self.src = None;
        }
    }

    /// Markdown for this block; `number` is its position in a numbered list.
    fn markdown(&self, number: usize) -> String {
        if let Some(src) = &self.src {
            return src.clone();
        }
        let pad = "  ".repeat(self.indent);
        let item = |marker: &str| {
            let cont = format!("\n{pad}{}", " ".repeat(marker.len()));
            format!("{pad}{marker}{}", self.text.replace('\n', &cont))
        };
        match &self.kind {
            Kind::Paragraph | Kind::Raw => self.text.clone(),
            Kind::Heading(level) => format!(
                "{} {}",
                "#".repeat(*level as usize),
                self.text.replace('\n', " ")
            ),
            Kind::Bullet => item("- "),
            Kind::Numbered => item(&format!("{number}. ")),
            Kind::Todo(done) => item(if *done { "- [x] " } else { "- [ ] " }),
            Kind::Quote => self
                .text
                .lines()
                .map(|l| {
                    if l.is_empty() {
                        ">".into()
                    } else {
                        format!("> {l}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n"),
            Kind::Code(lang) => format!("```{lang}\n{}\n```", self.text),
            Kind::Divider => "---".into(),
        }
    }

    /// The block alone as Markdown, for rendering it (code fences, raw blocks).
    pub fn source(&self) -> String {
        self.markdown(1)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Doc {
    pub blocks: Vec<Block>,
    /// Text after the last block (trailing newlines), kept as it was.
    tail: String,
    next_id: u64,
}

impl Doc {
    pub fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// A new block, ready to insert.
    pub fn block(&mut self, kind: Kind, text: impl Into<String>) -> Block {
        Block::new(self.next_id(), kind, text)
    }

    /// 1-based position of each numbered item within its list (0 for other blocks).
    pub fn numbers(&self) -> Vec<usize> {
        let mut counters: Vec<usize> = Vec::new();
        self.blocks
            .iter()
            .map(|b| {
                if !b.kind.is_list() {
                    counters.clear();
                    return 0;
                }
                counters.truncate(b.indent + 1);
                counters.resize(b.indent + 1, 0);
                if b.kind == Kind::Numbered {
                    counters[b.indent] += 1;
                    counters[b.indent]
                } else {
                    counters[b.indent] = 0;
                    0
                }
            })
            .collect()
    }

    pub fn to_markdown(&self) -> String {
        let numbers = self.numbers();
        let mut out = String::new();
        let mut prev: Option<&Block> = None;
        for (block, number) in self.blocks.iter().zip(numbers) {
            if let Some(prev) = prev {
                // List items may sit on adjacent lines; anything else needs a blank line or
                // Markdown would merge it into the block above.
                let min = usize::from(!(prev.kind.is_list() && block.kind.is_list()));
                let gap = block.gap.map_or(min, |g| g.max(min));
                out.push('\n');
                out.push_str(&"\n".repeat(gap));
            } else if let Some(gap) = block.gap {
                out.push_str(&"\n".repeat(gap));
            }
            out.push_str(&block.markdown(number));
            prev = Some(block);
        }
        if !self.blocks.is_empty() {
            out.push_str(&self.tail);
        }
        out
    }
}

/// Range operations used by selection, clipboard and drag.
impl Doc {
    /// Markdown of `range` on its own (copying blocks); untouched blocks keep their source.
    pub fn range_markdown(&self, range: Range<usize>) -> String {
        let mut part = Doc {
            blocks: self.blocks[range].to_vec(),
            ..Doc::default()
        };
        if let Some(first) = part.blocks.first_mut() {
            first.gap = None;
        }
        part.to_markdown()
    }

    /// `blocks` (parsed elsewhere, or copies) with fresh ids, ready to insert here.
    pub fn adopt(&mut self, blocks: impl IntoIterator<Item = Block>) -> Vec<Block> {
        blocks
            .into_iter()
            .enumerate()
            .map(|(i, mut block)| {
                block.id = self.next_id();
                if i == 0 {
                    block.gap = None;
                }
                block
            })
            .collect()
    }

    /// Move `range` to insertion index `to` (counted before the move); returns where it landed.
    /// Dropping a range inside itself is a no-op.
    pub fn move_range(&mut self, range: Range<usize>, to: usize) -> Range<usize> {
        if to >= range.start && to <= range.end {
            return range;
        }
        let len = range.len();
        let moved: Vec<Block> = self.blocks.drain(range.clone()).collect();
        let at = if to > range.end { to - len } else { to };
        self.blocks.splice(at..at, moved);
        self.tidy();
        at..at + len
    }

    /// After blocks were moved, inserted or removed: numbered items are rewritten so their
    /// numbers follow their positions, and whatever is now first starts the body (no blank
    /// lines carried over from where it came from).
    pub fn tidy(&mut self) {
        for block in &mut self.blocks {
            if block.kind == Kind::Numbered {
                block.touch();
            }
        }
        if let Some(first) = self.blocks.first_mut() {
            first.gap = None;
        }
    }
}

/// `(kind, indent, marker length)` of a list item line.
fn list_marker(line: &str) -> Option<(Kind, usize, usize)> {
    let ws = line.len() - line.trim_start_matches([' ', '\t']).len();
    let indent = line[..ws]
        .chars()
        .map(|c| if c == '\t' { 2 } else { 1 })
        .sum::<usize>()
        / 2;
    let rest = &line[ws..];
    if let Some(after) = rest
        .strip_prefix("- ")
        .or_else(|| rest.strip_prefix("* "))
        .or_else(|| rest.strip_prefix("+ "))
    {
        for (box_, done) in [("[ ] ", false), ("[x] ", true), ("[X] ", true)] {
            if after.starts_with(box_) || after == box_.trim_end() {
                let len = (ws + 2 + box_.len()).min(line.len());
                return Some((Kind::Todo(done), indent, len));
            }
        }
        return Some((Kind::Bullet, indent, ws + 2));
    }
    let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if (1..=9).contains(&digits) {
        let after = &rest[digits..];
        if after.starts_with(". ") || after.starts_with(") ") {
            return Some((Kind::Numbered, indent, ws + digits + 2));
        }
    }
    None
}

fn heading(line: &str) -> Option<(u8, &str)> {
    let hashes = line.len() - line.trim_start_matches('#').len();
    let rest = &line[hashes..];
    ((1..=6).contains(&hashes) && (rest.is_empty() || rest.starts_with(' ')))
        .then(|| (hashes as u8, rest.trim()))
}

fn is_divider(line: &str) -> bool {
    let t: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    t.len() >= 3
        && t.chars()
            .next()
            .is_some_and(|c| matches!(c, '-' | '*' | '_') && t.chars().all(|d| d == c))
}

fn fence(line: &str) -> Option<&str> {
    let t = line.trim_start();
    (t.starts_with("```") || t.starts_with("~~~")).then(|| &t[..3])
}

/// A line that ends a paragraph because it starts another block.
fn starts_block(line: &str) -> bool {
    let t = line.trim_start();
    heading(t).is_some()
        || fence(line).is_some()
        || list_marker(line).is_some()
        || t.starts_with('>')
        || t.starts_with('|')
        || is_divider(line)
}

pub fn parse(text: &str) -> Doc {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut doc = Doc::default();
    let mut i = 0;
    let mut blank = 0;
    while i < lines.len() {
        let line = lines[i].trim_end_matches('\r');
        if line.trim().is_empty() {
            blank += 1;
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        let (kind, indent, body) = if let Some(open) = fence(line) {
            let lang = line.trim_start()[3..].trim().to_string();
            while i < lines.len() && !lines[i].trim_start().starts_with(open) {
                i += 1;
            }
            let body = lines[start + 1..i].join("\n");
            i = (i + 1).min(lines.len());
            (Kind::Code(lang), 0, body)
        } else if doc.blocks.is_empty()
            && line == "---"
            && let Some(end) = lines[i..].iter().position(|l| l.trim_end() == "---")
        {
            i += end + 1;
            (Kind::Raw, 0, lines[start..i].join("\n"))
        } else if is_divider(line) {
            (Kind::Divider, 0, String::new())
        } else if let Some((level, title)) = heading(line.trim_start()) {
            (Kind::Heading(level), 0, title.to_string())
        } else if let Some((kind, indent, len)) = list_marker(line) {
            let mut body = line[len..].to_string();
            // Lazy and indented continuation lines belong to the item.
            while i < lines.len() && !lines[i].trim().is_empty() && !starts_block(lines[i]) {
                body.push('\n');
                body.push_str(lines[i].trim());
                i += 1;
            }
            (kind, indent, body)
        } else if line.trim_start().starts_with('>') {
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                i += 1;
            }
            let body = lines[start..i]
                .iter()
                .map(|l| {
                    let l = l.trim_start().trim_start_matches('>');
                    l.strip_prefix(' ').unwrap_or(l)
                })
                .collect::<Vec<_>>()
                .join("\n");
            (Kind::Quote, 0, body)
        } else if line.trim_start().starts_with('|') || line.trim_start().starts_with('<') {
            while i < lines.len() && !lines[i].trim().is_empty() {
                i += 1;
            }
            (Kind::Raw, 0, lines[start..i].join("\n"))
        } else {
            while i < lines.len() && !lines[i].trim().is_empty() && !starts_block(lines[i]) {
                i += 1;
            }
            (Kind::Paragraph, 0, lines[start..i].join("\n"))
        };
        let mut block = doc.block(kind, body);
        block.indent = indent;
        block.src = Some(lines[start..i].join("\n"));
        block.gap = Some(blank);
        doc.blocks.push(block);
        blank = 0;
    }
    if !doc.blocks.is_empty() {
        doc.tail = "\n".repeat(blank);
    }
    doc
}

/// Markdown typed at the start of a paragraph (Notion shortcuts): the kind it turns into and
/// how many bytes of marker to drop. List items only convert to to-dos (`[] `).
pub fn shortcut(kind: &Kind, text: &str) -> Option<(Kind, usize)> {
    if *kind == Kind::Bullet {
        return ["[] ", "[ ] "]
            .iter()
            .find(|p| text.starts_with(**p))
            .map(|p| (Kind::Todo(false), p.len()))
            .or_else(|| text.starts_with("[x] ").then_some((Kind::Todo(true), 4)));
    }
    if *kind != Kind::Paragraph {
        return None;
    }
    if text == "---" {
        return Some((Kind::Divider, 3));
    }
    if text == "```" {
        return Some((Kind::Code(String::new()), 3));
    }
    for (prefix, kind) in [
        ("# ", Kind::Heading(1)),
        ("## ", Kind::Heading(2)),
        ("### ", Kind::Heading(3)),
        ("- [ ] ", Kind::Todo(false)),
        ("[] ", Kind::Todo(false)),
        ("[ ] ", Kind::Todo(false)),
        ("- ", Kind::Bullet),
        ("* ", Kind::Bullet),
        ("1. ", Kind::Numbered),
        ("> ", Kind::Quote),
    ] {
        if text.starts_with(prefix) {
            return Some((kind, prefix.len()));
        }
    }
    None
}

/// Inline text shown through the Markdown renderer as a plain paragraph: a leading marker
/// (`#`, `-`, `1.`, `>`…) is escaped so it stays text.
pub fn inline_markdown(text: &str) -> String {
    let lead = text.trim_start();
    let ws = &text[..text.len() - lead.len()];
    let digits = lead.len() - lead.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits > 0 && lead[digits..].starts_with(['.', ')']) {
        // Backslash only escapes punctuation: escape the `.` of `1.`.
        format!("{ws}{}\\{}", &lead[..digits], &lead[digits..])
    } else if starts_block(lead) || lead.starts_with(['+', '#']) {
        format!("{ws}\\{lead}")
    } else {
        text.to_string()
    }
}

/// Toggle an inline `marker` (`**`, `_`, `` ` ``, `~~`) around `sel` in `text`. Returns the
/// new text and selection.
pub fn toggle_mark(
    text: &str,
    sel: std::ops::Range<usize>,
    marker: &str,
) -> (String, std::ops::Range<usize>) {
    let m = marker.len();
    let (start, end) = (sel.start.min(text.len()), sel.end.min(text.len()));
    let inner = &text[start..end];
    // Selection includes the markers.
    if inner.len() >= 2 * m && inner.starts_with(marker) && inner.ends_with(marker) {
        let new = format!(
            "{}{}{}",
            &text[..start],
            &inner[m..inner.len() - m],
            &text[end..]
        );
        return (new, start..end - 2 * m);
    }
    // Markers sit just outside the selection.
    if start >= m && text[..start].ends_with(marker) && text[end..].starts_with(marker) {
        let new = format!("{}{inner}{}", &text[..start - m], &text[end + m..]);
        return (new, start - m..end - m);
    }
    let new = format!("{}{marker}{inner}{marker}{}", &text[..start], &text[end..]);
    (new, start + m..end + m)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "Intro line\nstill intro\n\n## Section\n\n- one\n- two\n  - nested\n* star\n\n1. first\n2) second\n\n- [ ] todo\n- [x] done\n\n> quote\n> more\n\n```rust\nfn main() {}\n\n```\n\n---\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n![img](a.png)\n";

    #[test]
    fn untouched_documents_round_trip() {
        for text in [
            SAMPLE,
            "",
            "one",
            "\n\npara\n\n\n\nafter\n\n",
            "---\ntitle: x\n---\n\nbody",
            "```\nunclosed",
        ] {
            assert_eq!(parse(text).to_markdown(), text, "{text:?}");
        }
    }

    #[test]
    fn parses_kinds() {
        let doc = parse(SAMPLE);
        let kinds: Vec<_> = doc
            .blocks
            .iter()
            .map(|b| (b.kind.clone(), b.indent))
            .collect();
        assert_eq!(
            kinds,
            [
                (Kind::Paragraph, 0),
                (Kind::Heading(2), 0),
                (Kind::Bullet, 0),
                (Kind::Bullet, 0),
                (Kind::Bullet, 1),
                (Kind::Bullet, 0),
                (Kind::Numbered, 0),
                (Kind::Numbered, 0),
                (Kind::Todo(false), 0),
                (Kind::Todo(true), 0),
                (Kind::Quote, 0),
                (Kind::Code("rust".into()), 0),
                (Kind::Divider, 0),
                (Kind::Raw, 0),
                (Kind::Paragraph, 0),
            ]
        );
        assert_eq!(doc.blocks[0].text, "Intro line\nstill intro");
        assert_eq!(doc.blocks[1].text, "Section");
        assert_eq!(doc.blocks[10].text, "quote\nmore");
        assert_eq!(doc.blocks[11].text, "fn main() {}\n");
        assert_eq!(doc.blocks[8].text, "todo");
    }

    #[test]
    fn edited_blocks_are_rewritten() {
        let mut doc = parse("- a\n- b\n\ntext");
        doc.blocks[1].set_kind(Kind::Todo(true));
        doc.blocks[2].set_kind(Kind::Heading(1));
        assert_eq!(doc.to_markdown(), "- a\n- [x] b\n\n# text");

        let mut doc = parse("- a\n- b\n- c");
        doc.blocks[1].set_kind(Kind::Paragraph);
        assert_eq!(doc.to_markdown(), "- a\n\nb\n\n- c");
    }

    #[test]
    fn new_blocks_get_markdown_spacing_and_numbers() {
        let mut doc = Doc::default();
        for (kind, text) in [
            (Kind::Heading(1), "Title"),
            (Kind::Numbered, "a"),
            (Kind::Numbered, "b"),
            (Kind::Quote, "q\n\nr"),
            (Kind::Code("js".into()), "x"),
        ] {
            let block = doc.block(kind, text);
            doc.blocks.push(block);
        }
        assert_eq!(
            doc.to_markdown(),
            "# Title\n\n1. a\n2. b\n\n> q\n>\n> r\n\n```js\nx\n```"
        );
    }

    #[test]
    fn multi_line_items_indent_continuations() {
        let mut doc = Doc::default();
        let mut block = doc.block(Kind::Bullet, "a\nb");
        block.indent = 1;
        doc.blocks.push(block);
        assert_eq!(doc.to_markdown(), "  - a\n    b");
        assert_eq!(parse("  - a\n    b").blocks[0].text, "a\nb");
    }

    #[test]
    fn shortcuts() {
        assert_eq!(
            shortcut(&Kind::Paragraph, "## x"),
            Some((Kind::Heading(2), 3))
        );
        assert_eq!(shortcut(&Kind::Paragraph, "- "), Some((Kind::Bullet, 2)));
        assert_eq!(
            shortcut(&Kind::Paragraph, "[] "),
            Some((Kind::Todo(false), 3))
        );
        assert_eq!(shortcut(&Kind::Bullet, "[] "), Some((Kind::Todo(false), 3)));
        assert_eq!(shortcut(&Kind::Paragraph, "---"), Some((Kind::Divider, 3)));
        assert_eq!(shortcut(&Kind::Paragraph, "#hashtag"), None);
        assert_eq!(shortcut(&Kind::Heading(1), "- "), None);
    }

    #[test]
    fn inline_escapes_leading_markers() {
        assert_eq!(inline_markdown("# not a heading"), "\\# not a heading");
        assert_eq!(inline_markdown("1. no"), "1\\. no");
        assert_eq!(inline_markdown("**bold**"), "**bold**");
    }

    #[test]
    fn copies_ranges_verbatim() {
        let doc = parse("intro\n\n* star\n* two\n\n## H");
        assert_eq!(doc.range_markdown(1..3), "* star\n* two");
        assert_eq!(doc.range_markdown(2..4), "* two\n\n## H");
    }

    #[test]
    fn adopted_blocks_get_fresh_ids() {
        let mut doc = parse("a\n\nb");
        let pasted = parse("x\n\ny").blocks;
        let ids: Vec<u64> = doc.adopt(pasted).iter().map(|b| b.id).collect();
        assert_eq!(ids, [3, 4]);
    }

    #[test]
    fn moves_ranges() {
        let text = |d: &Doc| d.blocks.iter().map(|b| b.text.clone()).collect::<Vec<_>>();
        let mut doc = parse("a\n\nb\n\nc\n\nd");
        assert_eq!(doc.move_range(0..2, 4), 2..4);
        assert_eq!(text(&doc), ["c", "d", "a", "b"]);
        assert_eq!(doc.move_range(2..4, 0), 0..2);
        assert_eq!(text(&doc), ["a", "b", "c", "d"]);
        assert_eq!(doc.move_range(1..3, 2), 1..3, "inside itself");
        let mut doc = parse("1. a\n2. b");
        doc.move_range(1..2, 0);
        assert_eq!(doc.to_markdown(), "1. b\n2. a");
        let mut doc = parse("a\n\nb\n");
        doc.move_range(1..2, 0);
        assert_eq!(
            doc.to_markdown(),
            "b\n\na\n",
            "no blank line before the new first block"
        );
    }

    #[test]
    fn toggles_marks() {
        assert_eq!(toggle_mark("a b c", 2..3, "**"), ("a **b** c".into(), 4..5));
        assert_eq!(toggle_mark("a **b** c", 4..5, "**"), ("a b c".into(), 2..3));
        assert_eq!(toggle_mark("a **b** c", 2..7, "**"), ("a b c".into(), 2..3));
        assert_eq!(toggle_mark("ab", 1..1, "_"), ("a__b".into(), 2..2));
    }
}
