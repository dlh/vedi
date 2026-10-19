use crate::app::App;
use crate::text::buffer::Pos;
use crate::view::screen::Terminal;

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

    /// Sends the selection to the clipboard and reports success. The
    /// selection stays either way.
    pub(super) fn copy(&mut self) -> bool {
        let Some((start, end)) = self.view.selection() else {
            self.status = Some("nothing selected".into());
            return false;
        };
        let text = self.selected_text(start, end);
        if let Err(e) = self.copier.copy(&text) {
            self.status = Some(format!("copy failed: {e}"));
            return false;
        }
        let mut n = end.line - start.line + 1;
        if end.col == 0 && end.line > start.line {
            n -= 1;
        }
        self.status = Some(if n == 1 {
            "copied 1 line".into()
        } else {
            format!("copied {n} lines")
        });
        true
    }
}
