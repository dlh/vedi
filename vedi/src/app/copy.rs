use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use crate::app::{App, CopyResult, Flow};
use crate::sys::proc::Killer;
use crate::text::ansi::ViewStyle;
use crate::view::screen::Terminal;

/// A copy under way.
pub(super) struct Copying {
    rx: Receiver<CopyResult>,
    /// Lines selected, for the status line.
    lines: usize,
    report: Report,
    killer: Option<Killer>,
}

/// What of a copy the status line says.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(super) enum Report {
    /// How much went, or why it did not.
    All,
    /// Why it did not, only: with the status line hidden a message
    /// takes a row from the text, and one the mouse brought would move
    /// the text under the next click of a double or triple click.
    Failure,
}

impl<T: Terminal> App<T> {
    /// Starts sending the selection to the clipboard; with nothing
    /// selected, what to say. The selection stays either way.
    fn start_copy(&mut self, report: Report) -> Result<Copying, Flow> {
        let Some(sel) = self.views.shown.selection() else {
            return Err(Flow::Say("nothing selected".into()));
        };
        let mut out = Vec::new();
        sel.shown(self.views.shown.buf(), ViewStyle::Raw, &mut out);
        let text = String::from_utf8(out).expect("chars encoded as UTF-8");
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
        Ok(Copying {
            rx,
            lines,
            report,
            killer,
        })
    }

    /// Sends the selection to the clipboard and says how it went, as
    /// far as `report` has it said. A copier that takes time reports on
    /// `Event::Copied`; a copy started before then is ended.
    pub(super) fn copy(&mut self, report: Report) -> Flow {
        let c = match self.start_copy(report) {
            Ok(c) => c,
            Err(flow) => return flow,
        };
        match c.rx.try_recv() {
            Ok(res) => said(&c, res),
            Err(_) => {
                let flow = match c.report {
                    Report::All => Flow::Say("copying".into()),
                    Report::Failure => Flow::Continue,
                };
                self.copying = Some(c);
                flow
            }
        }
    }

    /// Sends the selection to the clipboard and waits for the result:
    /// quits on success, else says what went wrong.
    pub(super) fn copy_and_wait(&mut self) -> Flow {
        let c = match self.start_copy(Report::All) {
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
        let c = self.copying.take().expect("checked");
        said(&c, res)
    }

    /// A copy's result is still to come.
    pub fn copying(&self) -> bool {
        self.copying.is_some()
    }
}

/// The flow a finished copy's result makes under its report.
fn said(c: &Copying, res: CopyResult) -> Flow {
    match copied(c.lines, res) {
        (_, true) if c.report == Report::Failure => Flow::Continue,
        (msg, _) => Flow::Say(msg),
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
