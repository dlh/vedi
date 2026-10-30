//! A stretch of the text: which columns of each line it holds, how
//! many lines a copy says, and its chars as the view shows them.

use std::ops::Range;

use crate::text::ansi::{Link, ViewStyle};
use crate::text::buffer::{Buffer, Pos};
use crate::view::render::{osc8, sgr};
use crate::view::style::Style;

/// A stretch of the text: `start` to `end`, each within its line, and
/// the last line's newline when `newline`, the end being the end row.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Selection {
    pub start: Pos,
    pub end: Pos,
    pub newline: bool,
}

impl Selection {
    /// How many lines it holds, the count a copy says: one that ends at
    /// a line's start leaves that line out.
    pub fn lines(&self) -> usize {
        let short = self.end.col == 0 && !self.newline && self.end.line > self.start.line;
        self.end.line - self.start.line + 1 - usize::from(short)
    }

    /// The columns it holds of line `line`, `n` chars long: `n` is the
    /// line's newline, held when the range ends past it.
    pub fn cols(&self, line: usize, n: usize) -> Range<usize> {
        if line < self.start.line || line > self.end.line {
            return 0..0;
        }
        let from = if line == self.start.line {
            self.start.col.min(n)
        } else {
            0
        };
        let to = if line == self.end.line {
            self.end.col.min(n) + usize::from(self.newline)
        } else {
            n + 1
        };
        from..to
    }

    /// Appends the chars it holds of `buf`, lines joined with newlines,
    /// as the view shows them: under `Color` each run's style as SGR
    /// and its link as OSC 8, the style a line carries in from before
    /// included; under `Plain` the links alone; under `Raw` the text,
    /// escapes and all. Ends on no style or link, then the last newline
    /// when it holds it. A line that cannot be read is empty, as on
    /// screen; `buf` keeps the error.
    pub fn shown(&self, buf: &Buffer, view: ViewStyle, out: &mut Vec<u8>) {
        let (mut style, mut link): (Style, Option<Link>) = Default::default();
        let chars = |out: &mut Vec<u8>, text: &[char]| {
            for c in text {
                out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
            }
        };
        for li in self.start.line..=self.end.line {
            let l = buf.line(li);
            let n = l.text.len();
            let cols = self.cols(li, n);
            let (from, to) = (cols.start, cols.end.min(n));
            if li > self.start.line {
                out.push(b'\n');
            }
            if view == ViewStyle::Raw {
                chars(out, &l.text[from..to]);
                continue;
            }
            for run in &l.runs {
                let (s, e) = (run.start.max(from), run.end.min(to));
                if s >= e {
                    continue;
                }
                if view == ViewStyle::Color && run.style != style {
                    sgr(out, run.style);
                    style = run.style;
                }
                if let Some(l) = osc8(out, link.as_ref(), run.link.as_ref()) {
                    link = l;
                }
                chars(out, &l.text[s..e]);
            }
        }
        if link.is_some() {
            out.extend_from_slice(b"\x1b]8;;\x1b\\");
        }
        if style != Style::default() {
            out.extend_from_slice(b"\x1b[0m");
        }
        if self.newline {
            out.push(b'\n');
        }
    }
}
