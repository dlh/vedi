//! Where the cursor and the view go: rows, columns, words, inputs, and
//! the startup positions.

use std::collections::HashMap;
use std::rc::Rc;

use unicode_width::UnicodeWidthChar;

use crate::app::{App, LAID_LINES, StartScreen};
use crate::text::buffer::Pos;
use crate::view::layout::{Laid, Layout, Mode};
use crate::view::screen::Terminal;

/// A row start: a line's separator row, which orders before the
/// line's own rows, or the start of one of its segments.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(super) struct Row {
    pub line: usize,
    /// `None` for the row naming the line's input.
    pub col: Option<usize>,
}

impl Row {
    pub fn sep(line: usize) -> Row {
        Row { line, col: None }
    }

    pub fn text(p: Pos) -> Row {
        Row {
            line: p.line,
            col: Some(p.col),
        }
    }

    /// The text position; a separator row's is its line's start.
    pub fn pos(self) -> Pos {
        Pos {
            line: self.line,
            col: self.col.unwrap_or(0),
        }
    }
}

/// A word is letters, digits, marks and underscores: Unicode's L, Nd
/// and M, which std does not expose. `is_alphanumeric` is Alphabetic
/// plus Nd, Nl and No: over L and Nd it adds No (superscripts,
/// fractions, circled numbers) and Nl, the smaller deviation.
pub(super) fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || is_mark(c)
}

/// Approximately M: what takes no cell, less the format characters
/// that also take none.
fn is_mark(c: char) -> bool {
    c.width() == Some(0)
        && !matches!(
            c,
            '\u{ad}' | '\u{61c}' | '\u{180e}' | '\u{200b}'..='\u{200f}'
                | '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}'
                | '\u{e0000}'..='\u{e007f}'
        )
}

/// Lines laid out for one `Layout`, by index.
pub(super) struct LaidCache {
    layout: Layout,
    lines: HashMap<usize, Rc<Laid>>,
    /// The cursor's line with its newline row, and the layout it was
    /// made from, so the row is not laid again while nothing changed.
    newline: Option<(usize, Rc<Laid>, Rc<Laid>)>,
}

impl LaidCache {
    pub fn new(layout: Layout) -> LaidCache {
        LaidCache {
            layout,
            lines: HashMap::new(),
            newline: None,
        }
    }

    /// Forgets every layout: the text changed under them.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.newline = None;
    }

    fn newline_row(&mut self, i: usize, src: Rc<Laid>, l: Layout) -> Rc<Laid> {
        if let Some((line, s, d)) = &self.newline
            && *line == i
            && Rc::ptr_eq(s, &src)
        {
            return Rc::clone(d);
        }
        let d = Rc::new(l.newline_row((*src).clone()));
        self.newline = Some((i, src, Rc::clone(&d)));
        d
    }
}

impl<T: Terminal> App<T> {
    pub(super) fn line_len(&mut self, i: usize) -> usize {
        self.buf.text(i, &mut self.scratch);
        self.scratch.len()
    }

    /// Line `i` laid out for the current width and mode. Layouts are
    /// kept until the width or mode changes, or `LAID_LINES` of them
    /// are held. A kept layout is used only while it fits the text: a
    /// line past the end may appear later, and a file may shrink. The
    /// cursor's line gets a row for its newline while the cursor is on
    /// it and the line's last row is full. With wrap markers the
    /// newline has the marker's column, unless a tab wider than the
    /// screen fills that too.
    pub(super) fn line_layout(&mut self, i: usize) -> Rc<Laid> {
        let l = self.layout();
        if self.laid.layout != l || self.laid.lines.len() >= LAID_LINES {
            self.laid = LaidCache::new(l);
        }
        self.buf.text(i, &mut self.scratch);
        let n = self.scratch.len();
        let ln = match self.laid.lines.get(&i) {
            Some(ln) if ln.cells().len() == n + 1 => Rc::clone(ln),
            _ => {
                let ln = Rc::new(l.line(&self.scratch));
                self.laid.lines.insert(i, Rc::clone(&ln));
                ln
            }
        };
        if i != self.cur.line || self.cur.col < n {
            return ln;
        }
        let mut l = l;
        let (w, _) = self.screen.size();
        if self.wrap_marks(w) {
            l.width = w;
        }
        self.laid.newline_row(i, ln, l)
    }

    pub(super) fn page_rows(&self) -> usize {
        self.text_rows().max(1)
    }

    pub(super) fn half_page_rows(&self) -> usize {
        (self.text_rows() / 2).max(1)
    }

    pub(super) fn end_pos(&mut self) -> Pos {
        let n = self.buf.len();
        if n == 0 {
            return Pos::default();
        }
        Pos {
            line: n - 1,
            col: self.line_len(n - 1),
        }
    }

