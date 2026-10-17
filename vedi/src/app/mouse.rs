//! Button 1 places the cursor and drags the selection, with Shift
//! extends it; the wheel scrolls, sideways too, the bindings as well.

use std::time::{Duration, Instant};

use crate::app::event::{Mouse, MouseKind};
use crate::app::motion::is_word;
use crate::app::{App, Focus};
use crate::text::buffer::Pos;
use crate::view::layout::Mode;
use crate::view::screen::Terminal;

/// How soon a press on the same cell as the last one counts as the
/// next click of a double or triple click.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// How far one wheel tick scrolls the view.
const WHEEL_ROWS: isize = 3;
const WHEEL_COLS: isize = 4;

/// How often a drag held at an edge scrolls a row.
const AUTO_SCROLL_TICK: Duration = Duration::from_millis(50);

/// The last button-1 press, for multiple clicks and drags.
#[derive(Copy, Clone, Debug)]
struct Click {
    at: Instant,
    x: usize,
    y: usize,
    /// The text under the cell, as laid out at the press.
    pos: Pos,
    /// Presses in a row on this cell: 2 is a double-click.
    n: usize,
}

#[derive(Default)]
pub(super) struct State {
    last_press: Option<Click>,
    /// Button 1 is down.
    held: bool,
    /// And went down on the text, so motion selects.
    dragging: bool,
    /// The cell the drag last reached.
    drag_x: usize,
    drag_y: usize,
    /// When the auto-scroll tick a drag at an edge armed is due.
    pub ticking: Option<Instant>,
}

impl State {
    /// Button 1 is up: nothing drags until the next press.
    pub fn release(&mut self) {
        self.held = false;
        self.dragging = false;
        self.ticking = None;
    }

    /// The drag is over, though the button may still be down: motion
    /// until the release does nothing.
    pub fn end_drag(&mut self) {
        self.dragging = false;
    }

    /// Which click of a double or triple click a press at `now` on
    /// (`x`, `y`) is: one more than the last press's when it was on the
    /// same cell within `DOUBLE_CLICK`, else the first.
    fn click_count(&self, x: usize, y: usize, now: Instant) -> usize {
        match self.last_press {
            Some(l) if (l.x, l.y) == (x, y) && now.duration_since(l.at) <= DOUBLE_CLICK => l.n + 1,
            _ => 1,
        }
    }

    /// The direction a held drag is scrolling on a screen `h` rows
    /// high: -1 on the top row, 1 on the bottom row, the status line's
    /// or not, 0 inside the text or not dragging.
    fn edge(&self, h: usize) -> isize {
        if !self.held || !self.dragging {
            0
        } else if self.drag_y == 0 {
            -1
        } else if self.drag_y + 1 >= h {
            1
        } else {
            0
        }
    }
}

impl<T: Terminal> App<T> {
    /// A press returns from help like any key. The mouse is ignored at
    /// the / and : prompts, and a press from before one is forgotten.
    pub(super) fn handle_mouse(&mut self, m: Mouse) {
        if matches!(self.focus, Focus::Searching { .. } | Focus::Commanding) {
            self.mouse.held = false;
            self.mouse.dragging = false;
            return;
        }
        let helping = self.text.is_some();
        match m.kind {
            MouseKind::Wheel { rows, cols } => match wheel(rows, cols, m.shift) {
                (rows, _) if rows != 0 && helping => self.scroll_help(rows),
                (_, cols) if cols != 0 && helping => self.scroll_help_sideways(cols),
                (rows, _) if rows != 0 => self.scroll_view(rows),
                (_, cols) if cols != 0 => self.scroll_view_sideways(cols),
                _ => {}
            },
            MouseKind::Release => self.mouse.release(),
            // Motion with the button down looks like a press; only a
            // press on the text drags.
            MouseKind::Press | MouseKind::Drag if self.mouse.held => {
                if self.mouse.dragging {
                    self.drag(m.x, m.y);
                }
            }
            MouseKind::Press | MouseKind::Drag => {
                self.mouse.held = true;
                self.press(m.x, m.y, m.shift);
            }
        }
    }

