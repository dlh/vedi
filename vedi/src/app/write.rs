//! :write, which saves the text to a file as the view shows it.

use std::io::ErrorKind;
use std::path::PathBuf;

use crate::app::App;
use crate::text::ansi::{Link, ViewStyle};
use crate::text::buffer::{Buffer, Pos};
use crate::view::render::{osc8, sgr};
use crate::view::screen::Terminal;
use crate::view::style::Style;

impl<T: Terminal> App<T> {
    /// Writes the selection, or with none the whole text, to the file
    /// `name`, made new, or emptied first when `overwrite`; a file that
    /// exists is otherwise refused. A leading ~ is the home directory.
    /// The text goes as the view shows it: with its colors and links,
    /// with links alone, or as it came. The status line says how many
    /// lines went.
    pub(super) fn write(&mut self, name: &str, overwrite: bool) -> Result<(), String> {
        let path = match (&self.home, name.strip_prefix('~')) {
            (Some(home), Some(rest)) if rest.is_empty() || rest.starts_with('/') => {
                let mut p = home.clone().into_os_string();
                p.push(rest);
                PathBuf::from(p)
            }
            _ => PathBuf::from(name),
        };
        let mut w = match (self.write)(&path, overwrite) {
            Ok(w) => w,
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                return Err(format!("{name} exists; write! overwrites"));
            }
            Err(e) => return Err(format!("write failed: {e}")),
        };
        let view = self.settings.view_style;
        let v = self.text_view();
        let buf = v.buf();
        let failed = |e: std::io::Error| format!("write failed: {e}");
        let lines = match v.selection() {
            Some((start, end)) => {
                let mut out = Vec::new();
                shown(buf, view, start, end, &mut out)?;
                w.write_all(&out).map_err(failed)?;
                let mut lines = end.line - start.line + 1;
                if end.col == 0 && end.line > start.line {
                    lines -= 1;
                }
                lines
            }
            // The whole input as it came is its bytes: no line decoded.
            None if view == ViewStyle::Raw => {
                buf.write_to(&mut *w).map_err(failed)?;
                buf.len()
            }
            None => {
                let mut out = Vec::new();
                if let Some(last) = buf.len().checked_sub(1) {
                    let end = Pos {
                        line: last,
                        col: usize::MAX,
                    };
                    shown(buf, view, Pos::default(), end, &mut out)?;
                    if buf.trailing_newline() {
                        out.push(b'\n');
                    }
                }
                w.write_all(&out).map_err(failed)?;
                buf.len()
            }
        };
        w.done().map_err(failed)?;
        self.status = Some(match lines {
            1 => format!("wrote 1 line to {name}"),
            _ => format!("wrote {lines} lines to {name}"),
        });
        Ok(())
    }
}

/// Appends the chars from `start` to `end`, lines joined with newlines,
/// as the view shows them: under `Color` each run's style as SGR and
/// its link as OSC 8, the style a line carries in from before included;
/// under `Plain` the links alone; under `Raw` the text, escapes and all.
/// Ends on no style or link. A line that cannot be read is empty on
/// screen, and here an error: a file is never written short.
fn shown(
    buf: &Buffer,
    view: ViewStyle,
    start: Pos,
    end: Pos,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    let (mut style, mut link): (Style, Option<Link>) = Default::default();
    let chars = |out: &mut Vec<u8>, text: &[char]| {
        for c in text {
            out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
        }
    };
    for li in start.line..=end.line {
        let l = buf.line(li);
        let n = l.text.len();
        let from = if li == start.line {
            start.col.min(n)
        } else {
            0
        };
        let to = if li == end.line { end.col.min(n) } else { n };
        if li > start.line {
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
    match buf.read_error() {
        Some(e) => Err(format!("write failed: {e}")),
        None => Ok(()),
    }
}
