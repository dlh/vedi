//! Button 1 places the cursor and drags the selection, with Shift
//! extends it; the wheel scrolls, sideways too, the bindings as well.

use std::time::{Duration, Instant};

use crate::app::event::{Mouse, MouseKind};
use crate::app::text_view::Motion;
use crate::app::views::Role;
use crate::app::{App, Focus, Kind};
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

/// Where button 1 is.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
enum Button {
    #[default]
    Up,
    /// Down, but off the text or its drag over: motion does nothing
    /// until the release.
    Held,
    /// Down on the text: motion selects.
    Dragging,
}

/// What a press or motion with button 1 down is to the text.
pub(super) enum Down {
    /// The press: button 1 was up.
    Press,
    /// Motion of a drag.
    Drag,
    /// Motion that selects nothing.
    Held,
}

/// What a press on the text does.
pub(super) enum Press {
    /// Extends the selection to the cell.
    Extend,
    /// Places the cursor; the nth click in a row on the cell, so 2 is a
    /// double-click.
    Click(usize),
}

/// Button 1's bookkeeping: which click of a multiple click a press is,
/// whether motion drags, and when a drag held at an edge next scrolls.
/// It is told the time and the screen; it reads neither.
#[derive(Default)]
pub(super) struct State {
    last_press: Option<Click>,
    button: Button,
    /// The cell the drag last reached.
    drag_x: usize,
    drag_y: usize,
    /// When the auto-scroll tick a drag at an edge armed is due.
    ticking: Option<Instant>,
}

impl State {
    /// Button 1 is down on a cell, pressed or moved there.
    pub fn down(&mut self) -> Down {
        match self.button {
            Button::Up => {
                self.button = Button::Held;
                Down::Press
            }
            Button::Held => Down::Held,
            Button::Dragging => Down::Drag,
        }
    }

    /// Button 1 pressed the text at `pos`, on (`x`, `y`) at `now`:
    /// motion now drags from there. With `shift` the press extends the
    /// selection and counts toward no double click; else it is the
    /// next click on the cell within `DOUBLE_CLICK` of the last, or the
    /// first.
    pub fn press(&mut self, x: usize, y: usize, pos: Pos, now: Instant, shift: bool) -> Press {
        let n = match self.last_press {
            _ if shift => 0,
            Some(l) if (l.x, l.y) == (x, y) && now.duration_since(l.at) <= DOUBLE_CLICK => l.n + 1,
            _ => 1,
        };
        self.last_press = Some(Click {
            at: now,
            x,
            y,
            pos,
            n,
        });
        self.button = Button::Dragging;
        self.ticking = None;
        if shift {
            Press::Extend
        } else {
            Press::Click(n)
        }
    }

    /// Button 1 is up: nothing drags until the next press, and no tick
    /// is due.
    pub fn release(&mut self) {
        self.button = Button::Up;
        self.ticking = None;
    }

    /// The drag is over, though the button may still be down: motion
    /// until the release does nothing.
    pub fn end_drag(&mut self) {
        if self.button == Button::Dragging {
            self.button = Button::Held;
        }
    }

    /// The drag reached (`x`, `y`) at `now` on a screen `h` rows high.
    /// On the top or bottom row it arms a tick `AUTO_SCROLL_TICK` ahead
    /// unless one is armed. Returns the text pressed, to anchor at when
    /// nothing is selected yet: the press may have moved the rows.
    pub fn drag(&mut self, x: usize, y: usize, h: usize, now: Instant) -> Option<Pos> {
        (self.drag_x, self.drag_y) = (x, y);
        if self.edge(h) != 0 && self.ticking.is_none() {
            self.ticking = Some(now + AUTO_SCROLL_TICK);
        }
        self.last_press.map(|l| l.pos)
    }

