//! Inline Markdown as it's shown while editing: markers (`**`, `_`, `` ` ``, `~~`, `\`) and
//! link targets are hidden, the text between them is styled. Two coordinate spaces: `source`
//! (bytes of the block's Markdown, what's stored) and `display` (bytes of the drawn text).
//! Unmatched markers stay visible as typed, so `**bold` only turns bold once it's closed.

use std::ops::Range;

/// Formatting of a run of text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Marks {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    pub link: bool,
}

impl Marks {
    fn add(&mut self, other: Marks) {
        self.bold |= other.bold;
        self.italic |= other.italic;
        self.code |= other.code;
        self.strike |= other.strike;
        self.link |= other.link;
    }
}

/// A matched pair of markers (source ranges); for links `open` is `[` and `close` `](url)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub open: Range<usize>,
    pub close: Range<usize>,
    pub marks: Marks,
}

impl Span {
    fn content(&self) -> Range<usize> {
        self.open.end..self.close.start
    }
}

/// A run of visible source bytes and where it lands in the display text.
#[derive(Clone, Copy, Debug)]
struct Seg {
    src: usize,
    disp: usize,
    len: usize,
}

/// Which side of hidden markers a display offset maps to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bias {
    /// The end of the text before: typing there continues its formatting.
    Left,
    /// The start of the text after.
    Right,
}

#[derive(Clone, Debug)]
pub struct Inline {
    pub display: String,
    /// Formatted display ranges, in order (unformatted text has no entry).
    pub styles: Vec<(Range<usize>, Marks)>,
    segs: Vec<Seg>,
    /// A link was left as typed because the caret is in its target.
    pub revealed: bool,
}

impl Inline {
    /// `source` shown as is (code blocks).
    pub fn plain(source: &str) -> Self {
        Self {
            display: source.to_string(),
            styles: Vec::new(),
            segs: vec![Seg {
                src: 0,
                disp: 0,
                len: source.len(),
            }],
            revealed: false,
        }
    }

    pub fn parse(source: &str) -> Self {
        Self::parse_revealing(source, None)
    }

    /// Like `parse`, but a link whose target holds source offset `caret` stays as typed, so
    /// its URL can be edited.
    pub fn parse_revealing(source: &str, caret: Option<usize>) -> Self {
        let (mut spans, escapes) = scan(source);
        let before = spans.len();
        spans.retain(|s| {
            !(s.marks.link && caret.is_some_and(|c| c > s.close.start && c < s.close.end))
        });
        let revealed = spans.len() != before;
        let mut hidden: Vec<Range<usize>> = escapes;
        for s in &spans {
            hidden.push(s.open.clone());
            hidden.push(s.close.clone());
        }
        hidden.sort_by_key(|r| r.start);

        let mut marks = vec![Marks::default(); source.len()];
        for s in &spans {
            for m in &mut marks[s.content()] {
                m.add(s.marks);
            }
        }

        let mut display = String::with_capacity(source.len());
        let mut segs = Vec::new();
        let mut styles: Vec<(Range<usize>, Marks)> = Vec::new();
        let mut at = 0;
        let mut push_visible = |range: Range<usize>, display: &mut String| {
            if range.is_empty() {
                return;
            }
            let disp = display.len();
            segs.push(Seg {
                src: range.start,
                disp,
                len: range.len(),
            });
            display.push_str(&source[range.clone()]);
            for (i, m) in marks[range].iter().enumerate() {
                if *m == Marks::default() {
                    continue;
                }
                let d = disp + i;
                match styles.last_mut() {
                    Some((r, last)) if r.end == d && last == m => r.end = d + 1,
                    _ => styles.push((d..d + 1, *m)),
                }
            }
        };
        for h in &hidden {
            if h.start >= at {
                push_visible(at..h.start, &mut display);
            }
            at = at.max(h.end);
        }
        push_visible(at..source.len(), &mut display);
        Self {
            display,
            styles,
            segs,
            revealed,
        }
    }

    /// Display offset of source offset `s`; offsets inside hidden markers snap to their spot.
    pub fn to_display(&self, s: usize) -> usize {
        let mut d = 0;
        for seg in &self.segs {
            if s < seg.src {
                break;
            }
            d = seg.disp + (s - seg.src).min(seg.len);
        }
        d
    }

    /// Source offset of display offset `d`.
    pub fn to_source(&self, d: usize, bias: Bias) -> usize {
        let Some(first) = self.segs.first() else {
            return 0;
        };
        if d == 0 {
            return first.src;
        }
        match bias {
            Bias::Left => self
                .segs
                .iter()
                .find(|s| d > s.disp && d <= s.disp + s.len)
                .map_or_else(|| self.end(), |s| s.src + d - s.disp),
            Bias::Right => self
                .segs
                .iter()
                .find(|s| d >= s.disp && d < s.disp + s.len)
                .map_or_else(|| self.end(), |s| s.src + d - s.disp),
        }
    }