    /// Whether line `i` has a row above it naming its input: the first
    /// line of an input, with separators on and several inputs given.
    /// An input with no lines starts where the next does and gets none.
    pub(super) fn sep_at(&self, i: usize) -> bool {
        self.settings.file_separators && self.names.len() > 1 && self.buf.starts_part(i)
    }

    /// Row `p` with its line's separator in its place when `p` is the
    /// line's first row and the line has one: the row that names an
    /// input comes into view with its first line.
    pub(super) fn with_sep(&self, p: Pos) -> Row {
        if p.col == 0 && self.sep_at(p.line) {
            return Row::sep(p.line);
        }
        Row::text(p)
    }

    /// Moves the cursor `n` visual rows (negative is up), keeping its
    /// cell column where possible, and returns the rows left when the
    /// text ran out. Crossing a line resets the column first:
    /// `line_layout` opens a newline row for the cursor's line, and the
    /// old column must not open one on the new line. A row naming an
    /// input counts as a row crossed, though the cursor cannot stop on
    /// it: one row's move steps over it.
    pub(super) fn move_rows(&mut self, mut n: isize) -> isize {
        let mut ln = self.line_layout(self.cur.line);
        let (mut row, x) = self.cursor_pos(&ln);
        while n > 0 {
            if row + 1 < ln.rows() {
                row += 1;
            } else if self.cur.line + 1 < self.buf.len() {
                if n > 1 && self.sep_at(self.cur.line + 1) {
                    n -= 1;
                }
                self.cur = Pos {
                    line: self.cur.line + 1,
                    col: 0,
                };
                ln = self.line_layout(self.cur.line);
                row = 0;
            } else {
                break;
            }
            n -= 1;
        }
        while n < 0 {
            if row > 0 {
                row -= 1;
            } else if self.cur.line > 0 {
                if n < -1 && self.sep_at(self.cur.line) {
                    n += 1;
                }
                self.cur = Pos {
                    line: self.cur.line - 1,
                    col: 0,
                };
                ln = self.line_layout(self.cur.line);
                row = ln.rows() - 1;
            } else {
                break;
            }
            n += 1;
        }
        self.cur.col = ln.col(row, x);
        n
    }

    /// The cursor's row and cell x on its line `ln`, at the glyph that
    /// shows it: a char inside a cluster has no cell of its own.
    pub(super) fn cursor_pos(&self, ln: &Laid) -> (usize, usize) {
        let (g, _) = ln.glyph(self.cur.col);
        ln.pos(g)
    }

    /// Moves one char left or right, crossing lines at the ends, and on
    /// over the rest of a cluster so the cursor rests on a char with a
    /// cell. A line that shrank leaves the cursor past its end; it
    /// comes back first.
    pub(super) fn move_col(&mut self, d: isize) {
        let n = self.line_len(self.cur.line);
        self.cur.col = self.cur.col.min(n);
        let ln = self.line_layout(self.cur.line);
        let xs = ln.cells();
        if d < 0 {
            if self.cur.col > 0 {
                self.cur.col -= 1;
                while self.cur.col > 0 && xs[self.cur.col + 1] == xs[self.cur.col] {
                    self.cur.col -= 1;
                }
            } else if self.cur.line > 0 {
                self.cur.line -= 1;
                self.cur.col = self.line_len(self.cur.line);
            }
        } else if self.cur.col < n {
            self.cur.col += 1;
            while self.cur.col < n && xs[self.cur.col + 1] == xs[self.cur.col] {
                self.cur.col += 1;
            }
        } else if self.cur.line + 1 < self.buf.len() {
            self.cur.line += 1;
            self.cur.col = 0;
        }
    }

    /// To the end of the current or next word.
    pub(super) fn word_right(&mut self) {
        self.buf.text(self.cur.line, &mut self.scratch);
        let text = &self.scratch;
        if self.cur.col >= text.len() {
            self.move_col(1);
            return;
        }
        let mut i = self.cur.col;
        while i < text.len() && !is_word(text[i]) {
            i += 1;
        }
        while i < text.len() && is_word(text[i]) {
            i += 1;
        }
        self.cur.col = i;
    }

    /// To the start of the current or previous word.
    pub(super) fn word_left(&mut self) {
        if self.cur.col == 0 {
            self.move_col(-1);
            return;
        }
        self.buf.text(self.cur.line, &mut self.scratch);
        let text = &self.scratch;
        let mut i = self.cur.col.min(text.len());
        while i > 0 && !is_word(text[i - 1]) {
            i -= 1;
        }
        while i > 0 && is_word(text[i - 1]) {
            i -= 1;
        }
        self.cur.col = i;
    }

