//! The rows of the text as the screen shows them: a line's segments
//! and, with several inputs, the row naming each.

use std::collections::HashMap;
use std::rc::Rc;

use crate::app::text_view::TextView;
use crate::text::buffer::Pos;
use crate::view::layout::{Laid, Layout, Mode};

/// How many lines are kept laid out before starting over.
const LAID_LINES: usize = 1024;

/// A row start: a line's separator row, which orders before the
/// line's own rows, the start of one of its segments, or the end row,
/// at the column past the last line's newline.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Row {
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

/// The cursor's glyph on a screen row.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct CursorCell {
    /// Cells from the row's left edge; negative, scrolled off to the
    /// left.
    pub x: isize,
    pub width: usize,
    /// On screen and clear of the < and > marking text off either
    /// side: they take a cell only where there is text beyond them, and
    /// hide a cursor under them. With no > a glyph cut off at the right
    /// edge shows the cursor on its first cell.
    pub shown: bool,
}

/// Where the view is: a line, or the end row past it.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct At {
    pub line: usize,
    /// The end row: one past `line`, and no line.
    pub past: bool,
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

impl TextView {
    pub(super) fn line_len(&mut self, i: usize) -> usize {
        self.buf.text(i, &mut self.scratch);
        self.scratch.len()
    }

    /// Line `i` laid out for the current width and mode. Layouts are
    /// kept until the width or mode changes, or `LAID_LINES` of them
    /// are held. A kept layout is used only while it fits the text: a
    /// line past the end may appear later, and a file may shrink. The
    /// cursor's line gets a row for its newline while the cursor is on
    /// it, drawn, and the line's last row is full. With wrap markers the
    /// newline has the marker's column, unless a tab wider than the
    /// screen fills that too.
    pub fn line_layout(&mut self, i: usize) -> Rc<Laid> {
        let l = self.geom.layout();
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
        if i != self.cur.line || self.cur.col != n || self.hidden {
            return ln;
        }
        let mut l = l;
        if self.geom.wrap_marks() {
            l.width = self.geom.cols;
        }
        self.laid.newline_row(i, ln, l)
    }

    /// Whether line `i` ends in the input's last newline, after which
    /// comes the end row: an empty row where the cursor rests past the
    /// text, as it rests on the next line's start past any other
    /// newline. The row is no line: the count stays what wc says.
    pub(super) fn last_newline(&self, i: usize) -> bool {
        i + 1 == self.buf.len() && self.buf.trailing_newline()
    }

    /// The last cursor column on line `i`: its newline, or the end row
    /// past it, one more.
    pub(super) fn end_col(&mut self, i: usize) -> usize {
        self.line_len(i) + usize::from(self.last_newline(i))
    }

    /// Whether `p` is the end row.
    pub fn is_end_row(&mut self, p: Row) -> bool {
        p.col
            .is_some_and(|c| self.last_newline(p.line) && c > self.line_len(p.line))
    }

    /// Whether the cursor is on the end row.
    pub(super) fn at_end_row(&mut self) -> bool {
        self.is_end_row(Row::text(self.cur))
    }

    /// The line the view is at: the cursor's, or the top row's while
    /// the cursor is scrolled off screen, so it says where the view is.
    pub fn at(&mut self) -> At {
        if self.cursor_visible() {
            return At {
                line: self.cur.line,
                past: self.at_end_row(),
            };
        }
        let top = self.top();
        At {
            line: top.line,
            past: self.is_end_row(top),
        }
    }

    /// The end of the text, past the last newline if there is one.
    pub(super) fn end_pos(&mut self) -> Pos {
        let n = self.buf.len();
        if n == 0 {
            return Pos::default();
        }
        Pos {
            line: n - 1,
            col: self.end_col(n - 1),
        }
    }

