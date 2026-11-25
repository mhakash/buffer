//! The edited text: Markdown source plus a selection kept in display offsets (what the user
//! sees and moves through). Edits go to the source; `caret_src` remembers exactly where the
//! last one ended, so typing right after closing `**` isn't bold while typing at the end of
//! bold text (after a click or arrow) is. Emits the kit's `InputEvent` so it slots in where a
//! `TextareaState` was.

use std::ops::Range;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui_kit::base::actions::{SelectDown, SelectLeft, SelectRight, SelectUp};
use gpui_kit::component::input::{
    Backspace, Copy, Cut, Delete, DeleteToBeginningOfLine, DeleteToEndOfLine, DeleteToNextWordEnd,
    DeleteToPreviousWordStart, Enter, IndentInline, InputEvent, MoveDown, MoveEnd, MoveHome,
    MoveLeft, MoveRight, MoveToEnd, MoveToEndOfLine, MoveToNextWord, MoveToPreviousWord,
    MoveToStart, MoveToStartOfLine, MoveUp, Paste, Redo, SelectAll, SelectToEnd, SelectToEndOfLine,
    SelectToNextWordEnd, SelectToPreviousWordStart, SelectToStart, SelectToStartOfLine,
    ShowCharacterPalette, Undo,
};
use gpui_kit::{
    App, Bounds, ClipboardItem, Context, EntityInputHandler, EventEmitter, FocusHandle, Focusable,
    MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point, SharedString, TextLayout,
    UTF16Selection, Window, point, px, size,
};

use super::inline::{self, Bias, Inline};

/// Clipboard metadata prefix: the copied text's Markdown source.
const MARKDOWN: &str = "buffer-markdown:";

/// Typing within this long of the last edit undoes together.
const COALESCE: Duration = Duration::from_millis(800);

#[derive(Clone)]
struct Snapshot {
    source: String,
    sel: Range<usize>,
}

/// A visual row of the laid-out text, in display offsets.
#[derive(Clone, Copy, Debug)]
pub(super) struct Row {
    pub start: usize,
    pub end: usize,
    pub top: Pixels,
}

pub struct RichTextState {
    pub(super) focus_handle: FocusHandle,
    source: String,
    pub(super) inline: Inline,
    /// Code: shown as typed, no inline formatting.
    plain: bool,
    pub(super) placeholder: SharedString,
    /// Display offsets; `reversed` puts the caret at the start.
    pub(super) sel: Range<usize>,
    reversed: bool,
    /// Exact source caret after an edit (dropped when the caret is moved).
    caret_src: Option<usize>,
    /// IME composition, display offsets.
    pub(super) marked: Option<Range<usize>>,
    /// Layout of the last frame, and the text it was made for.
    pub(super) layout: Option<(TextLayout, String)>,
    pub(super) bounds: Option<Bounds<Pixels>>,
    pub(super) selecting: bool,
    /// No click or edit since the text was loaded: a click on a link follows it.
    fresh: bool,
    /// Link under the press that may be followed on release.
    press_link: Option<String>,
    /// Where relative link targets resolve.
    link_dir: PathBuf,
    /// x the caret keeps while moving up and down.
    goal_x: Option<Pixels>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last_edit: Option<Instant>,
}

impl EventEmitter<InputEvent> for RichTextState {}

