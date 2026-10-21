use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use crate::app::{App, CopyResult};
use crate::sys::opencmd::Killer;
use crate::text::buffer::Pos;
use crate::view::screen::Terminal;

/// A copy under way.
pub(super) struct Copying {
    rx: Receiver<CopyResult>,
    /// Lines selected, for the status line.
    lines: usize,
    killer: Option<Killer>,
}

impl<T: Terminal> App<T> {
    /// The selected chars, lines joined with newlines.
    fn selected_text(&mut self, start: Pos, end: Pos) -> String {
        let mut s = String::new();
        for li in start.line..=end.line {
            self.view.buf().text(li, &mut self.scratch);
            let n = self.scratch.len();
            let from = if li == start.line {
                start.col.min(n)
            } else {
                0
            };
            let to = if li == end.line { end.col.min(n) } else { n };
            if li > start.line {
                s.push('\n');
            }
            s.extend(&self.scratch[from..to]);
        }
        s
    }

    /// Starts sending the selection to the clipboard; `None` with
    /// nothing selected. The selection stays either way.
    fn start_copy(&mut self) -> Option<Copying> {
        let Some((start, end)) = self.view.selection() else {
            self.status = Some("nothing selected".into());
            return None;
        };
        let text = self.selected_text(start, end);
        let mut lines = end.line - start.line + 1;
        if end.col == 0 && end.line > start.line {
            lines -= 1;
        }
        // A copy still under way would land after this one, maybe last.
        if let Some(k) = self.copying.take().and_then(|c| c.killer) {
            k.kill();
        }
        let (tx, rx) = mpsc::channel();
        let wake = Arc::clone(&self.wake);
        let killer = self.copier.copy(
            &text,
            Box::new(move |res| {
                let _ = tx.send(res);
                wake();
            }),
        );
        Some(Copying { rx, lines, killer })
    }

    /// Puts a copy's result in the status line and reports success.
    fn copied(&mut self, lines: usize, res: CopyResult) -> bool {
        let (msg, ok) = match res {
            Ok(()) if lines == 1 => ("copied 1 line".into(), true),
            Ok(()) => (format!("copied {lines} lines"), true),
            Err(e) => (format!("copy failed: {e}"), false),
        };
        self.status = Some(msg);
        ok
    }

    /// Sends the selection to the clipboard. A copier that takes time
    /// reports on `Event::Copied`; a copy started before then is
    /// ended.
    pub(super) fn copy(&mut self) {
        let Some(c) = self.start_copy() else {
            return;
        };
        match c.rx.try_recv() {
            Ok(res) => {
                self.copied(c.lines, res);
            }
            Err(_) => {
                self.status = Some("copying".into());
                self.copying = Some(c);
            }
        }
    }

    /// Sends the selection to the clipboard and waits for the result,
    /// which says whether to quit on it.
    pub(super) fn copy_and_wait(&mut self) -> bool {
        let Some(c) = self.start_copy() else {
            return false;
        };
        let res = c.rx.recv().unwrap_or_else(|_| Err("no result".into()));
        self.copied(c.lines, res)
    }

    /// Takes up the copy's result once it is in.
    pub(super) fn copy_done(&mut self) {
        let Some(c) = &self.copying else {
            return;
        };
        let res = match c.rx.try_recv() {
            Ok(res) => res,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("no result".into()),
        };
        let lines = c.lines;
        self.copying = None;
        self.copied(lines, res);
    }

    /// A copy's result is still to come.
    pub fn copying(&self) -> bool {
        self.copying.is_some()
    }
}
