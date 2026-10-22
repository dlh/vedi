//! Button 1 places the cursor and drags the selection, with Shift
//! extends it; the wheel scrolls, sideways too, the bindings as well.

use std::time::{Duration, Instant};

use crate::app::event::{Mouse, MouseKind};
use crate::app::text_view::Motion;
use crate::app::{App, Focus};
use crate::text::buffer::Pos;
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
        if self.focus != Focus::Viewing {
            self.mouse.held = false;
            self.mouse.dragging = false;
            return;
        }
        let helping = self.help.is_some();
        match m.kind {
            MouseKind::Wheel { rows, cols } => {
                let (rows, cols) = wheel(rows, cols, m.shift);
                if rows == 0 && cols == 0 {
                    return;
                }
                self.act();
                match (rows, cols) {
                    (rows, _) if rows != 0 && helping => self.view.scroll_within(rows),
                    (_, cols) if helping => self.view.scroll_sideways_within(cols),
                    (rows, _) if rows != 0 => self.view.scroll(rows),
                    (_, cols) => self.view.scroll_sideways(cols),
                }
            }
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
        let rows = self.view.rows();
        self.act();
        if self.help.is_some() {
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
        let v = &mut self.view;
        if shift {
            let pos = v.cell_pos(x, y);
            self.mouse.last_press = Some(Click {
                at: now,
                x,
                y,
                pos,
                n: 0,
            });
            v.go(Motion::To(pos), true);
            return;
        }
        let pos = v.cell_pos(x, y);
        v.set_cursor(pos);
        self.mouse.last_press = Some(Click {
            at: now,
            x,
            y,
            pos,
            n,
        });
        v.deselect();
        match n {
            2 => v.select_word(),
            // The cursor lands after the line's newline, maybe below the
            // screen; scrolling it in would move the line under the
            // pointer.
            n if n >= 3 => {
                v.select_line();
                return;
            }
            _ => {}
        }
        v.scroll_to_cursor();
    }

    /// Selects from the pressed text to the cell under the mouse,
    /// clamped to the text rows. The anchor is the position pressed, not
    /// the cell: the press may have moved the rows. On the top row or
    /// the status row the view scrolls a row per tick while the button
    /// stays held.
    fn drag(&mut self, x: usize, y: usize) {
        let v = &mut self.view;
        if v.selection().is_none()
            && let Some(l) = self.mouse.last_press
        {
            v.set_cursor(l.pos);
            v.extend();
        }
        (self.mouse.drag_x, self.mouse.drag_y) = (x, y);
        let pos = v.cell_pos(x, y.min(v.rows().saturating_sub(1)));
        v.go(Motion::To(pos), true);
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
            self.act();
            self.view.scroll(d);
            self.drag(self.mouse.drag_x, self.mouse.drag_y);
        }
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
