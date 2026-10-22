use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::input::keymap::Keymap;
use crate::input::setting::Settings;
use crate::sys::opencmd::Killer;
use crate::text::buffer::Buffer;
use crate::text::reader::Gen;

/// The name of input read from stdin, which a window title in the
/// input renames.
pub const STDIN: &str = "<stdin>";

/// The clock double-clicks and the redraw hold are timed by.
pub type Clock = Box<dyn FnMut() -> Instant>;

/// A copy's outcome; the error is what the status line shows.
pub type CopyResult = Result<(), String>;

/// Takes a copy's result, on whatever thread it comes in.
pub type Done = Box<dyn FnOnce(CopyResult) + Send>;

/// Sends text to the clipboard and hands `done` the result: at once,
/// or from a thread when the sending takes time, and then the
/// command's killer, so a copy superseded can be ended.
pub trait Copier {
    fn copy(&mut self, text: &str, done: Done) -> Option<Killer>;
}

/// Wakes the loop once a copy's result is in: `Event::Copied` on its
/// sender.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// Reads the input again for a reload, through open_cmd or not: an
/// empty buffer whose reader, started, sends `Event::Progress` for the
/// generation given on the loop's sender, which the opener holds.
pub type Opener = Box<dyn FnMut(Gen, bool) -> io::Result<Buffer>>;

/// Reads a file through a command: its argv by file name, %s the
/// name; `None` to read it as it is.
pub type OpenCmdFor = Box<dyn Fn(&str) -> Option<Vec<String>>>;

/// Where :write goes: written, then `done`, which puts the file in
/// place. One dropped before then leaves no file.
pub trait Sink: Write {
    fn done(self: Box<Self>) -> io::Result<()>;
}

/// Opens the file :write saves to, by name: made new, or replaced once
/// done when `overwrite`; `AlreadyExists` otherwise for one that
/// exists.
pub type Writer = Box<dyn FnMut(&Path, bool) -> io::Result<Box<dyn Sink>>>;

pub struct Options {
    /// What the status line calls each input, by its part in the buffer.
    pub names: Vec<String>,
    pub settings: Settings,
    /// 1-based line to put at the top.
    pub start_line: Option<usize>,
    /// Keep the cursor on the last line until EOF (+G).
    pub follow: bool,
    pub screen: Option<StartScreen>,
    pub copier: Box<dyn Copier>,
    pub wake: Wake,
    pub clock: Clock,
    /// There is a ⌘ key.
    pub macos: bool,
    pub keys: Keymap,
    /// `None` when the input cannot be read again, as a pipe cannot.
    pub open: Option<Opener>,
    /// The command each input was opened through, for the status line.
    pub open_cmd: Option<OpenCmdFor>,
    pub write: Writer,
    /// What a leading ~ in a :write name stands for; `None` when unknown.
    pub home: Option<PathBuf>,
}

/// The view the terminal was showing, so the pager can open on the
/// same rows: the last screenful, scrolled `scrolled_by` rows up, with
/// the cursor at the 1-based (`cursor_row`, `cursor_col`) of that
/// screenful. A zero `cursor_row` leaves the cursor at the top. The
/// status line takes one row, so the terminal's top row is dropped.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct StartScreen {
    pub scrolled_by: usize,
    pub cursor_row: usize,
    pub cursor_col: usize,
}
