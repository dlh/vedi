//! Where the cursor and the view go: rows, columns, words, inputs, the
//! selection and the scrolls that leave the cursor where it is.

use unicode_width::UnicodeWidthChar;

use crate::app::text_view::{Row, TextView};
use crate::text::buffer::Pos;
use crate::view::layout::Mode;

/// A word is letters, digits, marks and underscores: Unicode's L, Nd
/// and M, which std does not expose. `is_alphanumeric` is Alphabetic
/// plus Nd, Nl and No: over L and Nd it adds No (superscripts,
/// fractions, circled numbers) and Nl, the smaller deviation.
fn is_word(c: char) -> bool {
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

impl TextView {
    /// Moves the cursor `n` visual rows (negative is up), keeping its
    /// cell column where possible, and returns the rows left when the
    /// text ran out. Crossing a line resets the column first:
    /// `line_layout` opens a newline row for the cursor's line, and the
    /// old column must not open one on the new line. A row naming an
    /// input counts as a row crossed, though the cursor cannot stop on
    /// it: one row's move steps over it.
    pub fn move_rows(&mut self, mut n: isize) -> isize {
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

    /// Moves one char left or right, crossing lines at the ends, and on
    /// over the rest of a cluster so the cursor rests on a char with a
    /// cell. A line that shrank leaves the cursor past its end; it
    /// comes back first.
    pub fn move_col(&mut self, d: isize) {
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

    pub fn line_start(&mut self) {
        self.cur.col = 0;
    }

    pub fn line_end(&mut self) {
        self.cur.col = self.line_len(self.cur.line);
    }

    /// To the end of the current or next word.
    pub fn word_right(&mut self) {
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
    pub fn word_left(&mut self) {
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
    pub fn next_input(&mut self) {
        let n = self.buf.len();
        let cur = self.cur.line;
        if let Some(start) = self.stops().into_iter().find(|&s| s > cur && s < n) {
            self.jump_to_input(start);
        }
    }

    /// To the stop the cursor's line is under, or from there to the
    /// one before it; on the first line, nowhere.
    pub fn prev_input(&mut self) {
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
    pub fn screen_row(&mut self, n: usize) {
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
        let (row, _) = ln.pos(p.col);
        Pos {
            line: p.line,
            col: ln.col(row, x + self.xoff),
        }
    }

    /// The ordered selection bounds; `None` when there is no anchor or
    /// it equals the cursor.
    pub fn selection(&self) -> Option<(Pos, Pos)> {
        let a = self.anchor?;
        if a == self.cur {
            return None;
        }
        Some((a.min(self.cur), a.max(self.cur)))
    }

    /// The mark is set: motions extend the selection.
    pub fn marking(&self) -> bool {
        self.marking
    }

    /// Anchors the selection at the cursor if there is none, so a
    /// motion extends it.
    pub fn extend(&mut self) {
        self.anchor.get_or_insert(self.cur);
    }

    /// Clears the selection, unless the mark is set.
    pub fn drop(&mut self) {
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

    /// Moves the view `n` rows (negative is up), keeping the first line
    /// to the last row at the top. The cursor keeps its place, hidden
    /// while off screen: the next motion brings the view back to it.
    pub fn scroll(&mut self, mut n: isize) {
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
    /// off screen: the next motion brings the view back to it.
    pub fn scroll_sideways(&mut self, n: isize) {
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
    pub fn scroll_within(&mut self, n: isize) {
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
    pub fn scroll_sideways_within(&mut self, n: isize) {
        let last = self.width().saturating_sub(self.geom.cols) as isize;
        self.xoff = (self.xoff as isize + n).clamp(0, last) as usize;
    }

    /// The widest line, in cells.
    pub fn width(&mut self) -> usize {
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
