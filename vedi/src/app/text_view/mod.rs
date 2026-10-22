//! The text, where the cursor and the selection are in it, and the view
//! of it: which rows and columns the screen shows. Only a motion brings
//! a view scrolled off the cursor back; new text, a reload, a resize or
//! a new layout keeps the view where it is and only clamps, unless the
//! cursor was on screen before.

mod motion;
mod rows;
mod search;

use crate::app::StartScreen;
use crate::text::buffer::{Buffer, Pos, Progress};
use crate::text::search::Matcher;
use crate::view::layout::{Layout, Mode, WrapStyle};
pub use motion::Motion;
pub(crate) use motion::is_word;
use rows::LaidCache;
pub use rows::Row;
pub use search::Outcome;

/// The screen the view is laid out for.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct Geometry {
    /// The rows the text has.
    pub rows: usize,
    /// The screen's rows and columns.
    pub height: usize,
    pub cols: usize,
    pub mode: Mode,
    /// Cells per tab stop; zero is the default.
    pub tab: usize,
    pub wrap_style: WrapStyle,
    /// A row names each input.
    pub separators: bool,
    /// Text off the sides is marked with < and >, a wrapped row with \.
    pub edge_markers: bool,
}

impl Geometry {
    /// Whether wrap mode marks rows that continue: the last column is
    /// then the marker's, and text wraps a column early. Under three
    /// columns there is no room.
    pub fn wrap_marks(&self) -> bool {
        self.edge_markers && self.mode == Mode::Wrap && self.cols > 2
    }

    pub fn layout(&self) -> Layout {
        let width = if self.wrap_marks() {
            self.cols - 1
        } else {
            self.cols
        };
        Layout {
            width,
            mode: self.mode,
            tab: self.tab,
            wrap_style: self.wrap_style,
        }
    }
}

/// Where the view starts, applied as the text comes in.
#[derive(Copy, Clone, Debug, Default)]
pub struct Start {
    /// The 0-based +N target.
    pub line: Option<usize>,
    /// Keep the cursor on the last line until EOF (+G).
    pub follow: bool,
    /// Applied at EOF; no text is drawn until then.
    pub screen: Option<StartScreen>,
}

pub struct TextView {
    buf: Buffer,
    cur: Pos,
    /// The selection anchor; `None` when there is no selection.
    anchor: Option<Pos>,
    /// The mark is set: motions extend the selection.
    marking: bool,
    /// The first visible row.
    top: Row,
    /// Horizontal scroll in `NoWrap` mode.
    xoff: usize,
    geom: Geometry,
    start: Start,
    matcher: Matcher,
    /// The last search was backward, and so n goes up and N down.
    backward: bool,
    highlight: bool,
    laid: LaidCache,
    /// A line's chars, decoded into it rather than allocated per call.
    scratch: Vec<char>,
}

impl TextView {
    /// A view of `buf` as read so far, on the screen `geom` describes;
    /// `appended` places it on the text.
    pub fn new(buf: Buffer, start: Start, geom: Geometry) -> TextView {
        TextView {
            buf,
            cur: Pos::default(),
            anchor: None,
            marking: false,
            // The first row: line 0's separator, if it has one.
            top: Row::sep(0),
            xoff: 0,
            geom,
            start,
            matcher: Matcher::new(""),
            backward: false,
            highlight: false,
            laid: LaidCache::new(geom.layout()),
            scratch: Vec::new(),
        }
    }

    pub fn buf(&self) -> &Buffer {
        &self.buf
    }

    pub fn cursor(&self) -> Pos {
        self.cur
    }

    pub fn set_cursor(&mut self, p: Pos) {
        self.cur = p;
    }

    pub fn xoff(&self) -> usize {
        self.xoff
    }

    pub fn geometry(&self) -> Geometry {
        self.geom
    }

    /// The rows the text has.
    pub fn rows(&self) -> usize {
        self.geom.rows
    }

    pub fn page_rows(&self) -> usize {
        self.geom.rows.max(1)
    }

    pub fn half_page_rows(&self) -> usize {
        (self.geom.rows / 2).max(1)
    }