    /// Source range of display range `r`: just the visible text, markers around it left out.
    pub fn to_source_range(&self, r: Range<usize>) -> Range<usize> {
        if r.is_empty() {
            let s = self.to_source(r.start, Bias::Left);
            return s..s;
        }
        self.to_source(r.start, Bias::Right)..self.to_source(r.end, Bias::Left)
    }

    fn end(&self) -> usize {
        self.segs.last().map_or(0, |s| s.src + s.len)
    }
}

/// Finds matched marker pairs and backslash escapes (the `\` bytes to hide).
fn scan(src: &str) -> (Vec<Span>, Vec<Range<usize>>) {
    let b = src.as_bytes();
    let mut spans = Vec::new();
    let mut escapes = Vec::new();
    // Open emphasis markers: (byte, length, position).
    let mut open: Vec<(u8, usize, usize)> = Vec::new();
    // Links being read: (`[` position, `]` position, end of `)`).
    let mut links: Vec<(usize, usize, usize)> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' if b.get(i + 1).is_some_and(u8::is_ascii_punctuation) => {
                escapes.push(i..i + 1);
                i += 2;
            }
            b'`' => {
                let run = run_len(b, i);
                match find_run(b, i + run, b'`', run) {
                    Some(j) if j > i + run => {
                        spans.push(Span {
                            open: i..i + run,
                            close: j..j + run,
                            marks: Marks {
                                code: true,
                                ..Marks::default()
                            },
                        });
                        i = j + run;
                    }
                    _ => i += run,
                }
            }
            c @ (b'*' | b'_' | b'~') => {
                let run = run_len(b, i);
                let prev = src[..i].chars().next_back().unwrap_or(' ');
                let next = src[i + run..].chars().next().unwrap_or(' ');
                let can_open = !next.is_whitespace() && (c != b'_' || !prev.is_alphanumeric());
                let can_close = !prev.is_whitespace() && (c != b'_' || !next.is_alphanumeric());
                let tokens: &[usize] = match (c, run) {
                    (b'~', 2) => &[2],
                    (b'~', _) => &[],
                    (_, 1) => &[1],
                    (_, 2) => &[2],
                    // `***x***`: bold outside, italic inside.
                    (_, 3) if can_close && has_open(&open, c, 1) => &[1, 2],
                    (_, 3) => &[2, 1],
                    _ => &[],
                };
                let mut at = i;
                for &len in tokens {
                    let matched = can_close
                        .then(|| open.iter().rposition(|&(oc, ol, _)| oc == c && ol == len))
                        .flatten();
                    if let Some(k) = matched {
                        let (_, _, pos) = open[k];
                        open.truncate(k);
                        spans.push(Span {
                            open: pos..pos + len,
                            close: at..at + len,
                            marks: match (c, len) {
                                (b'~', _) => Marks {
                                    strike: true,
                                    ..Marks::default()
                                },
                                (_, 2) => Marks {
                                    bold: true,
                                    ..Marks::default()
                                },
                                _ => Marks {
                                    italic: true,
                                    ..Marks::default()
                                },
                            },
                        });
                    } else if can_open {
                        open.push((c, len, at));
                    }
                    at += len;
                }
                i += run;
            }
            b'!' if b.get(i + 1) == Some(&b'[') => {
                // Images stay as typed.
                i = link_target(b, i + 1).map_or(i + 2, |(_, end)| end);
            }
            b'[' => {
                if let Some((close, end)) = link_target(b, i) {
                    links.push((i, close, end));
                }
                i += 1;
            }
            b']' => match links.last() {
                Some(&(start, close, end)) if close == i => {
                    links.pop();
                    open.retain(|&(_, _, pos)| pos < start);
                    spans.push(Span {
                        open: start..start + 1,
                        close: i..end,
                        marks: Marks {
                            link: true,
                            ..Marks::default()
                        },
                    });
                    i = end;
                }
                _ => i += 1,
            },
            _ => i += 1,
        }
    }
    spans.sort_by_key(|s| s.open.start);
    (spans, escapes)
}

fn run_len(b: &[u8], i: usize) -> usize {
    b[i..].iter().take_while(|&&c| c == b[i]).count()
}

fn has_open(open: &[(u8, usize, usize)], c: u8, len: usize) -> bool {
    open.iter().any(|&(oc, ol, _)| oc == c && ol == len)
}

/// Next run of exactly `len` bytes `c` at or after `from`.
fn find_run(b: &[u8], from: usize, c: u8, len: usize) -> Option<usize> {
    let mut j = from;
    while j < b.len() {
        if b[j] == c {
            let run = run_len(b, j);
            if run == len {
                return Some(j);
            }
            j += run;
        } else {
            j += 1;
        }
    }
    None
}

