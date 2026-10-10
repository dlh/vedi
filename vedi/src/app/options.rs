use std::io;
use std::time::Instant;

use crate::input::keymap::Keymap;
use crate::text::ansi::ViewStyle;
use crate::text::buffer::Buffer;
use crate::text::reader::Gen;
use crate::view::layout::{Mode, WrapStyle};

/// The name of input read from stdin, which a window title in the
/// input renames.
pub const STDIN: &str = "<stdin>";

/// The clock double-clicks and the redraw hold are timed by.
pub type Clock = Box<dyn FnMut() -> Instant>;

/// Sends text to the clipboard; the error is what the status line shows.
pub trait Copier {
    fn copy(&mut self, text: &str) -> Result<(), String>;
}

/// Reads the input again for a reload: an empty buffer whose reader,
/// started, sends `Event::Progress` for the generation given on the
/// loop's sender, which the opener holds.
pub type Opener = Box<dyn FnMut(Gen) -> io::Result<Buffer>>;

pub struct Options {
    /// What the status line calls each input, by its part in the buffer.
    pub names: Vec<String>,
    pub mode: Mode,
    pub wrap_style: WrapStyle,
    /// 1-based line to put at the top.
    pub start_line: Option<usize>,
    /// Keep the cursor on the last line until EOF (+G).
    pub follow: bool,
    pub screen: Option<StartScreen>,
    pub copier: Box<dyn Copier>,
    pub clock: Clock,
    /// There is a ⌘ key.
    pub macos: bool,
    pub keys: Keymap,
    /// Cells per tab stop; 0 for 8.
    pub tab_width: usize,
    /// Mark text off the sides with < and >, a wrapped row with \.
    pub edge_markers: bool,
    /// A row names each input, when there are several.
    pub file_separators: bool,
    /// A `Changed` event reads the file again.
    pub auto_reload: bool,
    /// The status line shows only for a prompt, a message, help or a
    /// read error.
    pub hide_status: bool,
    pub view_style: ViewStyle,
    /// `None` when the input cannot be read again, as a pipe cannot.
    pub open: Option<Opener>,
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