    /// The first visible row, as a row start: the text or the layout
    /// may have changed under it.
    pub fn top(&mut self) -> Row {
        self.top = self.snap(self.top);
        self.top
    }

    /// Lays the view out for `geom`. The horizontal scroll starts over
    /// with the mode; a cursor on screen before stays on screen, one
    /// scrolled off stays off.
    pub fn resize(&mut self, geom: Geometry) {
        if geom == self.geom {
            return;
        }
        let shown = self.cursor_visible();
        if geom.mode != self.geom.mode {
            self.xoff = 0;
        }
        self.geom = geom;
        if shown {
            self.scroll_to_cursor();
        }
    }

    /// Takes the reader's progress; `appended` places the view once a
    /// burst of it is in.
    pub fn apply(&mut self, p: Progress) {
        self.buf.apply(p);
    }

    /// Runs after the reader appended lines: applies a pending +N,
    /// follows for +G and applies a `StartScreen` at EOF. New text is
    /// not a motion: unless one of those moved the cursor, a view
    /// scrolled sideways off it stays put.
    pub fn appended(&mut self) {
        let eof = self.buf.finished().is_some();
        let n = self.buf.len();
        let (cur, xoff) = (self.cur, self.xoff);
        let shown = self.cursor_visible();
        if let Some(line) = self.start.line
            && (n > line || eof)
        {
            self.cur = Pos {
                line: line.min(n.saturating_sub(1)),
                col: 0,
            };
            self.top = self.with_sep(self.cur);
            self.start.line = None;
        }
        if self.start.follow {
            self.go_last();
        }
        if let Some(s) = self.start.screen
            && eof
        {
            self.show_screen(s);
            self.start.screen = None;
        }
        if shown || self.cur != cur {
            self.scroll_to_cursor();
        }
        if self.cur == cur && self.geom.mode == Mode::NoWrap {
            self.xoff = xoff;
        }
    }

    /// The user placed the view: startup positioning stops.
    pub fn end_startup(&mut self) {
        self.start = Start::default();
    }

    /// Whether a `StartScreen` still waits for EOF: the view is not
    /// known until then.
    pub fn start_pending(&self) -> bool {
        self.start.screen.is_some()
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
        let rows = self.geom.rows;
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
        let steps =
            (s.cursor_row as isize - (self.geom.height - rows) as isize).min(rows as isize) - 1;
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

    /// Whether the cursor's row is among the text rows on screen; with
    /// no rows or no text, nothing is scrolled away from.
    pub fn cursor_visible(&mut self) -> bool {
        let rows = self.geom.rows;
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
            if bottom >= crow {
                break;
            }
            bottom = self.next_row(bottom);
        }
        if bottom < crow {
            return false;
        }
        let l = self.geom.layout();
        if l.mode != Mode::NoWrap || l.width == 0 {
            return true;
        }
        // As drawn: the markers take a cell only where there is text
        // beyond them, and hide a cursor under them; with no > a glyph
        // cut off at the right edge shows the cursor on its first cell.
        let (x, w, width) = self.cursor_span();
        let mark = usize::from(self.geom.edge_markers);
        let left = if self.xoff > 0 { mark } else { 0 };
        let right = if width > self.xoff + l.width { mark } else { 0 };
        if x < self.xoff + left {
            return false;
        }
        if right == 0 {
            return x < self.xoff + l.width;
        }
        x + w <= self.xoff + l.width - right
    }

    /// Moves `top` and `xoff` the least that brings the cursor on
    /// screen.
    pub fn scroll_to_cursor(&mut self) {
        let rows = self.geom.rows;
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
        let l = self.geom.layout();
        if l.mode != Mode::NoWrap || l.width == 0 {
            self.xoff = 0;
            return;
        }
        // The cursor's whole glyph must fit, clear of the < and > that
        // mark text off either side.
        let (x, w, width) = self.cursor_span();
        let mark = usize::from(self.geom.edge_markers);
        let right = |xoff: usize| if width > xoff + l.width { mark } else { 0 };
        if x < self.xoff + mark {
            self.xoff = x.saturating_sub(mark);
        } else if x + w > self.xoff + l.width - right(self.xoff) {
            let xoff = (x + w).saturating_sub(l.width);
            let xoff = xoff + right(xoff);
            self.xoff = x.saturating_sub(mark).min(xoff);
        }
    }