/// For `[` at `i`: the matching `]` and the end of the `(…)` right after it.
fn link_target(b: &[u8], i: usize) -> Option<(usize, usize)> {
    let mut depth = 0;
    let mut j = i;
    let close = loop {
        match b.get(j)? {
            b'\\' => j += 1,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    break j;
                }
            }
            b'\n' => return None,
            _ => {}
        }
        j += 1;
    };
    if b.get(close + 1) != Some(&b'(') {
        return None;
    }
    let paren = b[close + 2..]
        .iter()
        .position(|&c| c == b')' || c == b'\n')?;
    let end = close + 2 + paren;
    (b[end] == b')').then_some((close, end + 1))
}

/// Target of the link whose label holds source offset `s`.
pub fn link_at(source: &str, s: usize) -> Option<String> {
    scan(source)
        .0
        .into_iter()
        .find(|span| span.marks.link && span.content().contains(&s))
        .and_then(|span| source.get(span.close.start + 2..span.close.end - 1))
        .map(str::to_string)
}

/// End of the link whose target holds source offset `caret`.
pub fn link_end(source: &str, caret: usize) -> Option<usize> {
    scan(source)
        .0
        .into_iter()
        .find(|s| s.marks.link && caret > s.close.start && caret < s.close.end)
        .map(|s| s.close.end)
}

/// Drop marker pairs left with nothing between them (`****`, `[](url)`), keeping `caret`
/// (a source offset) on the same text.
pub fn tidy(source: &str, caret: usize) -> (String, usize) {
    let mut text = source.to_string();
    let mut caret = caret.min(text.len());
    loop {
        let (spans, _) = scan(&text);
        let Some(empty) = spans.iter().find(|s| s.content().is_empty()) else {
            return (text, caret);
        };
        for r in [empty.close.clone(), empty.open.clone()] {
            text.replace_range(r.clone(), "");
            if caret >= r.end {
                caret -= r.len();
            } else if caret > r.start {
                caret = r.start;
            }
        }
    }
}

/// Cut `source` at source offset `at`; formatting around the cut is closed in the first half
/// and reopened in the second, so both stay valid Markdown (`**ab|cd**` → `**ab**`, `**cd**`).
pub fn split(source: &str, at: usize) -> (String, String) {
    let (before, after) = split_raw(source, at);
    (tidy(&before, 0).0, tidy(&after, 0).0)
}

fn split_raw(source: &str, at: usize) -> (String, String) {
    let at = at.min(source.len());
    let (spans, _) = scan(source);
    let mut around: Vec<&Span> = spans
        .iter()
        .filter(|s| s.open.end <= at && at <= s.close.start)
        .collect();
    // A side the cut leaves empty of a span's text doesn't get its markers: the first half
    // ends before openers right at the cut, the second starts after closers right at it.
    let mut end = at;
    let mut left: Vec<&Span> = around.clone();
    while let Some(k) = left.iter().position(|s| s.open.end == end) {
        end = left.remove(k).open.start;
    }
    let mut start = at;
    while let Some(k) = around.iter().position(|s| s.close.start == start) {
        start = around.remove(k).close.end;
    }
    let mut before = source[..end].to_string();
    for s in left.iter().rev() {
        before.push_str(&source[s.close.clone()]);
    }
    let mut after: String = around.iter().map(|s| &source[s.open.clone()]).collect();
    after.push_str(&source[start..]);
    (before, after)
}

/// Delete source range `range`; formatting whose text is all deleted goes with it. Returns
/// the new source and where the deletion happened.
pub fn delete(source: &str, range: Range<usize>) -> (String, usize) {
    let (spans, _) = scan(source);
    let (mut start, mut end) = (range.start, range.end);
    loop {
        let wider = spans.iter().find(|s| {
            let c = s.content();
            start <= c.start && c.end <= end && (s.open.start < start || s.close.end > end)
        });
        let Some(s) = wider else { break };
        start = start.min(s.open.start);
        end = end.max(s.close.end);
    }
    let mut text = source.to_string();
    text.replace_range(start..end, "");
    tidy(&text, start)
}

