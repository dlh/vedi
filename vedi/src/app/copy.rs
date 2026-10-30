use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use crate::app::text_view::Selection;
use crate::app::{App, CopyResult, Flow};
use crate::sys::opencmd::Killer;
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
    fn selected_text(&mut self, sel: Selection) -> String {
        let mut s = String::new();
        for li in sel.start.line..=sel.end.line {
            self.views.shown.buf().text(li, &mut self.scratch);
            let n = self.scratch.len();
            let cols = sel.cols(li, n);
            s.extend(&self.scratch[cols.start..cols.end.min(n)]);
            if cols.end > n {
                s.push('\n');
            }
        }
        s
    }

    /// Starts sending the selection to the clipboard; with nothing
    /// selected, what to say. The selection stays either way.
    fn start_copy(&mut self) -> Result<Copying, Flow> {
        let Some(sel) = self.views.shown.selection() else {
            return Err(Flow::Say("nothing selected".into()));
        };
        let text = self.selected_text(sel);
        let lines = sel.lines();
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
        Ok(Copying { rx, lines, killer })
    }

    /// Sends the selection to the clipboard and says how it went. A
    /// copier that takes time reports on `Event::Copied`; a copy
    /// started before then is ended.
    pub(super) fn copy(&mut self) -> Flow {
        let c = match self.start_copy() {
            Ok(c) => c,
            Err(flow) => return flow,
        };
        match c.rx.try_recv() {
            Ok(res) => Flow::Say(copied(c.lines, res).0),
            Err(_) => {
                self.copying = Some(c);
                Flow::Say("copying".into())
            }
        }
    }

    /// Sends the selection to the clipboard and waits for the result:
    /// quits on success, else says what went wrong.
    pub(super) fn copy_and_wait(&mut self) -> Flow {
        let c = match self.start_copy() {
            Ok(c) => c,
            Err(flow) => return flow,
        };
        let res = c.rx.recv().unwrap_or_else(|_| Err("no result".into()));
        match copied(c.lines, res) {
            (_, true) => Flow::Quit,
            (msg, false) => Flow::Say(msg),
        }
    }

    /// Takes up the copy's result once it is in, and says how it went.
    pub(super) fn copy_done(&mut self) -> Flow {
        let Some(c) = &self.copying else {
            return Flow::Continue;
        };
        let res = match c.rx.try_recv() {
            Ok(res) => res,
            Err(TryRecvError::Empty) => return Flow::Continue,
            Err(TryRecvError::Disconnected) => Err("no result".into()),
        };
        let lines = c.lines;
        self.copying = None;
        Flow::Say(copied(lines, res).0)
    }

    /// A copy's result is still to come.
    pub fn copying(&self) -> bool {
        self.copying.is_some()
    }
}

/// What the status line says of a copy's result, and whether it went.
fn copied(lines: usize, res: CopyResult) -> (String, bool) {
    match res {
        Ok(()) if lines == 1 => ("copied 1 line".into(), true),
        Ok(()) => (format!("copied {lines} lines"), true),
        Err(e) => (format!("copy failed: {e}"), false),
    }
}
