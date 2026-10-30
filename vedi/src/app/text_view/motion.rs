//! Where the cursor and the view go: rows, columns, words, inputs, the
//! selection and the scrolls that leave the cursor where it is.

use unicode_width::UnicodeWidthChar;

use crate::app::text_view::{Lead, Row, Selection, TextView};
use crate::text::buffer::Pos;
use crate::view::layout::Mode;

/// How far Left and Right scroll a view-led view sideways.
const SIDE_STEP: isize = 8;

/// A word is letters, digits, marks and underscores: Unicode's L, Nd
/// and M, which std does not expose. `is_alphanumeric` is Alphabetic
/// plus Nd, Nl and No: over L and Nd it adds No (superscripts,
/// fractions, circled numbers) and Nl, the smaller deviation.
pub(crate) fn is_word(c: char) -> bool {
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

/// Where a key, a click or a line number takes the cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Up,
    Down,
    Left,
    Right,
    LineStart,
    LineEnd,
    PageUp,
    PageDown,
    HalfPageUp,
    HalfPageDown,
    WordLeft,
    WordRight,
    First,
    Last,
    ScreenTop,
    ScreenMiddle,
    ScreenBottom,
    PrevInput,
    NextInput,
    To(Pos),
}

impl TextView {
    /// Moves the cursor by `m` and brings it on screen, extending the
    /// selection if `extend` or the mark is set, else dropping it. A
    /// page that does not extend scrolls the view instead and leaves the
    /// cursor and the selection as they are. A view-led view scrolls
    /// by `m` instead.
    pub fn go(&mut self, m: Motion, extend: bool) {
        if self.lead == Lead::View {
            return self.scroll_by(m);
        }
        let extending = extend || self.marking;
        let page = self.page_rows() as isize;
        let half = self.half_page_rows() as isize;
        if !extending {
            match m {
                Motion::PageUp => return self.scroll(-page),
                Motion::PageDown => return self.scroll(page),
                Motion::HalfPageUp => return self.scroll(-half),
                Motion::HalfPageDown => return self.scroll(half),
                _ => {}
            }
        }
        if extend {
            self.extend();
        } else {
            self.drop();
        }
        // Rows a row motion had nowhere to take.
        let mut left = 0;
        match m {
            Motion::Up => left = self.move_rows(-1),
            Motion::Down => left = self.move_rows(1),
            Motion::Left => self.move_col(-1),
            Motion::Right => self.move_col(1),
            Motion::LineStart | Motion::LineEnd if self.at_end_row() => {}
            Motion::LineStart => self.cur.col = 0,
            Motion::LineEnd => self.cur.col = self.line_len(self.cur.line),
            Motion::PageUp => left = self.move_rows(-page),
            Motion::PageDown => left = self.move_rows(page),
            Motion::HalfPageUp => left = self.move_rows(-half),
            Motion::HalfPageDown => left = self.move_rows(half),
            Motion::WordLeft => self.word_left(),
            Motion::WordRight => self.word_right(),
            Motion::First => self.cur = Pos::default(),
            Motion::Last => self.cur = self.end_pos(),
            Motion::ScreenTop => self.screen_row(0),
            Motion::ScreenMiddle => self.screen_row(self.geom.rows / 2),
            Motion::ScreenBottom => self.screen_row(self.geom.rows.saturating_sub(1)),
            Motion::PrevInput => self.prev_input(),
            Motion::NextInput => self.next_input(),
            Motion::To(p) => self.cur = p,
        }
        // Extending past the last row, by Shift or the mark, takes the
        // rest of the last line; past the first, the start of the first.
        if extending && left > 0 {
            self.cur = self.end_pos();
        } else if extending && left < 0 {
            self.cur = Pos::default();
        }
        // The end of the text is a place in the file, not in the line:
        // with wrap off the view keeps its side scroll and the cursor
        // may sit off the right edge.
        let xoff = self.xoff;
        self.scroll_to_cursor();
        if m == Motion::Last && self.geom.mode == Mode::NoWrap {
            self.xoff = xoff;
        }
    }