impl Focusable for RichTextState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl RichTextState {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        cx.on_focus(&focus_handle, window, |_, _, cx| cx.emit(InputEvent::Focus))
            .detach();
        cx.on_blur(&focus_handle, window, |this, _, cx| {
            this.selecting = false;
            this.marked = None;
            cx.emit(InputEvent::Blur);
            cx.notify();
        })
        .detach();
        Self {
            focus_handle,
            source: String::new(),
            inline: Inline::parse(""),
            plain: false,
            placeholder: SharedString::default(),
            sel: 0..0,
            reversed: false,
            caret_src: None,
            marked: None,
            layout: None,
            bounds: None,
            selecting: false,
            fresh: false,
            press_link: None,
            link_dir: PathBuf::new(),
            goal_x: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
        }
    }

    // ---- what the block editor uses (source offsets) ----

    pub fn value(&self) -> &str {
        &self.source
    }

    /// Replace the text without emitting `Change`; starts a fresh undo history.
    pub fn set_value(&mut self, text: impl Into<String>, _: &mut Window, cx: &mut Context<Self>) {
        self.source = text.into();
        self.reparse();
        let end = self.inline.display.len();
        self.sel = end..end;
        self.reversed = false;
        self.caret_src = None;
        self.marked = None;
        self.fresh = true;
        self.undo.clear();
        self.redo.clear();
        self.last_edit = None;
        cx.notify();
    }

    /// Where relative link targets resolve.
    pub fn set_link_dir(&mut self, dir: PathBuf) {
        self.link_dir = dir;
    }

    /// Code blocks: no inline formatting.
    pub fn set_plain(&mut self, plain: bool, cx: &mut Context<Self>) {
        if self.plain != plain {
            self.plain = plain;
            self.reparse();
            cx.notify();
        }
    }

    pub fn set_placeholder(
        &mut self,
        text: impl Into<SharedString>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.placeholder = text.into();
        cx.notify();
    }

    /// The selection in source offsets (just the visible text when it isn't empty).
    pub fn selected_range(&self) -> Range<usize> {
        if self.sel.is_empty() {
            let s = self.caret_source();
            return s..s;
        }
        self.inline.to_source_range(self.sel.clone())
    }

    pub fn set_selected_range(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        let (start, end) = (
            range.start.min(self.source.len()),
            range.end.min(self.source.len()),
        );
        self.caret_src = (start == end).then_some(start);
        self.reparse();
        self.sel = self.inline.to_display(start)..self.inline.to_display(end);
        self.reversed = false;
        self.goal_x = None;
        cx.notify();
    }

    /// Replace the whole text as one undoable edit (emits `Change`).
    pub fn replace_all(&mut self, text: impl Into<String>, _: &mut Window, cx: &mut Context<Self>) {
        self.record(false);
        self.source = text.into();
        self.reparse();
        let end = self.inline.display.len();
        self.sel = self.sel.start.min(end)..self.sel.end.min(end);
        self.caret_src = None;
        cx.emit(InputEvent::Change);
        cx.notify();
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus_handle.focus(window, cx);
    }

    /// Where the text was drawn last frame.
    pub fn text_bounds(&self) -> Option<Bounds<Pixels>> {
        self.bounds
    }

    pub fn at_start(&self) -> bool {
        self.sel == (0..0)
    }

    pub fn at_end(&self) -> bool {
        let end = self.inline.display.len();
        self.sel == (end..end)
    }

    pub fn all_selected(&self) -> bool {
        self.sel == (0..self.inline.display.len())
    }

    /// The text before and after the caret (the selection dropped), formatting balanced.
    pub fn split_at_caret(&self) -> (String, String) {
        let range = self.selected_range();
        let (before, _) = inline::split(&self.source, range.start);
        let (_, after) = inline::split(&self.source, range.end);
        (before, after)
    }

    /// (visual row of the caret, rows) in the last layout.
    pub fn caret_row(&self) -> (usize, usize) {
        let rows = self.rows();
        let caret = self.cursor();
        let row = rows
            .iter()
            .position(|r| caret <= r.end)
            .unwrap_or(rows.len().saturating_sub(1));
        (row, rows.len().max(1))
    }

    // ---- internals ----

    fn reparse(&mut self) {
        self.inline = if self.plain {
            Inline::plain(&self.source)
        } else {
            Inline::parse_revealing(&self.source, self.caret_src)
        };
    }

    pub(super) fn display(&self) -> &str {
        &self.inline.display
    }

    pub(super) fn cursor(&self) -> usize {
        if self.reversed {
            self.sel.start
        } else {
            self.sel.end
        }
    }

    fn caret_source(&self) -> usize {
        self.caret_src
            .filter(|s| self.inline.to_display(*s) == self.sel.start)
            .unwrap_or_else(|| self.inline.to_source(self.sel.start, Bias::Left))
    }

    /// Save the state for ⌘Z; `typing` coalesces with the previous typing.
    fn record(&mut self, typing: bool) {
        let now = Instant::now();
        let coalesce = typing && self.last_edit.is_some_and(|t| now - t < COALESCE);
        if !coalesce {
            self.undo.push(Snapshot {
                source: self.source.clone(),
                sel: self.sel.clone(),
            });
            if self.undo.len() > 200 {
                self.undo.remove(0);
            }
        }
        self.last_edit = typing.then_some(now);
        self.redo.clear();
    }

    /// Replace display range `range` (the selection when `None`) with `text`.
    fn replace(&mut self, range: Option<Range<usize>>, text: &str, cx: &mut Context<Self>) {
        let range = range.unwrap_or(self.sel.clone());
        let src = if range == self.sel {
            self.selected_range()
        } else {
            self.inline.to_source_range(range)
        };
        self.fresh = false;
        self.record(!text.is_empty() && !text.contains(char::is_whitespace));
        let (mut source, at) = if text.is_empty() {
            inline::delete(&self.source, src)
        } else {
            let mut s = self.source.clone();
            s.replace_range(src.clone(), text);
            (s, src.start + text.len())
        };
        let mut caret = at;
        if !text.is_empty() {
            (source, caret) = inline::tidy(&source, caret);
        }
        self.source = source;
        self.caret_src = Some(caret);
        self.reparse();
        let d = self.inline.to_display(caret);
        self.sel = d..d;
        self.reversed = false;
        self.goal_x = None;
        cx.emit(InputEvent::Change);
        cx.notify();
    }

    pub(super) fn move_to(&mut self, d: usize, cx: &mut Context<Self>) {
        self.fresh = false;
        let d = self.settle(self.clamp(d));
        self.sel = d..d;
        self.reversed = false;
        cx.notify();
    }

    pub(super) fn select_to(&mut self, d: usize, cx: &mut Context<Self>) {
        self.fresh = false;
        let anchor = if self.reversed {
            self.sel.end
        } else {
            self.sel.start
        };
        let (anchor, d) = if self.inline.revealed {
            let a = self.inline.to_source(anchor, Bias::Left);
            let h = self.inline.to_source(self.clamp(d), Bias::Left);
            self.caret_src = None;
            self.reparse();
            (self.inline.to_display(a), self.inline.to_display(h))
        } else {
            (anchor, self.clamp(d))
        };
        self.reversed = d < anchor;
        self.sel = anchor.min(d)..anchor.max(d);
        self.caret_src = None;
        cx.notify();
    }

    /// The caret leaves where the last edit put it: a link shown as typed (its target being
    /// edited) folds back to its label. Returns `d` in the new display offsets.
    fn settle(&mut self, d: usize) -> usize {
        let src = self.inline.to_source(d, Bias::Left);
        let revealed = self.inline.revealed;
        self.caret_src = None;
        if !revealed {
            return d;
        }
        self.reparse();
        self.inline.to_display(src)
    }

    /// Enter while editing a link's target: put the caret after the link. False otherwise.
    pub fn finish_link(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(end) = self
            .caret_src
            .filter(|_| self.inline.revealed)
            .and_then(|c| inline::link_end(&self.source, c))
        else {
            return false;
        };
        self.caret_src = Some(end);
        self.reparse();
        let d = self.inline.to_display(end);
        self.sel = d..d;
        self.reversed = false;
        cx.notify();
        true
    }

    fn clamp(&self, d: usize) -> usize {
        let text = self.display();
        let mut d = d.min(text.len());
        while !text.is_char_boundary(d) {
            d -= 1;
        }
        d
    }

    fn prev_char(&self, d: usize) -> usize {
        self.display()[..d]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    fn next_char(&self, d: usize) -> usize {
        self.display()[d..]
            .chars()
            .next()
            .map_or(d, |c| d + c.len_utf8())
    }

    fn prev_word(&self, d: usize) -> usize {
        let text = &self.display()[..d];
        let trimmed = text.trim_end();
        let kind = trimmed.chars().next_back().map(word_char);
        trimmed
            .char_indices()
            .rev()
            .take_while(|(_, c)| Some(word_char(*c)) == kind && !c.is_whitespace())
            .last()
            .map_or(trimmed.len(), |(i, _)| i)
    }

    fn next_word(&self, d: usize) -> usize {
        let text = &self.display()[d..];
        let lead = text.len() - text.trim_start().len();
        let rest = &text[lead..];
        let kind = rest.chars().next().map(word_char);
        let len: usize = rest
            .chars()
            .take_while(|c| Some(word_char(*c)) == kind && !c.is_whitespace())
            .map(char::len_utf8)
            .sum();
        d + lead + len
    }

    /// Word at `d` (double click).
    fn word_at(&self, d: usize) -> Range<usize> {
        let text = self.display();
        let Some(c) = text[d..]
            .chars()
            .next()
            .or_else(|| text[..d].chars().next_back())
        else {
            return d..d;
        };
        let kind = word_char(c);
        let start = text[..d]
            .char_indices()
            .rev()
            .take_while(|(_, c)| word_char(*c) == kind)
            .last()
            .map_or(d, |(i, _)| i);
        let end = d + text[d..]
            .chars()
            .take_while(|c| word_char(*c) == kind)
            .map(char::len_utf8)
            .sum::<usize>();
        start..end
    }

    // ---- layout ----

    fn layout(&self) -> Option<&TextLayout> {
        self.layout
            .as_ref()
            .filter(|(_, text)| *text == self.display() || self.display().is_empty())
            .map(|(l, _)| l)
    }

    pub(super) fn rows(&self) -> Vec<Row> {
        let (Some(layout), Some(bounds)) = (self.layout(), self.bounds) else {
            return vec![Row {
                start: 0,
                end: self.display().len(),
                top: px(0.),
            }];
        };
        let lh = layout.line_height();
        let mut rows = Vec::new();
        let mut top = bounds.top();
        let mut line_start = 0;
        for line in layout.line_layouts() {
            let mut start = 0;
            for b in line.wrap_boundaries() {
                let ix = line.runs()[b.run_ix].glyphs[b.glyph_ix].index;
                rows.push(Row {
                    start: line_start + start,
                    end: line_start + ix,
                    top,
                });
                start = ix;
                top += lh;
            }
            rows.push(Row {
                start: line_start + start,
                end: line_start + line.len(),
                top,
            });
            top += lh;
            line_start += line.len() + 1;
        }
        let len = self.display().len();
        for r in &mut rows {
            r.start = r.start.min(len);
            r.end = r.end.min(len);
        }
        rows
    }

    pub(super) fn position_of(&self, d: usize) -> Option<Point<Pixels>> {
        let layout = self.layout()?;
        if self.display().is_empty() {
            return Some(self.bounds?.origin);
        }
        layout.position_for_index(d.min(self.display().len()))
    }

    pub(super) fn line_height(&self) -> Pixels {
        self.layout().map_or(px(20.), TextLayout::line_height)
    }

    pub(super) fn index_at(&self, position: Point<Pixels>) -> usize {
        let Some(layout) = self.layout() else {
            return self.cursor();
        };
        if self.display().is_empty() {
            return 0;
        }
        let ix = match layout.index_for_position(position) {
            Ok(ix) | Err(ix) => ix,
        };
        self.clamp(ix)
    }

    fn vertical(&mut self, step: isize, extend: bool, cx: &mut Context<Self>) {
        let rows = self.rows();
        let (row, _) = self.caret_row();
        let target = row as isize + step;
        let lh = self.line_height();
        let x = self
            .goal_x
            .or_else(|| self.position_of(self.cursor()).map(|p| p.x));
        let d = if target < 0 {
            0
        } else if target as usize >= rows.len() {
            self.display().len()
        } else {
            let r = rows[target as usize];
            match x {
                Some(x) => self.index_at(point(x, r.top + lh / 2.)),
                None => r.start,
            }
        };
        if extend {
            self.select_to(d, cx);
        } else {
            self.move_to(d, cx);
        }
        self.goal_x = x;
    }

    fn row_bounds(&self) -> (usize, usize) {
        let rows = self.rows();
        let (row, _) = self.caret_row();
        rows.get(row)
            .map_or((0, self.display().len()), |r| (r.start, r.end))
    }

    // ---- mouse ----

    pub(super) fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        self.focus(window, cx);
        let d = self.index_at(event.position);
        self.goal_x = None;
        // A plain click on a link follows it when the block was just opened by that click
        // (it looked like rendered text), or with ⌘ any time.
        self.press_link = (!self.plain
            && !event.modifiers.shift
            && event.click_count <= 1
            && (self.fresh || event.modifiers.secondary(cx)))
        .then(|| self.link_under(event.position, d))
        .flatten();
        if event.modifiers.shift {
            self.select_to(d, cx);
        } else {
            match event.click_count {
                2 => {
                    let w = self.word_at(d);
                    self.sel = w;
                    self.reversed = false;
                    self.caret_src = None;
                    cx.notify();
                }
                n if n >= 3 => {
                    self.sel = 0..self.display().len();
                    self.reversed = false;
                    self.caret_src = None;
                    cx.notify();
                }
                _ => self.move_to(d, cx),
            }
        }
        // Synthetic presses (a click replayed into a block just opened) can carry 0.
        self.selecting = event.click_count <= 1 && !event.modifiers.shift;
    }

    pub(super) fn on_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selecting = false;
        if let Some(url) = self.press_link.take()
            && self.sel.is_empty()
            && event.button == MouseButton::Left
        {
            crate::components::open_link(&url, event.position, &self.link_dir, window, cx);
        }
    }

    pub(super) fn press_link_pending(&self) -> bool {
        self.press_link.is_some()
    }

    /// Target of the link drawn at `position` (display offset `d`), if any.
    fn link_under(&self, position: Point<Pixels>, d: usize) -> Option<String> {
        let layout = self.layout()?;
        // `index_for_position` rounds to the nearest gap: only count a hit on a glyph.
        layout.index_for_position(position).ok()?;
        let on_link = |d: usize| {
            self.inline
                .styles
                .iter()
                .any(|(r, m)| m.link && r.start <= d && d < r.end)
        };
        let d = if on_link(d) {
            d
        } else {
            d.checked_sub(1).filter(|d| on_link(*d))?
        };
        inline::link_at(&self.source, self.inline.to_source(d, Bias::Right))
    }

    pub(super) fn on_drag(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if self.selecting {
            let d = self.index_at(position);
            self.select_to(d, cx);
        }
    }

    // ---- actions ----

    pub(super) fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if self.sel.is_empty() {
            let d = self.sel.start;
            if d == 0 {
                return;
            }
            let prev = self.prev_char(d);
            self.replace(Some(prev..d), "", cx);
        } else {
            self.replace(None, "", cx);
        }
    }

    pub(super) fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        if self.sel.is_empty() {
            let d = self.sel.start;
            let next = self.next_char(d);
            if next > d {
                self.replace(Some(d..next), "", cx);
            }
        } else {
            self.replace(None, "", cx);
        }
    }

    fn delete_to(&mut self, d: usize, cx: &mut Context<Self>) {
        if !self.sel.is_empty() {
            self.replace(None, "", cx);
            return;
        }
        let c = self.sel.start;
        let range = c.min(d)..c.max(d);
        if !range.is_empty() {
            self.replace(Some(range), "", cx);
        }
    }

    pub(super) fn delete_to_line_start(
        &mut self,
        _: &DeleteToBeginningOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (start, _) = self.row_bounds();
        self.delete_to(start, cx);
    }

    pub(super) fn delete_to_line_end(
        &mut self,
        _: &DeleteToEndOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (_, end) = self.row_bounds();
        self.delete_to(end, cx);
    }

    pub(super) fn delete_word_back(
        &mut self,
        _: &DeleteToPreviousWordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let d = self.prev_word(self.sel.start);
        self.delete_to(d, cx);
    }

    pub(super) fn delete_word_forward(
        &mut self,
        _: &DeleteToNextWordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let d = self.next_word(self.sel.end);
        self.delete_to(d, cx);
    }

    pub(super) fn left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        let d = if self.sel.is_empty() {
            self.prev_char(self.sel.start)
        } else {
            self.sel.start
        };
        self.move_to(d, cx);
        self.goal_x = None;
    }

    pub(super) fn right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        let d = if self.sel.is_empty() {
            self.next_char(self.sel.end)
        } else {
            self.sel.end
        };
        self.move_to(d, cx);
        self.goal_x = None;
    }

    pub(super) fn up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        self.vertical(-1, false, cx);
    }

    pub(super) fn down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        self.vertical(1, false, cx);
    }

    pub(super) fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        let d = self.prev_char(self.cursor());
        self.select_to(d, cx);
    }

    pub(super) fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        let d = self.next_char(self.cursor());
        self.select_to(d, cx);
    }

    pub(super) fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.vertical(-1, true, cx);
    }

    pub(super) fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.vertical(1, true, cx);
    }

    pub(super) fn home(&mut self, _: &MoveHome, _: &mut Window, cx: &mut Context<Self>) {
        let (start, _) = self.row_bounds();
        self.move_to(start, cx);
    }

    pub(super) fn end(&mut self, _: &MoveEnd, _: &mut Window, cx: &mut Context<Self>) {
        let (_, end) = self.row_bounds();
        self.move_to(end, cx);
    }

    pub(super) fn line_start(
        &mut self,
        _: &MoveToStartOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (start, _) = self.row_bounds();
        self.move_to(start, cx);
    }

    pub(super) fn line_end(&mut self, _: &MoveToEndOfLine, _: &mut Window, cx: &mut Context<Self>) {
        let (_, end) = self.row_bounds();
        self.move_to(end, cx);
    }

    pub(super) fn to_start(&mut self, _: &MoveToStart, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    pub(super) fn to_end(&mut self, _: &MoveToEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.display().len(), cx);
    }

    pub(super) fn word_left(
        &mut self,
        _: &MoveToPreviousWord,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let d = self.prev_word(self.cursor());
        self.move_to(d, cx);
    }

    pub(super) fn word_right(
        &mut self,
        _: &MoveToNextWord,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let d = self.next_word(self.cursor());
        self.move_to(d, cx);
    }

    pub(super) fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.sel = 0..self.display().len();
        self.reversed = false;
        self.caret_src = None;
        cx.notify();
    }

    pub(super) fn select_line_start(
        &mut self,
        _: &SelectToStartOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (start, _) = self.row_bounds();
        self.select_to(start, cx);
    }

    pub(super) fn select_line_end(
        &mut self,
        _: &SelectToEndOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (_, end) = self.row_bounds();
        self.select_to(end, cx);
    }

    pub(super) fn select_start(
        &mut self,
        _: &SelectToStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(0, cx);
    }

    pub(super) fn select_end(&mut self, _: &SelectToEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.display().len(), cx);
    }

    pub(super) fn select_word_left(
        &mut self,
        _: &SelectToPreviousWordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let d = self.prev_word(self.cursor());
        self.select_to(d, cx);
    }

    pub(super) fn select_word_right(
        &mut self,
        _: &SelectToNextWordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let d = self.next_word(self.cursor());
        self.select_to(d, cx);
    }

    /// Copies Markdown, so formatting survives a paste back into a note.
    /// Copies the text as shown, with its Markdown attached so formatting survives a paste
    /// back into a note. With nothing selected the action goes on (block selection).
    pub(super) fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if self.sel.is_empty() {
            cx.propagate();
            return;
        }
        let shown = self.display()[self.sel.clone()].to_string();
        let markdown = inline::extract(&self.source, self.selected_range());
        cx.write_to_clipboard(if shown == markdown {
            ClipboardItem::new_string(shown)
        } else {
            ClipboardItem::new_string_with_metadata(shown, format!("{MARKDOWN}{markdown}"))
        });
    }

    pub(super) fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.sel.is_empty() {
            self.copy(&Copy, window, cx);
            self.replace(None, "", cx);
        }
    }

    pub(super) fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let item = cx.read_from_clipboard();
        let markdown = item
            .as_ref()
            .and_then(|c| c.metadata())
            .and_then(|m| m.strip_prefix(MARKDOWN))
            .map(str::to_string);
        if let Some(text) = markdown.or_else(|| item.and_then(|c| c.text())) {
            self.last_edit = None;
            self.replace(None, &text, cx);
            self.last_edit = None;
        }
    }

    pub(super) fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.restore(false, cx);
    }

    pub(super) fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        self.restore(true, cx);
    }

    fn restore(&mut self, redo: bool, cx: &mut Context<Self>) {
        let (from, to) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        let Some(snap) = from.pop() else { return };
        to.push(Snapshot {
            source: std::mem::take(&mut self.source),
            sel: self.sel.clone(),
        });
        self.source = snap.source;
        self.reparse();
        self.sel = self.clamp(snap.sel.start)..self.clamp(snap.sel.end);
        self.caret_src = None;
        self.last_edit = None;
        cx.emit(InputEvent::Change);
        cx.notify();
    }

    /// Enter that reaches the text (the editor takes it in text blocks): a line break.
    pub(super) fn enter(&mut self, action: &Enter, _: &mut Window, cx: &mut Context<Self>) {
        if !action.secondary {
            self.last_edit = None;
            self.replace(None, "\n", cx);
        }
    }

    pub(super) fn tab(&mut self, _: &IndentInline, _: &mut Window, cx: &mut Context<Self>) {
        if self.plain {
            self.replace(None, "    ", cx);
        }
    }

    pub(super) fn character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    // ---- UTF-16 (platform input) ----

    fn to_utf16(&self, d: usize) -> usize {
        self.display()[..d.min(self.display().len())]
            .encode_utf16()
            .count()
    }

    fn from_utf16(&self, u: usize) -> usize {
        let mut units = 0;
        for (i, c) in self.display().char_indices() {
            if units >= u {
                return i;
            }
            units += c.len_utf16();
        }
        self.display().len()
    }

    fn range_to_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.to_utf16(r.start)..self.to_utf16(r.end)
    }

    fn range_from_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.from_utf16(r.start)..self.from_utf16(r.end)
    }
}