    /// Returns from help, ignores the status line, and otherwise puts
    /// the cursor on the cell; a second press there within
    /// `DOUBLE_CLICK` selects the word, a third the line. With shift the
    /// selection extends to the cell instead, from the cursor when there
    /// is none, and the press counts toward no double click.
    fn press(&mut self, x: usize, y: usize, shift: bool) {
        // Measured before act: clearing a message gives a hidden status
        // line's row back to the text, and the click was on the message.
        let rows = self.text_rows();
        self.act();
        if self.text.is_some() {
            self.hide_help();
            return;
        }
        if y >= rows {
            return;
        }
        let now = (self.clock)();
        let n = self.mouse.click_count(x, y, now);
        self.mouse.dragging = true;
        self.mouse.ticking = None;
        if shift {
            self.anchor.get_or_insert(self.cur);
            self.cur = self.cell_pos(x, y);
            self.mouse.last_press = Some(Click {
                at: now,
                x,
                y,
                pos: self.cur,
                n: 0,
            });
            self.scroll_to_cursor();
            return;
        }
        self.cur = self.cell_pos(x, y);
        self.mouse.last_press = Some(Click {
            at: now,
            x,
            y,
            pos: self.cur,
            n,
        });
        self.anchor = None;
        self.marking = false;
        match n {
            2 => self.select_word(),
            // The cursor lands after the line's newline, maybe below the
            // screen; scrolling it in would move the line under the
            // pointer.
            n if n >= 3 => {
                self.select_line();
                return;
            }
            _ => {}
        }
        self.scroll_to_cursor();
    }

    /// Selects from the pressed text to the cell under the mouse,
    /// clamped to the text rows. The anchor is the position pressed, not
    /// the cell: the press may have moved the rows. On the top row or
    /// the status row the view scrolls a row per tick while the button
    /// stays held.
    fn drag(&mut self, x: usize, y: usize) {
        if self.anchor.is_none() {
            self.anchor = self.mouse.last_press.map(|l| l.pos);
        }
        (self.mouse.drag_x, self.mouse.drag_y) = (x, y);
        self.cur = self.cell_pos(x, y.min(self.text_rows().saturating_sub(1)));
        self.scroll_to_cursor();
        if self.edge() != 0 && self.mouse.ticking.is_none() {
            self.mouse.ticking = Some((self.clock)() + AUTO_SCROLL_TICK);
        }
    }

    fn edge(&self) -> isize {
        let (_, h) = self.screen.size();
        self.mouse.edge(h)
    }

    /// Scrolls a row for a drag still held at an edge and extends the
    /// selection to the row under the pointer; `drag` arms the next
    /// tick. A tick before the armed one is due, or when none is armed,
    /// is the redraw hold's or an earlier drag's.
    pub(super) fn tick(&mut self) {
        let Some(at) = self.mouse.ticking else {
            return;
        };
        if (self.clock)() < at {
            return;
        }
        self.mouse.ticking = None;
        let d = self.edge();
        if d != 0 {
            self.scroll_view(d);
            self.drag(self.mouse.drag_x, self.mouse.drag_y);
        }
    }

    /// Selects the word around the cursor, if it is on one.
    fn select_word(&mut self) {
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
    fn select_line(&mut self) {
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

    /// The position drawn at a screen cell: `y` rows down from `top`,
    /// stopping at the buffer's last row; the char whose cells hold `x`,
    /// or the newline past the row's end. A separator row is the line
    /// below, at the same column.
    fn cell_pos(&mut self, x: usize, y: usize) -> Pos {
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

    /// Moves the view `n` rows (negative is up), keeping the first line
    /// to the last row at the top. The cursor keeps its place, hidden
    /// while off screen: the next motion brings the view back to it.
    pub(super) fn scroll_view(&mut self, mut n: isize) {
        self.act();
        if self.text_rows() == 0 || self.buf.is_empty() {
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
    pub(super) fn scroll_view_sideways(&mut self, n: isize) {
        self.act();
        let l = self.layout();
        let rows = self.text_rows();
        if self.settings.mode != Mode::NoWrap || l.width == 0 || rows == 0 || self.buf.is_empty() {
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
}

/// The rows and columns a wheel event of `rows` and `cols` ticks
/// scrolls. Shift turns the wheel sideways: up is left, down right.
fn wheel(rows: i32, cols: i32, shift: bool) -> (isize, isize) {
    let (rows, cols) = (rows as isize * WHEEL_ROWS, cols as isize * WHEEL_COLS);
    if rows != 0 && shift {
        return (0, rows / WHEEL_ROWS * WHEEL_COLS);
    }
    (rows, cols)
}