    /// Makes the cursor's row the top one if it is off the screen,
    /// stopping at the last screenful.
    fn cursor_row_to_top(&mut self) {
        let rows = self.geom.rows;
        if rows == 0 || self.buf.is_empty() {
            return;
        }
        let crow = self.snap(Row::text(self.cur));
        let top = self.snap(self.top);
        if crow >= top {
            let mut bottom = top;
            for _ in 0..rows - 1 {
                if bottom >= crow {
                    break;
                }
                bottom = self.next_row(bottom);
            }
            if bottom >= crow {
                return;
            }
        }
        self.place_top(crow);
    }

    /// Decodes the text as raw or as colored. The lines' text changes
    /// under the view, which keeps its place as over a reload; laid out
    /// anew, a cursor on screen stays on screen.
    pub fn set_literal(&mut self, raw: bool) {
        if self.buf.literal() == raw {
            return;
        }
        let shown = self.cursor_visible();
        self.change_text(|b| b.set_literal(raw));
        if shown {
            self.scroll_to_cursor();
        }
    }

    /// Changes the text under the view and keeps the view's place: a
    /// cursor that was on the last line goes to the new last line, and
    /// every position is clamped, the line to the last and the column
    /// to the line's end. The cursor and the anchor each stay in the
    /// input they were in: a file above that shrank would otherwise
    /// hand them to the next. A top past the new end starts over from
    /// the first row, so a text shorter than the screen fills it from
    /// the top; a separator top stays one, which `snap` drops if the
    /// line no longer starts an input. The view scrolls only to a
    /// cursor that moved: new text is not a motion.
    pub fn change_text(&mut self, f: impl FnOnce(&mut Buffer)) {
        let place = Place::of(&self.buf, self.cur, self.anchor);
        f(&mut self.buf);
        let was = self.cur;
        let n = self.buf.len();
        let (buf, scratch) = (&self.buf, &mut self.scratch);
        let clamp = |p: Pos, scratch: &mut Vec<char>| {
            if n == 0 {
                return Pos::default();
            }
            let line = p.line.min(n - 1);
            buf.text(line, scratch);
            Pos {
                line,
                col: p.col.min(scratch.len()),
            }
        };
        // `line` within input `part`; an input that emptied yields to the
        // next.
        let within = |part: usize, line: usize| {
            let parts = buf.parts();
            let Some(&start) = parts.get(part).filter(|_| n > 0) else {
                return line;
            };
            let lo = start.min(n - 1);
            let hi = parts.get(part + 1).map_or(n, |&e| e.max(lo + 1)).min(n) - 1;
            line.clamp(lo, hi)
        };
        let line = if place.at_end {
            n.saturating_sub(1)
        } else {
            self.cur.line
        };
        self.cur = clamp(
            Pos {
                line: within(place.cur, line),
                col: self.cur.col,
            },
            scratch,
        );
        if self.top.line >= n {
            self.top = Row::sep(0);
        } else if let Some(col) = self.top.col {
            self.top = Row::text(clamp(
                Pos {
                    line: self.top.line,
                    col,
                },
                scratch,
            ));
        }
        if let (Some(a), Some(part)) = (&mut self.anchor, place.anchor) {
            a.line = within(part, a.line);
            *a = clamp(*a, scratch);
        }
        self.laid.clear();
        if self.cur != was {
            self.scroll_to_cursor();
        }
    }
}

/// Where a view was before its text changed.
#[derive(Copy, Clone, Debug)]
struct Place {
    /// The cursor was on the last line.
    at_end: bool,
    /// The cursor's input, and the anchor's.
    cur: usize,
    anchor: Option<usize>,
}

impl Place {
    fn of(buf: &Buffer, cur: Pos, anchor: Option<Pos>) -> Place {
        Place {
            at_end: cur.line + 1 >= buf.len(),
            cur: buf.part_at(cur.line),
            anchor: anchor.map(|a| buf.part_at(a.line)),
        }
    }
}
