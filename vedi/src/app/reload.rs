//! Reading the input again: on the key, or when the file changes on
//! disk.

use crate::app::{App, Flow};
use crate::text::ansi::ViewStyle;
use crate::text::buffer::Buffer;
use crate::text::reader::Gen;
use crate::view::screen::Terminal;

/// A reload being read, with its reader's generation.
pub(super) struct Pending {
    pub(super) generation: Gen,
    pub(super) buf: Buffer,
    /// Once the text lands, the status line names the command the
    /// cursor's input is read through, unless a read error shows.
    pub(super) names_open_cmd: bool,
}

impl<T: Terminal> App<T> {
    /// Reads the input again. One reload runs at a time; a change
    /// during one asks for another after it. Only the key says that
    /// it cannot.
    pub(super) fn reload(&mut self, by_key: bool) -> Flow {
        self.reload_then(by_key, false)
    }

    /// `reload`; `names_open_cmd` is for `Pending`.
    pub(super) fn reload_then(&mut self, by_key: bool, names_open_cmd: bool) -> Flow {
        let Some(open) = self.open.as_mut() else {
            if by_key {
                return Flow::Say("not a file".into());
            }
            return Flow::Continue;
        };
        if self.next.is_some() {
            self.dirty = true;
            return Flow::Continue;
        }
        let generation = self.last_gen.wrapping_add(1);
        self.last_gen = generation;
        match open(generation, self.through_open_cmd) {
            Ok(buf) => {
                self.next = Some(Pending {
                    generation,
                    buf,
                    names_open_cmd,
                });
                self.swap_if_done()
            }
            Err(e) if by_key => Flow::Say(format!("reload failed: {e}")),
            Err(_) => Flow::Continue,
        }
    }

    /// Puts a finished reload in the text's place, and starts the
    /// reload a change asked for meanwhile.
    pub(super) fn swap_if_done(&mut self) -> Flow {
        let Some(p) = self.next.take_if(|p| p.buf.finished().is_some()) else {
            return Flow::Continue;
        };
        self.generation = p.generation;
        self.put(p.buf);
        let b = self.text_view().buf();
        let failed = matches!(b.finished(), Some(Err(_))) || b.read_error().is_some();
        let flow = if p.names_open_cmd && !failed {
            Flow::Say(self.open_cmd_status())
        } else {
            Flow::Continue
        };
        if self.dirty {
            self.dirty = false;
            // Not by key, so it has nothing to say.
            self.reload(false);
        }
        flow
    }

    /// Puts `buf`, decoded as the view style has it, under the text's
    /// view, shown or set aside, which keeps its place; a filter's
    /// list is made again from it. The old buffer's files close with
    /// it.
    fn put(&mut self, mut buf: Buffer) {
        buf.set_literal(self.settings.view_style == ViewStyle::Raw);
        self.text_view_mut().change_text(|b| *b = buf);
        self.refilter();
    }
}