    /// Whether line `i` has a row above it naming its input: the first
    /// line of an input, with separators on and several inputs given.
    /// An input with no lines starts where the next does and gets none.
    pub(super) fn sep_at(&self, i: usize) -> bool {
        self.geom.separators && self.buf.starts_part(i)
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

    /// The cursor's row and cell x on its line `ln`, at the glyph that
    /// shows it: a char inside a cluster has no cell of its own.
    pub(super) fn cursor_pos(&self, ln: &Laid) -> (usize, usize) {
        let (g, _) = ln.glyph(self.cur.col);
        ln.pos(g)
    }

    /// The cursor's glyph as cells of its line: its x, its width and
    /// the line's, for a nowrap view. The glyph is the whole of a
    /// cluster, not its first cell.
    pub(super) fn cursor_span(&mut self) -> (usize, usize, usize) {
        let ln = self.line_layout(self.cur.line);
        let xs = ln.cells();
        if self.cur.col >= xs.len() {
            return (0, 1, xs[xs.len() - 1]);
        }
        let (g, _) = ln.glyph(self.cur.col);
        let (_, x) = ln.pos(g);
        let w = if g + 1 < xs.len() {
            (xs[g + 1] - xs[g]).max(1)
        } else {
            1
        };
        (x, w, xs[xs.len() - 1])
    }

    /// The cursor's glyph on row `p`, when it starts there: where the
    /// frame draws it, and whether. The cursor on any char of a cluster
    /// shows on the cluster, which has no smaller part to show it on.
    /// On the end row it is the first cell. Past the end of a line that
    /// shrank under it, until a motion clamps it, the cursor is nowhere.
    pub fn cursor_on(&mut self, p: Row) -> Option<CursorCell> {
        let col = p.col?;
        if p.line != self.cur.line {
            return None;
        }
        let ln = self.line_layout(p.line);
        let xs = ln.cells();
        let n = xs.len() - 1;
        if self.cur.col > n {
            if col <= n || !self.last_newline(p.line) {
                return None;
            }
            let x = -(self.xoff as isize);
            let shown = x >= 0 && self.geom.cols > 0;
            return Some(CursorCell { x, width: 1, shown });
        }
        if col > n {
            return None;
        }
        let (ci, _) = ln.glyph(self.cur.col);
        let segi = ln.segment_at(col);
        if ln.segment_at(ci) != segi {
            return None;
        }
        let x = xs[ci] as isize - (xs[ln.segments()[segi].start] + self.xoff) as isize;
        // The newline, after the last char, is one cell.
        let width = if ci < n {
            (xs[ci + 1] - xs[ci]).max(1)
        } else {
            1
        };
        let cols = self.geom.cols;
        let mut shown = x >= 0 && x < cols as isize;
        if self.geom.edge_markers && self.geom.mode == Mode::NoWrap {
            let line_width = xs[n];
            if self.xoff > 0 && line_width > 0 {
                shown = shown && x != 0;
            }
            if line_width > self.xoff + cols {
                shown = shown && (x + width as isize) < cols as isize;
            }
        }
        Some(CursorCell { x, width, shown })
    }

    /// Makes row `p` the top one, stopping at the last screenful: each
    /// row missing below `p` is one more above it.
    pub(super) fn place_top(&mut self, mut p: Row) {
        let rows = self.geom.rows;
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
        let n = ln.cells().len() - 1;
        if p.col.is_some_and(|c| c > n) && self.last_newline(line) {
            return Row {
                line,
                col: Some(n + 1),
            };
        }
        let col = ln.segments()[ln.segment_at(p.col.unwrap_or(0))].start;
        Row {
            line,
            col: Some(col),
        }
    }

    /// The row start after `p`, or `p` at the end: the next line's
    /// separator, when it has one, comes before the line.
    pub fn next_row(&mut self, p: Row) -> Row {
        let Some(col) = p.col else {
            return Row::text(Pos {
                line: p.line,
                col: 0,
            });
        };
        let ln = self.line_layout(p.line);
        let n = ln.cells().len() - 1;
        if col > n {
            return p;
        }
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
        if self.last_newline(p.line) {
            return Row {
                line: p.line,
                col: Some(n + 1),
            };
        }
        p
    }

    /// The row start before `p`, or `p` at the beginning: a line's
    /// first row steps back to its separator, when it has one.
    pub(super) fn prev_row(&mut self, p: Row) -> Row {
        if let Some(col) = p.col {
            let ln = self.line_layout(p.line);
            let segs = ln.segments();
            if col >= ln.cells().len() {
                return Row {
                    line: p.line,
                    col: Some(segs[segs.len() - 1].start),
                };
            }
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
}
