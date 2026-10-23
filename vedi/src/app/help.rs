//! The key bindings shown in the text's place, laid out after less's
//! help, 80 columns wide whatever the screen.

use std::mem;

use crate::app::text_view::TextView;
use crate::app::{App, Flow};
use crate::input::action::{Action, Command};
use crate::input::help::Section;
use crate::text::buffer::{Buffer, Progress, Storage};
use crate::text::index::Indexer;
use crate::view::screen::Terminal;

/// How wide the help is laid out, whatever the screen: less's help is
/// likewise fixed. A narrower screen cuts it at the edge and scrolls
/// sideways.
const HELP_COLS: usize = 80;

impl<T: Terminal> App<T> {
    /// Puts the bindings in the text's place, as a view-led text view
    /// of their own, so drawing, scrolling and searching are the text's.
    /// The search starts over: help's is its own.
    pub(super) fn show_help(&mut self) {
        let help = TextView::view_led(help_buffer(&self.keys.help()));
        self.help = Some(mem::replace(&mut self.view, help));
        self.mouse.end_drag();
        self.sync_view();
    }

    /// Puts the text back as it was. Its view is laid out for the
    /// screen as it is now at the next draw: a cursor that was on screen
    /// stays on screen, one scrolled off stays off.
    pub(super) fn hide_help(&mut self) {
        if let Some(text) = self.help.take() {
            self.view = text;
        }
    }

    /// Scrolls the bindings by a motion and searches them by the search
    /// keys, as the text's keys do on a view-led view; any other key
    /// returns. The search is the text's: its pattern, direction and
    /// highlight carry over, as less's do.
    pub(super) fn help_key(&mut self, cmd: Option<Command>) -> Flow {
        let stays = |c: &Command| {
            matches!(
                c.action,
                Action::Search | Action::SearchBack | Action::SearchNext | Action::SearchPrev
            ) || (c.action.is_movement() && !c.extend)
        };
        match cmd {
            Some(c) if stays(&c) => self.view_key(c),
            _ => {
                self.hide_help();
                Flow::Continue
            }
        }
    }
}

/// The bindings laid out as text, read in full.
fn help_buffer(secs: &[Section]) -> Buffer {
    let text = help_text(secs);
    let mut ix = Indexer::new();
    let mut buf = Buffer::new(Storage::Memory(Vec::new()), false);
    buf.apply(Progress::Data {
        chunk: Some(text.as_bytes().into()),
        indexed: ix.feed(text.as_bytes()),
    });
    buf.apply(Progress::Data {
        chunk: None,
        indexed: ix.finish(),
    });
    buf.apply(Progress::Done {
        err: None,
        trailing_newline: true,
    });
    buf
}

/// Lays out the help screen as text: a centered title, a note on the
/// modifier glyphs, the first section's rows with no heading, then each
/// section's name centered in capitals between dashed rules. Rows are
/// indented two, the keys two spaces apart in a column as wide as the
/// widest list but at most half the room, the doc beside with a period;
/// a list or a doc wider than its column continues on the next line, in
/// its column, and a key wider than the column has a line to itself.
fn help_text(secs: &[Section]) -> String {
    let glyphs = |keys: &[String]| -> Vec<String> { keys.iter().map(|k| help_glyphs(k)).collect() };
    let keyw = secs
        .iter()
        .flat_map(|s| &s.rows)
        .map(|r| glyphs(&r.keys).join("  ").chars().count())
        .max()
        .unwrap_or(0)
        .min((HELP_COLS - 4) / 2);
    let docw = HELP_COLS - 4 - keyw;
    let rule = format!(" {}\n", "-".repeat(HELP_COLS - 2));
    let mut out = format!("{}\n\n", help_center("SUMMARY OF VEDI COMMANDS"));
    out.push_str("      ⌃ ⌥ ⌘ are Ctrl, Alt and Cmd: ⌃F is Ctrl+f in the config file.\n\n");
    for s in secs {
        if !s.name.is_empty() {
            out.push_str(&format!("\n{}\n\n", help_center(&s.name.to_uppercase())));
        }
        for r in &s.rows {
            let kl = wrap_words(&glyphs(&r.keys), "  ", keyw);
            let doc: Vec<String> = format!("{}.", r.doc)
                .split_whitespace()
                .map(String::from)
                .collect();
            let dl = wrap_words(&doc, " ", docw);
            for j in 0..kl.len().max(dl.len()) {
                let k = kl.get(j).map_or("", String::as_str);
                let d = dl.get(j).map_or("", String::as_str);
                let pad = " ".repeat(keyw.saturating_sub(k.chars().count()));
                let line = format!("  {k}{pad}  {d}");
                out.push_str(line.trim_end_matches(' '));
                out.push('\n');
            }
        }
        out.push_str(&rule);
    }
    out
}

