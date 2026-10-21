//! What the loop is told: keys, the mouse, a resize, the reader's
//! progress, a file change, and the timer.

use crate::input::key::Key;
use crate::text::buffer::Progress;
use crate::text::reader::Gen;

pub enum Event {
    Key(Key),
    Mouse(Mouse),
    /// Columns and rows; the terminal already has them.
    Resize(usize, usize),
    /// From the reader of generation `Gen`; a stale one is dropped.
    Progress(Gen, Progress),
    /// The file changed on disk.
    Changed,
    /// A deadline passed: the redraw hold, or the auto-scroll tick.
    Tick,
    /// A copy's result is in.
    Copied,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Mouse {
    pub kind: MouseKind,
    pub x: usize,
    pub y: usize,
    pub shift: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MouseKind {
    Press,
    Release,
    Drag,
    /// Ticks down and right; negative is up and left.
    Wheel {
        rows: i32,
        cols: i32,
    },
}