    /// To the next stop after the cursor's line; past the last,
    /// nowhere. An input with no lines is passed over.
    pub(super) fn next_input(&mut self) {
        let n = self.buf.len();
        let cur = self.cur.line;
        if let Some(start) = self.stops().into_iter().find(|&s| s > cur && s < n) {
            self.jump_to_input(start);
        }
    }

    /// To the stop the cursor's line is under, or from there to the
    /// one before it; on the first line, nowhere.
    pub(super) fn prev_input(&mut self) {
        let cur = self.cur.line;
        let target = self.stops().into_iter().rfind(|&s| s < cur).unwrap_or(0);
        if target < cur {
            self.jump_to_input(target);
        }
    }

    /// Where [ and ] stop, ascending: the line each input begins at and
    /// each line that sets a window title.
    fn stops(&self) -> Vec<usize> {
        let mut stops: Vec<usize> = self.buf.parts().to_vec();
        stops.extend_from_slice(self.buf.titles());
        stops.sort_unstable();
        stops
    }

    /// Puts the cursor on `line`, a stop, with the row naming its input,
    /// or the line itself, at the top.
    fn jump_to_input(&mut self, line: usize) {
        self.cur = Pos { line, col: 0 };
        let top = self.with_sep(self.cur);
        self.place_top(top);
    }

    /// Makes row `p` the top one, stopping at the last screenful: each
    /// row missing below `p` is one more above it.
    pub(super) fn place_top(&mut self, mut p: Row) {
        let rows = self.text_rows();
        let mut below = 0;
        let mut q = p;
        while below + 1 < rows {
            let next = self.next_row(q);
            if next == q {
                break;
            }
            q = next;
            below += 1;
        }
        while below + 1 < rows {
            p = self.prev_row(p);
            below += 1;
        }
        self.top = p;
    }

    /// Clamps `p` to the buffer and moves it back to the start of its
    /// segment, so it can serve as a row start.
    pub(super) fn snap(&mut self, p: Row) -> Row {
        let n = self.buf.len();
        let line = p.line.min(n.saturating_sub(1));
        // A separator row stays one; the first row of an empty buffer
        // waits there for the lines, whose first may have a separator.
        if p.col.is_none() && (n == 0 || self.sep_at(line)) {
            return Row::sep(line);
        }
        if n == 0 {
            return Row::text(Pos::default());
        }
        let ln = self.line_layout(line);
        let col = ln.segments()[ln.segment_at(p.col.unwrap_or(0))].start;
        Row {
            line,
            col: Some(col),
        }
    }

    /// The row start after `p`, or `p` at the end: the next line's
    /// separator, when it has one, comes before the line.
    pub(super) fn next_row(&mut self, p: Row) -> Row {
        let Some(col) = p.col else {
            return Row::text(Pos {
                line: p.line,
                col: 0,
            });
        };
        let ln = self.line_layout(p.line);
        let i = ln.segment_at(col);
        if i + 1 < ln.rows() {
            return Row {
                line: p.line,
                col: Some(ln.segments()[i + 1].start),
            };
        }
        if p.line + 1 < self.buf.len() {
            if self.sep_at(p.line + 1) {
                return Row::sep(p.line + 1);
            }
            return Row::text(Pos {
                line: p.line + 1,
                col: 0,
            });
        }
        p
    }

    /// The row start before `p`, or `p` at the beginning: a line's
    /// first row steps back to its separator, when it has one.
    pub(super) fn prev_row(&mut self, p: Row) -> Row {
        if let Some(col) = p.col {
            let ln = self.line_layout(p.line);
            let i = ln.segment_at(col);
            if i > 0 {
                return Row {
                    line: p.line,
                    col: Some(ln.segments()[i - 1].start),
                };
            }
            if self.sep_at(p.line) {
                return Row::sep(p.line);
            }
        }
        if p.line > 0 {
            let prev = self.line_layout(p.line - 1);
            let segs = prev.segments();
            return Row {
                line: p.line - 1,
                col: Some(segs[segs.len() - 1].start),
            };
        }
        p
    }

    /// Whether the cursor's row is among the text rows on screen; with
    /// no rows or no text, nothing is scrolled away from.
    pub(super) fn cursor_on_screen(&mut self) -> bool {
        let rows = self.text_rows();
        if rows == 0 || self.buf.is_empty() {
            return true;
        }
        let top = self.snap(self.top);
        let crow = self.snap(Row::text(self.cur));
        if crow < top {
            return false;
        }
        let mut bottom = top;
        for _ in 0..rows - 1 {
            bottom = self.next_row(bottom);
        }
        crow <= bottom
    }