    /// The rows to scroll at `now` for a drag still held at an edge of
    /// a screen `h` rows high, and the cell to drag to again, which
    /// arms the next tick. None before the armed tick is due, or when
    /// none is: that tick was the redraw hold's or an earlier drag's.
    pub fn due(&mut self, h: usize, now: Instant) -> Option<(isize, usize, usize)> {
        if now < self.ticking? {
            return None;
        }
        self.ticking = None;
        match self.edge(h) {
            0 => None,
            d => Some((d, self.drag_x, self.drag_y)),
        }
    }

    /// When the armed tick is due.
    #[inline]
    pub fn deadline(&self) -> Option<Instant> {
        self.ticking
    }

    /// The direction a drag is scrolling on a screen `h` rows high: -1
    /// on the top row, 1 on the bottom row, the status line's or not,
    /// 0 inside the text or not dragging.
    fn edge(&self, h: usize) -> isize {
        if self.button != Button::Dragging {
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
    /// the prompts, but for the wheel at the & prompt, which scrolls the
    /// list; a press from before one is forgotten.
    pub(super) fn handle_mouse(&mut self, m: Mouse) {
        if self.focus != Focus::Viewing {
            self.mouse.release();
            let wheel = matches!(m.kind, MouseKind::Wheel { .. });
            if !(wheel && self.focus == Focus::Prompting(Kind::Filter)) {
                return;
            }
        }
        match m.kind {
            MouseKind::Wheel { rows, cols } => {
                let (rows, cols) = wheel(rows, cols, m.shift);
                if rows == 0 && cols == 0 {
                    return;
                }
                self.act();
                if rows != 0 {
                    self.views.shown.scroll(rows);
                } else {
                    self.views.shown.scroll_sideways(cols);
                }
            }
            MouseKind::Release => self.mouse.release(),
            // Motion with the button down looks like a press.
            MouseKind::Press | MouseKind::Drag => match self.mouse.down() {
                Down::Press => self.press(m.x, m.y, m.shift),
                Down::Drag => self.drag(m.x, m.y),
                Down::Held => {}
            },
        }
    }

    /// Returns from help, ignores the status line, and otherwise puts
    /// the cursor on the cell; a second press there within
    /// `DOUBLE_CLICK` selects the word, a third the line. With shift the
    /// selection extends to the cell instead, from the cursor when there
    /// is none.
    fn press(&mut self, x: usize, y: usize, shift: bool) {
        // Measured before act: clearing a message gives a hidden status
        // line's row back to the text, and the click was on the message.
        let rows = self.views.shown.rows();
        self.act();
        if self.views.role() == Role::Help {
            self.hide_help();
            return;
        }
        if y >= rows {
            return;
        }
        let now = (self.clock)();
        let v = &mut self.views.shown;
        let pos = v.cell_pos(x, y);
        let n = match self.mouse.press(x, y, pos, now, shift) {
            Press::Extend => {
                v.go(Motion::To(pos), true);
                return;
            }
            Press::Click(n) => n,
        };
        v.set_cursor(pos);
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
    /// clamped to the text rows. On the top row or the status row the
    /// view scrolls a row per tick while the button stays held.
    fn drag(&mut self, x: usize, y: usize) {
        let (_, h) = self.screen.size();
        let anchor = self.mouse.drag(x, y, h, (self.clock)());
        let v = &mut self.views.shown;
        if !v.has_selection()
            && let Some(a) = anchor
        {
            v.set_cursor(a);
            v.extend();
        }
        let pos = v.cell_pos(x, y.min(v.rows().saturating_sub(1)));
        v.go(Motion::To(pos), true);
    }

    /// Scrolls a row for a drag still held at an edge and extends the
    /// selection to the row under the pointer, which arms the next tick.
    pub(super) fn tick(&mut self) {
        let (_, h) = self.screen.size();
        if let Some((rows, x, y)) = self.mouse.due(h, (self.clock)()) {
            self.act();
            self.views.shown.scroll(rows);
            self.drag(x, y);
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