/// Centers `text` in `HELP_COLS`.
fn help_center(text: &str) -> String {
    let pad = HELP_COLS.saturating_sub(text.chars().count()) / 2;
    format!("{}{text}", " ".repeat(pad))
}

/// A key's help name: Ctrl+, Alt+ and Cmd+ as ⌃ ⌥ ⌘, and a letter
/// under them a capital, as ⌃F.
fn help_glyphs(name: &str) -> String {
    let mut glyphs = String::new();
    let mut name = name;
    let mut again = true;
    while again {
        again = false;
        for (word, glyph) in [("Ctrl+", "⌃"), ("Alt+", "⌥"), ("Cmd+", "⌘")] {
            if let Some(rest) = name.strip_prefix(word) {
                glyphs.push_str(glyph);
                name = rest;
                again = true;
            }
        }
    }
    let key = name.strip_prefix("Shift+").unwrap_or(name);
    if !glyphs.is_empty() && key.chars().count() == 1 {
        return format!(
            "{glyphs}{}",
            name.to_uppercase().replacen("SHIFT+", "Shift+", 1)
        );
    }
    format!("{glyphs}{name}")
}

/// Joins `words` with `sep` into lines of at most `width` chars; a word
/// wider than that gets a line of its own.
fn wrap_words(words: &[String], sep: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for w in words {
        if line.is_empty() {
            line = w.clone();
        } else if line.chars().count() + sep.chars().count() + w.chars().count() <= width {
            line.push_str(sep);
            line.push_str(w);
        } else {
            lines.push(mem::replace(&mut line, w.clone()));
        }
    }
    lines.push(line);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::help::Row;

    fn row(keys: &[&str], doc: &str) -> Row {
        Row {
            keys: keys.iter().map(|k| k.to_string()).collect(),
            doc: doc.to_string(),
        }
    }

    /// The layout of less's help, 80 columns wide whatever the screen.
    #[test]
    fn help_text_layout() {
        let secs = [
            Section {
                name: String::new(),
                rows: vec![row(&["h"], "Show the key bindings")],
            },
            Section {
                name: "Moving".into(),
                rows: vec![
                    row(&["Up", "Down", "j"], "Move the cursor"),
                    row(
                        &["Ctrl+f", "Alt+Shift+Left", "Cmd+Shift+g", "Alt+<"],
                        "Copy the selection and quit; with none, down a line",
                    ),
                ],
            },
        ];
        let want = "                            SUMMARY OF VEDI COMMANDS\n\
            \n      ⌃ ⌥ ⌘ are Ctrl, Alt and Cmd: ⌃F is Ctrl+f in the config file.\n\
            \n  h                              Show the key bindings.\n \
            ------------------------------------------------------------------------------\n\
            \n                                     MOVING\n\
            \n  Up  Down  j                    Move the cursor.\n  \
            ⌃F  ⌥Shift+Left  ⌘Shift+G  ⌥<  Copy the selection and quit; with none, down a\n                                 \
            line.\n \
            ------------------------------------------------------------------------------\n";
        assert_eq!(help_text(&secs), want);
    }

    #[test]
    fn glyphs() {
        let tests = [
            ("j", "j"),
            ("G", "G"),
            ("Space", "Space"),
            ("Shift+Left", "Shift+Left"),
            ("Ctrl+f", "⌃F"),
            ("Ctrl+Left", "⌃Left"),
            ("Ctrl+Space", "⌃Space"),
            ("Alt+v", "⌥V"),
            ("Alt+<", "⌥<"),
            ("Cmd+c", "⌘C"),
            ("Cmd+Shift+g", "⌘Shift+G"),
            ("Ctrl+Shift+Left", "⌃Shift+Left"),
            ("Ctrl+Alt+Cmd+Shift+Up", "⌃⌥⌘Shift+Up"),
            ("Up, Down at the prompt", "Up, Down at the prompt"),
        ];
        for (name, want) in tests {
            assert_eq!(help_glyphs(name), want, "{name}");
        }
    }
}
