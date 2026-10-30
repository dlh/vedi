//! The input: read again on the key or when a file changes on disk,
//! through open_cmd or as it is.

use std::fmt;
use std::io;

use crate::app::options::{OpenCmdFor, Opener};
use crate::app::{App, Flow};
use crate::text::ansi::ViewStyle;
use crate::text::buffer::{Buffer, Progress};
use crate::text::reader::Gen;
use crate::view::screen::Terminal;

/// The input's readers: the text's, and one reload's at a time. Each
/// has a generation, and progress from any other is dropped, so a
/// reload given up on never lands.
pub(super) struct Input {
    /// `None` when the input cannot be read again, as a pipe cannot.
    open: Option<Opener>,
    open_cmd: Option<OpenCmdFor>,
    /// Off, a reload reads the files as they are, not through open_cmd.
    through_open_cmd: bool,
    /// The text's reader's generation.
    generation: Gen,
    /// The last generation handed out: a dropped reload's is never
    /// reused, so its reader's data stays dropped.
    last_gen: Gen,
    /// A reload being read.
    next: Option<Pending>,
    /// A change came during the reload: one more after it.
    dirty: bool,
}

/// A reload being read, with its reader's generation.
struct Pending {
    generation: Gen,
    buf: Buffer,
    /// Once the text lands, the status line names the command the
    /// cursor's input is read through, unless a read error shows.
    names_open_cmd: bool,
}

/// Why a reload did not start.
pub(super) enum Refused {
    NotAFile,
    Open(io::Error),
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refused::NotAFile => f.write_str("not a file"),
            Refused::Open(e) => write!(f, "reload failed: {e}"),
        }
    }
}

/// Where a reader's progress went.
pub(super) enum Applied {
    /// The text's reader's, for the text's buffer.
    Text(Progress),
    /// A reload's buffer took it.
    Reload,
    /// A dropped reader's.
    Dropped,
}

impl Input {
    /// `generation` is the text's reader's.
    pub(super) fn new(
        open: Option<Opener>,
        open_cmd: Option<OpenCmdFor>,
        generation: Gen,
    ) -> Input {
        Input {
            open,
            open_cmd,
            through_open_cmd: true,
            generation,
            last_gen: generation,
            next: None,
            dirty: false,
        }
    }

    /// Reads the input again. One reload runs at a time; a change
    /// during one asks for another after it, which is not refused.
    pub(super) fn reload(&mut self) -> Result<(), Refused> {
        self.reload_then(false)
    }

    /// `reload`; `names_open_cmd` is for `Pending`.
    fn reload_then(&mut self, names_open_cmd: bool) -> Result<(), Refused> {
        let Some(open) = self.open.as_mut() else {
            return Err(Refused::NotAFile);
        };
        if self.next.is_some() {
            self.dirty = true;
            return Ok(());
        }
        let generation = self.last_gen.wrapping_add(1);
        self.last_gen = generation;
        let buf = open(generation, self.through_open_cmd).map_err(Refused::Open)?;
        self.next = Some(Pending {
            generation,
            buf,
            names_open_cmd,
        });
        Ok(())
    }

    /// Reads the files through open_cmd, or as they are. A reload
    /// still being read is dropped: it may be waiting on a command
    /// that never ends.
    pub(super) fn toggle_open_cmd(&mut self) -> Result<(), Refused> {
        if self.open.is_none() {
            return Err(Refused::NotAFile);
        }
        self.through_open_cmd = !self.through_open_cmd;
        self.next = None;
        self.dirty = false;
        self.reload_then(true)
    }

    /// Drops the reload a change asked for during the current one.
    pub(super) fn drop_queued(&mut self) {
        self.dirty = false;
    }

    /// The reload's buffer, while one is being read.
    #[inline]
    pub(super) fn reloading(&self) -> Option<&Buffer> {
        self.next.as_ref().map(|p| &p.buf)
    }

    /// Gives a reader's progress to its buffer: a reload's here, the
    /// text's back for the text's.
    #[inline]
    pub(super) fn apply(&mut self, g: Gen, p: Progress) -> Applied {
        if g == self.generation {
            return Applied::Text(p);
        }
        match &mut self.next {
            Some(n) if n.generation == g => {
                n.buf.apply(p);
                Applied::Reload
            }
            _ => Applied::Dropped,
        }
    }

    /// Takes a finished reload for the text's place, with whether the
    /// status line names open_cmd's command once it is there, and
    /// starts the reload a change asked for meanwhile.
    pub(super) fn take_finished(&mut self) -> Option<(Buffer, bool)> {
        let p = self.next.take_if(|p| p.buf.finished().is_some())?;
        self.generation = p.generation;
        if self.dirty {
            self.dirty = false;
            // Not by key, so a refusal has no one to tell.
            let _ = self.reload();
        }
        Some((p.buf, p.names_open_cmd))
    }

    /// The command the file `name` is read through; `None` as it is.
    pub(super) fn command_for(&self, name: &str) -> Option<Vec<String>> {
        match &self.open_cmd {
            Some(f) if self.through_open_cmd => f(name),
            _ => None,
        }
    }
}

impl<T: Terminal> App<T> {
    /// Reads the input again. Only the key says that it cannot.
    pub(super) fn reload(&mut self, by_key: bool) -> Flow {
        let started = self.input.reload();
        self.started(started, by_key)
    }

    /// Reads the files through open_cmd, or as they are. The selection
    /// goes once the mode has turned: the lines it covered are not the
    /// text the next reload brings, even when this one fails to open.
    pub(super) fn toggle_open_cmd(&mut self) -> Flow {
        let started = self.input.toggle_open_cmd();
        if !matches!(started, Err(Refused::NotAFile)) {
            self.views.shown.deselect();
        }
        self.started(started, true)
    }

    fn started(&mut self, started: Result<(), Refused>, by_key: bool) -> Flow {
        match started {
            Ok(()) => self.swap_if_done(),
            Err(e) if by_key => Flow::Say(e.to_string()),
            Err(_) => Flow::Continue,
        }
    }

    /// Puts a finished reload in the text's place.
    pub(super) fn swap_if_done(&mut self) -> Flow {
        let Some((buf, names_open_cmd)) = self.input.take_finished() else {
            return Flow::Continue;
        };
        self.put(buf);
        let b = self.views.text().buf();
        let failed = matches!(b.finished(), Some(Err(_))) || b.read_error().is_some();
        if names_open_cmd && !failed {
            Flow::Say(self.open_cmd_status())
        } else {
            Flow::Continue
        }
    }

    /// Puts `buf`, decoded as the view style has it, under the text's
    /// view, shown or set aside, which keeps its place; a filter's
    /// list is made again from it. The old buffer's files close with
    /// it.
    fn put(&mut self, mut buf: Buffer) {
        buf.set_literal(self.settings.view_style == ViewStyle::Raw);
        self.views.text_mut().change_text(|b| *b = buf);
        self.refilter();
    }
}
