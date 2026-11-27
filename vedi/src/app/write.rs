//! :write, which saves the text to a file: as copy sends it, with its
//! colors, or as it came.

use std::io::ErrorKind;

use crate::app::filter::Blocks;
use crate::app::{App, Flow};
use crate::sys::words;
use crate::text::ansi::ViewStyle;
use crate::view::screen::Terminal;

/// How :write puts the text in the file.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(super) enum Form {
    /// As copy sends it: the chars shown.
    Plain,
    /// Each run's style as SGR and its link as OSC 8.
    Color,
    /// The input's bytes.
    Raw,
}

impl Form {
    /// The flags before the name, and the form each asks for.
    pub(super) const FLAGS: [(&str, Form); 2] = [("--color", Form::Color), ("--raw", Form::Raw)];

    /// The form a flag before the name asks for; `None` for another word.
    pub(super) fn flag(word: &str) -> Option<Form> {
        Form::FLAGS
            .iter()
            .find(|(f, _)| *f == word)
            .map(|&(_, form)| form)
    }
}

/// What follows write: the flag, a first word beginning with --, and
/// the name, the rest of `args`, spaces inside and after it kept.
pub(super) fn split_args(args: &str) -> (Option<&str>, &str) {
    let args = args.trim_start();
    match args.split_once(char::is_whitespace) {
        Some((flag, name)) if flag.starts_with("--") => (Some(flag), name.trim_start()),
        None if args.starts_with("--") => (Some(args), ""),
        _ => (None, args),
    }
}

impl<T: Terminal> App<T> {
    /// Writes the selection, or with none the whole text, to the file
    /// `name`, made new, or emptied first when `overwrite`; a file that
    /// exists is otherwise refused. In a filter's list, the text is the
    /// lines listed. A leading ~ is the home directory.
    /// The text goes as `form` has it; raw, a selection is its lines
    /// whole, as they came. Says how many lines went.
    pub(super) fn write(
        &mut self,
        name: &str,
        overwrite: bool,
        form: Form,
    ) -> Result<Flow, String> {
        let path = words::expand(self.home.as_deref(), name);
        let mut w = match (self.write)(&path, overwrite) {
            Ok(w) => w,
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                return Err(format!("{name} exists; write! overwrites"));
            }
            Err(e) => return Err(format!("write failed: {e}")),
        };
        let v = &mut self.views.shown;
        let failed = |e: std::io::Error| format!("write failed: {e}");
        let lines = match (v.selection(), form) {
            // The whole input as it came is its bytes: no line decoded.
            (None, Form::Raw) => {
                let buf = v.buf();
                buf.write_to(&mut *w).map_err(failed)?;
                buf.len()
            }
            (Some(sel), Form::Raw) => {
                let mut blocks = Blocks::new(v.buf());
                // A line the selection ends at the start of stays out,
                // as the count has it.
                for li in sel.start.line..sel.start.line + sel.lines() {
                    // A line that cannot be read leaves the error with
                    // the buffer; a file is never written short.
                    let Some(b) = blocks.line(li) else {
                        break;
                    };
                    w.write_all(b).map_err(failed)?;
                }
                if let Some(e) = v.buf().read_error() {
                    return Err(format!("write failed: {e}"));
                }
                sel.lines()
            }
            (sel, form) => {
                // Every line goes, an empty last one too, which a
                // selection ending there would not count.
                let lines = sel.map_or(v.buf().len(), |s| s.lines());
                // A literal buffer has no runs to encode: its escapes
                // are text, and go as text.
                let view = match form {
                    Form::Color if !v.buf().literal() => ViewStyle::Color,
                    _ => ViewStyle::Raw,
                };
                let mut out = Vec::new();
                if let Some(sel) = sel.or_else(|| v.whole()) {
                    sel.shown(v.buf(), view, &mut out);
                }
                // A line that cannot be read is empty on screen; a file
                // is never written short.
                if let Some(e) = v.buf().read_error() {
                    return Err(format!("write failed: {e}"));
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
