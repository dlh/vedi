//! :write, which saves the text to a file as the view shows it.

use std::io::ErrorKind;
use std::path::PathBuf;

use crate::app::text_view::Selection;
use crate::app::{App, Flow};
use crate::text::ansi::{Link, ViewStyle};
use crate::text::buffer::Buffer;
use crate::view::render::{osc8, sgr};
use crate::view::screen::Terminal;
use crate::view::style::Style;

impl<T: Terminal> App<T> {
    /// Writes the selection, or with none the whole text, to the file
    /// `name`, made new, or emptied first when `overwrite`; a file that
    /// exists is otherwise refused. In a filter's list, the text is the
    /// lines listed. A leading ~ is the home directory.
    /// The text goes as the view shows it: with its colors and links,
    /// with links alone, or as it came. Says how many lines went.
    pub(super) fn write(&mut self, name: &str, overwrite: bool) -> Result<Flow, String> {
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
        let v = &mut self.views.shown;
        let failed = |e: std::io::Error| format!("write failed: {e}");
        let lines = match (v.selection(), view) {
            // The whole input as it came is its bytes: no line decoded.
            (None, ViewStyle::Raw) => {
                let buf = v.buf();
                buf.write_to(&mut *w).map_err(failed)?;
                buf.len()
            }
            (sel, _) => {
                // Every line goes, an empty last one too, which a
                // selection ending there would not count.
                let lines = sel.map_or(v.buf().len(), |s| s.lines());
                let mut out = Vec::new();
                if let Some(sel) = sel.or_else(|| v.whole()) {
                    shown(v.buf(), view, sel, &mut out)?;
                }
                w.write_all(&out).map_err(failed)?;
                lines
            }
        };
        w.done().map_err(failed)?;
        Ok(Flow::Say(match lines {
            1 => format!("wrote 1 line to {name}"),
            _ => format!("wrote {lines} lines to {name}"),
        }))
    }
}

/// Appends the chars `sel` holds, lines joined with newlines, as the
/// view shows them: under `Color` each run's style as SGR and its link
/// as OSC 8, the style a line carries in from before included; under
/// `Plain` the links alone; under `Raw` the text, escapes and all. Ends
/// on no style or link, then the last newline when `sel` holds it. A
/// line that cannot be read is empty on screen, and here an error: a
/// file is never written short.
pub(super) fn shown(
    buf: &Buffer,
    view: ViewStyle,
    sel: Selection,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    let (mut style, mut link): (Style, Option<Link>) = Default::default();
    let chars = |out: &mut Vec<u8>, text: &[char]| {
        for c in text {
            out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
        }
    };
    for li in sel.start.line..=sel.end.line {
        let l = buf.line(li);
        let n = l.text.len();
        let cols = sel.cols(li, n);
        let (from, to) = (cols.start, cols.end.min(n));
        if li > sel.start.line {
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
    if sel.newline {
        out.push(b'\n');
    }
    match buf.read_error() {
        Some(e) => Err(format!("write failed: {e}")),
        None => Ok(()),
    }
}
