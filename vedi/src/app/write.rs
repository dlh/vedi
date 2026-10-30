//! :write, which saves the text to a file as the view shows it.

use std::io::ErrorKind;
use std::path::PathBuf;

use crate::app::{App, Flow};
use crate::text::ansi::ViewStyle;
use crate::view::screen::Terminal;

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