/// The Markdown of source range `range` with the formatting around it kept.
pub fn extract(source: &str, range: Range<usize>) -> String {
    let (_, after) = split_raw(source, range.start);
    let shift = after.len() - (source.len() - range.start.min(source.len()));
    let (mid, _) = split_raw(&after, range.end.saturating_sub(range.start) + shift);
    tidy(&mid, 0).0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(s: &str) -> String {
        Inline::parse(s).display
    }

    #[test]
    fn link_under_label() {
        let s = "see [docs](https://x.y/z) now";
        assert_eq!(link_at(s, 6).as_deref(), Some("https://x.y/z"));
        assert_eq!(link_at(s, 1), None);
        assert_eq!(link_at(s, 26), None);
    }

    #[test]
    fn markers_and_targets_are_hidden() {
        assert_eq!(shown("a **b** c"), "a b c");
        assert_eq!(shown("_i_ and *j*"), "i and j");
        assert_eq!(shown("`code` ~~x~~"), "code x");
        assert_eq!(shown("see [docs](https://x.y/z) now"), "see docs now");
        assert_eq!(shown("***both***"), "both");
        assert_eq!(shown(r"\*not\*"), "*not*");
    }

    #[test]
    fn unmatched_and_intraword_markers_stay() {
        assert_eq!(shown("**open"), "**open");
        assert_eq!(shown("snake_case_name"), "snake_case_name");
        assert_eq!(shown("2 * 3 * 4"), "2 * 3 * 4");
        assert_eq!(shown("![img](a.png)"), "![img](a.png)");
        assert_eq!(shown("[no target]"), "[no target]");
        assert_eq!(shown("`a **b** c`"), "a **b** c");
    }

    #[test]
    fn styles_cover_the_formatted_text() {
        let i = Inline::parse("a **b _c_** [d](u)");
        assert_eq!(i.display, "a b c d");
        let bold = Marks {
            bold: true,
            ..Marks::default()
        };
        let both = Marks {
            italic: true,
            ..bold
        };
        let link = Marks {
            link: true,
            ..Marks::default()
        };
        assert_eq!(i.styles, vec![(2..4, bold), (4..5, both), (6..7, link)]);
    }

    #[test]
    fn offsets_map_both_ways() {
        let i = Inline::parse("x **ab** y");
        // display "x ab y"
        assert_eq!(i.to_display(0), 0);
        assert_eq!(i.to_display(3), 2); // inside the opening marker
        assert_eq!(i.to_display(4), 2);
        assert_eq!(i.to_display(6), 4);
        assert_eq!(i.to_display(8), 4); // after the closing marker
        assert_eq!(i.to_source(2, Bias::Left), 2); // before `**`: plain
        assert_eq!(i.to_source(2, Bias::Right), 4); // after `**`
        assert_eq!(i.to_source(4, Bias::Left), 6); // end of "ab", inside bold
        assert_eq!(i.to_source(4, Bias::Right), 8);
        assert_eq!(i.to_source_range(2..4), 4..6);
        // Fully formatted text: the start is inside the markers.
        assert_eq!(Inline::parse("**ab**").to_source(0, Bias::Left), 2);
        assert_eq!(Inline::parse("").to_source(0, Bias::Left), 0);
    }

    #[test]
    fn a_caret_in_a_link_target_reveals_it() {
        let src = "see [docs]() now";
        assert_eq!(Inline::parse_revealing(src, Some(11)).display, src);
        assert_eq!(
            Inline::parse_revealing(src, Some(3)).display,
            "see docs now"
        );
    }

    #[test]
    fn tidy_drops_empty_links() {
        assert_eq!(tidy("a [](u) b", 7), ("a  b".to_string(), 2));
        assert_eq!(tidy("**_ _**", 0).0, "**_ _**");
    }

    #[test]
    fn deleting_all_formatted_text_drops_its_markers() {
        assert_eq!(delete("x **ab** y", 4..6), ("x  y".to_string(), 2));
        assert_eq!(delete("x **a** y", 4..5), ("x  y".to_string(), 2));
        assert_eq!(delete("x **ab** y", 4..5), ("x **b** y".to_string(), 4));
        assert_eq!(delete("**_ab_**", 3..5), (String::new(), 0));
        assert_eq!(delete("see [docs](u)", 5..9), ("see ".to_string(), 4));
    }

    #[test]
    fn split_keeps_formatting_on_both_sides() {
        assert_eq!(split("**abcd**", 4), ("**ab**".into(), "**cd**".into()));
        assert_eq!(split("x **ab** y", 6), ("x **ab**".into(), " y".into()));
        assert_eq!(
            split("[label](u)", 4),
            ("[lab](u)".into(), "[el](u)".into())
        );
        assert_eq!(split("plain", 2), ("pl".into(), "ain".into()));
        assert_eq!(split("x **ab**", 2), ("x ".into(), "**ab**".into()));
        assert_eq!(split("**ab** y", 4), ("**ab**".into(), " y".into()));
    }

    #[test]
    fn extract_wraps_partial_formatting() {
        assert_eq!(extract("x **abcd** y", 5..7), "**bc**");
        assert_eq!(extract("x **ab** y", 0..5), "x **a**");
        assert_eq!(extract("[docs](u) now", 1..3), "[do](u)");
    }
}