    /// Moves `top` and `xoff` the least that brings the cursor on
    /// screen.
    pub(super) fn scroll_to_cursor(&mut self) {
        let rows = self.text_rows();
        if rows == 0 {
            return;
        }
        let n = self.buf.len();
        self.cur.line = self.cur.line.min(n.saturating_sub(1));
        self.cur.col = self.cur.col.min(self.line_len(self.cur.line));
        self.top = self.snap(self.top);
        if n == 0 {
            return;
        }
        let crow = self.snap(Row::text(self.cur));
        if crow < self.top {
            // A row to spare for the separator.
            self.top = if rows > 1 {
                self.with_sep(crow.pos())
            } else {
                crow
            };
        } else {
            // rows-1 rows above the cursor, if below top, is the new top.
            let mut p = crow;
            for _ in 0..rows - 1 {
                if self.top >= p {
                    break;
                }
                p = self.prev_row(p);
            }
            if self.top < p {
                self.top = p;
            }
        }
        let l = self.layout();
        if self.settings.mode != Mode::NoWrap || l.width == 0 {
            self.xoff = 0;
            return;
        }
        // The cursor's whole glyph must fit, not just its first cell,
        // and clear of the < and > that mark text off either side.
        let ln = self.line_layout(self.cur.line);
        let xs = ln.cells();
        let (g, _) = ln.glyph(self.cur.col);
        let (_, x) = ln.pos(g);
        let w = if g + 1 < xs.len() {
            (xs[g + 1] - xs[g]).max(1)
        } else {
            1
        };
        let width = xs[xs.len() - 1];
        let mark = usize::from(self.settings.edge_markers);
        let right = |xoff: usize| if width > xoff + l.width { mark } else { 0 };
        if x < self.xoff + mark {
            self.xoff = x.saturating_sub(mark);
        } else if x + w > self.xoff + l.width - right(self.xoff) {
            let xoff = (x + w).saturating_sub(l.width);
            let xoff = xoff + right(xoff);
            self.xoff = x.saturating_sub(mark).min(xoff);
        }
    }

    /// Runs after the reader appended lines: applies a pending +N,
    /// follows for +G and applies a `StartScreen` at EOF. New text is
    /// not a motion: unless one of those moved the cursor, a view
    /// scrolled sideways off it stays put.
    pub(super) fn on_data(&mut self) {
        // The h key ended startup positioning, and the text view is
        // placed when it comes back.
        if self.text.is_some() {
            return;
        }
        let eof = self.buf.finished().is_some();
        let n = self.buf.len();
        let (cur, xoff) = (self.cur, self.xoff);
        let shown = self.cursor_on_screen();
        if let Some(line) = self.start_line
            && (n > line || eof)
        {
            self.cur = Pos {
                line: line.min(n.saturating_sub(1)),
                col: 0,
            };
            self.top = self.with_sep(self.cur);
            self.start_line = None;
        }
        if self.follow {
            self.cur = Pos {
                line: n.saturating_sub(1),
                col: 0,
            };
        }
        if let Some(s) = self.start_screen
            && eof
        {
            self.show_screen(s);
            self.start_screen = None;
        }
        if shown || self.cur != cur {
            self.scroll_to_cursor();
        }
        if self.cur == cur && self.settings.mode == Mode::NoWrap {
            self.xoff = xoff;
        }
    }

    /// Puts the view where the terminal had it. The buffer's last row
    /// was the terminal's bottom row, so the top is rows-1 plus the
    /// scroll above it; a trailing newline means the bottom row was
    /// blank and not in the buffer, one row fewer. The status line's
    /// row comes off the top of the screenful, and the cursor row moves
    /// up with it. When scrolled, the cursor was below the view and
    /// goes to the top.
    fn show_screen(&mut self, s: StartScreen) {
        if self.buf.is_empty() {
            return;
        }
        let (_, h) = self.screen.size();
        let rows = self.text_rows();
        let back =
            rows as isize - 1 + s.scrolled_by as isize - isize::from(self.buf.trailing_newline());
        let end = self.end_pos();
        let mut top = self.snap(Row::text(end));
        for _ in 0..back {
            top = self.prev_row(top);
        }
        self.top = top;
        self.cur = top.pos();
        if s.scrolled_by > 0 || s.cursor_row == 0 {
            return;
        }
        let mut p = top;
        let steps = (s.cursor_row as isize - (h - rows) as isize).min(rows as isize) - 1;
        for _ in 0..steps {
            p = self.next_row(p);
        }
        let ln = self.line_layout(p.line);
        let (row, _) = ln.pos(p.pos().col);
        self.cur = Pos {
            line: p.line,
            col: ln.col(row, s.cursor_col.saturating_sub(1)),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_chars() {
        for c in ['a', 'Z', '7', '١', '３', '_', 'é', '\u{301}', 'ß'] {
            assert!(is_word(c), "{c:?}");
        }
        for c in [' ', '-', '.', '/', '\u{200d}', '\u{200b}'] {
            assert!(!is_word(c), "{c:?}");
        }
    }
}