impl EntityInputHandler for RichTextState {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range);
        actual.replace(self.range_to_utf16(&range));
        Some(self.display()[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.sel),
            reversed: self.reversed,
        })
    }

    fn set_selected_text_range(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let r = self.range_from_utf16(&range);
        self.sel = r;
        self.reversed = false;
        self.caret_src = None;
        cx.notify();
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.to_utf16(self.display().len()))
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|r| self.range_to_utf16(r))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .map(|r| self.range_from_utf16(&r))
            .or(self.marked.take());
        self.marked = None;
        self.replace(range, text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .map(|r| self.range_from_utf16(&r))
            .or(self.marked.take());
        let start = range.as_ref().map_or(self.sel.start, |r| r.start);
        self.replace(range, text, cx);
        if text.is_empty() {
            self.marked = None;
            return;
        }
        let end = self.sel.start;
        self.marked = Some(start..end);
        if let Some(sel) = selected {
            let base = self.to_utf16(start);
            let len = text.encode_utf16().count();
            let s = self.from_utf16(base + sel.start.min(len));
            let e = self.from_utf16(base + sel.end.min(len));
            self.sel = s..e;
        }
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range);
        let start = self.position_of(range.start)?;
        let end = self.position_of(range.end).unwrap_or(start);
        let width = if end.y == start.y {
            end.x - start.x
        } else {
            px(1.)
        };
        Some(Bounds::new(start, size(width, self.line_height())))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let d = self.index_at(point);
        Some(self.to_utf16(d))
    }
}

/// Word characters vs. punctuation vs. space, for word motion.
fn word_char(c: char) -> u8 {
    if c.is_alphanumeric() || c == '_' {
        0
    } else if c.is_whitespace() {
        1
    } else {
        2
    }
}