    /// Moves the cursor `n` visual rows (negative is up), keeping its
    /// cell column where possible, and returns the rows left when the
    /// text ran out. Crossing a line resets the column first:
    /// `line_layout` opens a newline row for the cursor's line, and the
    /// old column must not open one on the new line. A row naming an
    /// input counts as a row crossed, though the cursor cannot stop on
    /// it: one row's move steps over it.
    fn move_rows(&mut self, mut n: isize) -> isize {
        let mut ln = self.line_layout(self.cur.line);
        let (mut row, mut x) = self.cursor_pos(&ln);
        // The end row is one past the last line's rows.
        if self.at_end_row() {
            (row, x) = (ln.rows(), 0);
        }
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
            } else if row < ln.rows() && self.last_newline(self.cur.line) {
                row = ln.rows();
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
        self.cur.col = if row == ln.rows() {
            ln.cells().len()
        } else {
            ln.col(row, x)
        };
        n
    }

    /// Moves one char left or right, crossing lines at the ends, and on
    /// over the rest of a cluster so the cursor rests on a char with a
    /// cell. A line that shrank leaves the cursor past its end; it
    /// comes back first.
    fn move_col(&mut self, d: isize) {
        let n = self.line_len(self.cur.line);
        let end = self.end_col(self.cur.line);
        self.cur.col = self.cur.col.min(end);
        let ln = self.line_layout(self.cur.line);
        let xs = ln.cells();
        if d < 0 {
            if self.cur.col > n {
                self.cur.col = n;
            } else if self.cur.col > 0 {
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
        } else if self.cur.col < end {
            self.cur.col = end;
        } else if self.cur.line + 1 < self.buf.len() {
            self.cur.line += 1;
            self.cur.col = 0;
        }
    }

    /// To the end of the current or next word.
    fn word_right(&mut self) {
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
    fn word_left(&mut self) {
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
    fn next_input(&mut self) {
        let n = self.buf.len();
        let cur = self.cur.line;
        if let Some(start) = self.stops().into_iter().find(|&s| s > cur && s < n) {
            self.jump_to_input(start);
        }
    }

    /// To the stop the cursor's line is under, or from there to the
    /// one before it; on the first line, nowhere.
    fn prev_input(&mut self) {
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

    /// Puts the cursor at the start of the text row `n` rows below the
    /// top of the screen. A row naming an input gives way to the text
    /// row after it, or before it on the bottom row.
    fn screen_row(&mut self, n: usize) {
        let rows = self.geom.rows;
        if rows == 0 || self.buf.is_empty() {
            return;
        }
        let mut p = self.snap(self.top);
        for _ in 0..n {
            p = self.next_row(p);
        }
        if p.col.is_none() && n + 1 >= rows {
            p = self.prev_row(p);
        }
        if p.col.is_none() {
            p = self.next_row(p);
        }
        self.cur = p.pos();
    }

    /// The position drawn at a screen cell: `y` rows down from `top`,
    /// stopping at the buffer's last row; the char whose cells hold `x`,
    /// or the newline past the row's end. A separator row is the line
    /// below, at the same column.
    pub fn cell_pos(&mut self, x: usize, y: usize) -> Pos {
        if self.buf.is_empty() {
            return Pos::default();
        }
        let mut p = self.snap(self.top);
        for _ in 0..y {
            p = self.next_row(p);
        }
        let p = p.pos();
        let ln = self.line_layout(p.line);
        if p.col >= ln.cells().len() {
            return p;
        }
        let (row, _) = ln.pos(p.col);
        Pos {
            line: p.line,
            col: ln.col(row, x + self.xoff),
        }
    }

    /// The selection; `None` when there is no anchor or it equals the
    /// cursor.
    pub fn selection(&mut self) -> Option<Selection> {
        if !self.has_selection() {
            return None;
        }
        let a = self.anchor?;
        let (start, mut end) = (a.min(self.cur), a.max(self.cur));
        let n = self.line_len(end.line);
        // Past its line's newline: the end row, or what was the end row
        // before more lines came in.
        let newline = end.col > n;
        end.col = end.col.min(n);
        Some(Selection {
            start,
            end,
            newline,
        })
    }

    /// Whether anything is selected: an anchor apart from the cursor.
    #[inline]
    pub fn has_selection(&self) -> bool {
        self.anchor.is_some_and(|a| a != self.cur)
    }

    /// The whole text as a selection; `None` with no text.
    pub fn whole(&mut self) -> Option<Selection> {
        let last = self.buf.len().checked_sub(1)?;
        let n = self.line_len(last);
        Some(Selection {
            start: Pos::default(),
            end: Pos { line: last, col: n },
            newline: self.last_newline(last),
        })
    }

    /// Anchors the selection at the cursor if there is none, so a
    /// motion extends it.
    pub fn extend(&mut self) {
        self.anchor.get_or_insert(self.cur);
    }

    /// Clears the selection, unless the mark is set.
    pub(super) fn drop(&mut self) {
        if !self.marking {
            self.anchor = None;
        }
    }

    /// Clears the selection and the mark.
    pub fn deselect(&mut self) {
        self.anchor = None;
        self.marking = false;
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(Pos::default());
        self.cur = self.end_pos();
        self.scroll_to_cursor();
    }

    /// Clears the selection and the mark; with no selection, the
    /// search highlight.
    pub fn clear_selection(&mut self) {
        if self.anchor.is_some() {
            self.deselect();
        } else {
            self.highlight = false;
        }
    }

    /// Sets the mark, so motions extend the selection; again, ends it.
    pub fn set_mark(&mut self) {
        if self.marking {
            self.deselect();
        } else {
            self.marking = true;
            self.extend();
        }
    }

    /// Selects the word around the cursor, if it is on one.
    pub fn select_word(&mut self) {
        self.buf.text(self.cur.line, &mut self.scratch);
        let text = &self.scratch;
        if self.cur.col >= text.len() || !is_word(text[self.cur.col]) {
            return;
        }
        let (mut i, mut j) = (self.cur.col, self.cur.col + 1);
        while i > 0 && is_word(text[i - 1]) {
            i -= 1;
        }
        while j < text.len() && is_word(text[j]) {
            j += 1;
        }
        self.anchor = Some(Pos {
            line: self.cur.line,
            col: i,
        });
        self.cur.col = j;
    }

    /// Selects the cursor's line with its newline, as Shift+Down from
    /// its start would; the last line to its end.
    pub fn select_line(&mut self) {
        self.anchor = Some(Pos {
            line: self.cur.line,
            col: 0,
        });
        self.cur = if self.cur.line + 1 < self.buf.len() {
            Pos {
                line: self.cur.line + 1,
                col: 0,
            }
        } else {
            self.end_pos()
        };
    }

    /// Scrolls a view-led view by `m`, no further than its ends: a row,
    /// a page, to the ends; Left and Right a step, the line keys to the
    /// sides. A motion to a screen row, an input or a position moves
    /// nothing.
    fn scroll_by(&mut self, m: Motion) {
        let n = self.buf.len() as isize;
        let page = self.page_rows() as isize;
        let half = self.half_page_rows() as isize;
        match m {
            Motion::Up => self.scroll_within(-1),
            Motion::Down => self.scroll_within(1),
            Motion::PageUp => self.scroll_within(-page),
            Motion::PageDown => self.scroll_within(page),
            Motion::HalfPageUp => self.scroll_within(-half),
            Motion::HalfPageDown => self.scroll_within(half),
            Motion::First => self.scroll_within(-n),
            Motion::Last => self.scroll_within(n),
            Motion::Left | Motion::WordLeft => self.scroll_sideways_within(-SIDE_STEP),
            Motion::Right | Motion::WordRight => self.scroll_sideways_within(SIDE_STEP),
            Motion::LineStart => {
                let w = self.width() as isize;
                self.scroll_sideways_within(-w);
            }
            Motion::LineEnd => {
                let w = self.width() as isize;
                self.scroll_sideways_within(w);
            }
            Motion::ScreenTop
            | Motion::ScreenMiddle
            | Motion::ScreenBottom
            | Motion::PrevInput
            | Motion::NextInput
            | Motion::To(_) => {}
        }
    }

    /// Moves the view `n` rows (negative is up), keeping the first line
    /// to the last row at the top. The cursor keeps its place, hidden
    /// while off screen: the next motion brings the view back to it.
    /// A view-led view scrolls within its text.
    pub fn scroll(&mut self, mut n: isize) {
        if self.lead == Lead::View {
            return self.scroll_within(n);
        }
        if self.geom.rows == 0 || self.buf.is_empty() {
            return;
        }
        self.top = self.snap(self.top);
        while n > 0 {
            self.top = self.next_row(self.top);
            n -= 1;
        }
        while n < 0 {
            self.top = self.prev_row(self.top);
            n += 1;
        }
    }

    /// Moves the view `n` columns (negative is left), no further right
    /// than brings the widest row on screen and its newline to the right
    /// edge; already past that, after scrolling down to narrower rows, a
    /// right tick stays put rather than turn back. Wrapped, nothing is
    /// off screen to the side. The cursor keeps its place, hidden while
    /// off screen: the next motion brings the view back to it. A
    /// view-led view scrolls within its text.
    pub fn scroll_sideways(&mut self, n: isize) {
        if self.lead == Lead::View {
            return self.scroll_sideways_within(n);
        }
        let l = self.geom.layout();
        let rows = self.geom.rows;
        if l.mode != Mode::NoWrap || l.width == 0 || rows == 0 || self.buf.is_empty() {
            return;
        }
        self.top = self.snap(self.top);
        // A row a line.
        let last = (self.top.line + rows).min(self.buf.len());
        let widest = (self.top.line..last)
            .map(|i| self.line_layout(i).cells().last().copied().unwrap_or(0) + 1)
            .max()
            .unwrap_or(0) as isize;
        let xoff = self.xoff as isize;
        let hi = (widest - l.width as isize).max(xoff).max(0);
        self.xoff = (xoff + n).clamp(0, hi) as usize;
    }

    /// Moves the view `n` rows, no further than the last page; each
    /// line is a row, the mode being `NoWrap`. The cursor is kept on
    /// screen: scrolled off, it moves to the edge row.
    pub(super) fn scroll_within(&mut self, n: isize) {
        let rows = self.geom.rows;
        let last = self.buf.len().saturating_sub(rows) as isize;
        let line = (self.top.line as isize + n).clamp(0, last) as usize;
        self.top = Row::text(Pos { line, col: 0 });
        if self.cur.line < line {
            self.cur = Pos { line, col: 0 };
        } else if self.cur.line >= line + rows {
            self.cur = Pos {
                line: (line + rows).saturating_sub(1),
                col: 0,
            };
        }
    }

    /// Moves the view `n` columns, no further than brings the widest
    /// line's end to the right edge.
    pub(super) fn scroll_sideways_within(&mut self, n: isize) {
        let last = self.width().saturating_sub(self.geom.cols) as isize;
        self.xoff = (self.xoff as isize + n).clamp(0, last) as usize;
    }

    /// The widest line, in cells.
    fn width(&mut self) -> usize {
        (0..self.buf.len())
            .map(|i| *self.line_layout(i).cells().last().unwrap_or(&0))
            .max()
            .unwrap_or(0)
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
