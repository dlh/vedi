//! The : prompt: a line number, a setting for the rest of the run, or
//! what a key does, with Tab completion.

use crate::app::prompt::cancels;
use crate::app::{App, Flow, Focus};
use crate::input::action::Command;
use crate::input::config::{self, Config};
use crate::input::key::{Key, KeyCode};
use crate::input::setting;
use crate::sys::clipboard::CommandCopier;
use crate::text::ansi::ViewStyle;
use crate::text::buffer::Pos;
use crate::view::layout::Mode;
use crate::view::screen::Terminal;

/// What the : prompt takes: goto, cycle, help, reload, quit and the
/// config verbs but clear_all_shortcuts, which would unbind : and q.
const COMMANDS: &[&str] = &[
    "goto",
    "cycle",
    "help",
    "reload",
    "quit",
    "wrap",
    "wrap_style",
    "tab_width",
    "edge_markers",
    "file_separators",
    "auto_reload",
    "status_line",
    "view_style",
    "clipboard_cmd",
    "map",
];

impl<T: Terminal> App<T> {
    pub(super) fn open_command(&mut self) {
        self.focus = Focus::Commanding;
        self.dragging = false;
        self.command.open();
    }

    /// Edits the : prompt. Enter runs what was typed and closes the
    /// prompt, unless Tab's list of commands is showing and the prompt
    /// holds one of them: then it adds the space after the command and
    /// keeps the prompt open, for the argument. Esc, Ctrl+g and Ctrl+c
    /// just close it.
    pub(super) fn command_key(&mut self, key: Key) -> Flow {
        let key = key.normalized();
        let (_, before) = last_word(&self.stem);
        let listing_commands =
            self.matches.is_some() && self.command.text() == self.filled && before.is_empty();
        if key.code != KeyCode::Tab {
            self.matches = None;
        }
        let command = self.filled.trim_end_matches(' ');
        match key.code {
            _ if cancels(key) => self.focus = Focus::Viewing,
            KeyCode::Enter if listing_commands && COMMANDS.contains(&command) => {
                let text = format!("{command} ");
                self.set_command(&text);
            }
            KeyCode::Enter => {
                self.focus = Focus::Viewing;
                if !self.command.is_empty() {
                    self.command.remember();
                }
                let line = self.command.text();
                return self.run_command(&line);
            }
            KeyCode::Tab => self.complete(),
            _ => {
                self.command.edit(key);
            }
        }
        Flow::Continue
    }

    /// Fills in the word being typed at the end of the : prompt from the
    /// candidates for its place: with one, the word and a space; with
    /// several, what they share, listing them sorted; with none,
    /// nothing. Tab again, the line as the last left it, fills in each
    /// match in turn and then what was typed.
    fn complete(&mut self) {
        if let Some(ms) = &self.matches
            && self.command.text() == self.filled
        {
            let pos = self.match_pos.map_or(0, |p| p + 1);
            if pos == ms.len() {
                self.match_pos = None;
                let stem = self.stem.clone();
                self.set_command(&stem);
                return;
            }
            self.match_pos = Some(pos);
            let (word, _) = last_word(&self.stem);
            let text = format!("{}{} ", &self.stem[..self.stem.len() - word.len()], ms[pos]);
            self.set_command(&text);
            return;
        }
        self.stem = self.command.text();
        let (word, before) = last_word(&self.stem);
        let base = &self.stem[..self.stem.len() - word.len()];
        let mut found: Vec<String> = candidates(&before)
            .into_iter()
            .filter(|c| c.starts_with(&word))
            .collect();
        found.sort();
        match &found[..] {
            [] => {}
            [one] => {
                let text = format!("{base}{one} ");
                self.set_command(&text);
            }
            [first, rest @ ..] => {
                let shared = rest.iter().fold(first.as_str(), |s, c| common_prefix(s, c));
                let text = format!("{base}{shared}");
                self.matches = Some(found.clone());
                self.match_pos = None;
                self.set_command(&text);
            }
        }
    }

    /// What a Tab leaves on the prompt.
    fn set_command(&mut self, text: &str) {
        self.command.text = text.chars().collect();
        self.filled = text.to_string();
    }

    /// Runs a : line. A number alone, or goto and a number, goes to
    /// that 1-based line, clamped to the input; cycle steps a setting;
    /// help, reload and quit do what their keys do; a config verb sets
    /// what it would in a file, for the rest of the run. Nothing typed
    /// does nothing; an error is reported on the status line.
    pub(super) fn run_command(&mut self, line: &str) -> Flow {
        let mut f: Vec<&str> = line.split_whitespace().collect();
        let Some(&verb) = f.first() else {
            return Flow::Continue;
        };
        if f.len() == 1 && is_number(verb) {
            f = vec!["goto", verb];
        }
        match f[0] {
            "goto" => {
                if f.len() != 2 || !is_number(f[1]) {
                    self.status = Some("goto takes a line number".into());
                    return Flow::Continue;
                }
                // Past usize is past the end all the same.
                let n = f[1].parse::<usize>().unwrap_or(usize::MAX);
                let (_, lines) = self.buf.part(self.cur.line);
                let line = lines
                    .start
                    .saturating_add(n)
                    .saturating_sub(1)
                    .min(lines.end.saturating_sub(1))
                    .max(lines.start);
                self.cur = Pos { line, col: 0 };
                self.drop();
                self.scroll_to_cursor();
                return Flow::Continue;
            }
            "cycle" => {
                match line.parse::<Command>() {
                    Ok(c) => self.cycle(c.arg.as_deref().unwrap_or_default()),
                    Err(e) => self.status = Some(e.to_string()),
                }
                return Flow::Continue;
            }
            "help" | "reload" | "quit" => {
                if f.len() != 1 {
                    self.status = Some(format!("{} takes no argument", f[0]));
                    return Flow::Continue;
                }
                match f[0] {
                    "help" => self.show_help(),
                    "reload" => self.reload(true),
                    _ => return Flow::Quit,
                }
                return Flow::Continue;
            }
            _ => {}
        }
        if !COMMANDS.contains(&verb) {
            self.status = Some(format!("unknown command {verb:?}"));
            return Flow::Continue;
        }
        let mut c = Config::default();
        if let Err(e) = config::parse_line(line, &mut c) {
            self.status = Some(e);
            return Flow::Continue;
        }
        match verb {
            "wrap" => self.set_mode(if c.wrap == Some(false) {
                Mode::NoWrap
            } else {
                Mode::Wrap
            }),
            "wrap_style" => self.wrap_style = c.wrap_style.unwrap_or_default(),
            "tab_width" => self.tab = c.tab_width.unwrap_or_default(),
            "edge_markers" => self.marks = c.edge_markers.unwrap_or_default(),
            "file_separators" => self.seps = c.file_separators.unwrap_or(true),
            "auto_reload" => {
                self.auto = c.auto_reload.unwrap_or(true);
                // A change during a reload asked for another.
                if !self.auto {
                    self.dirty = false;
                }
            }
            "status_line" => self.hide = c.status_line == Some(false),
            "view_style" => self.set_view(c.view_style.unwrap_or_default()),
            "clipboard_cmd" => {
                self.copier = Box::new(CommandCopier::new(c.clipboard_cmd.unwrap_or_default()))
            }
            "map" => self.keys = self.keys.apply(&c.keys),
            _ => {}
        }
        self.scroll_to_cursor();
        Flow::Continue
    }

    /// Sets a setting to the value after its current one, the first
    /// after the last, and reports the new value on the status line
    /// unless the line shows it anyway, as it does wrap, wrap_style and
    /// view_style, or is the setting.
    pub(super) fn cycle(&mut self, name: &str) {
        let Some(s) = setting::lookup(name) else {
            return;
        };
        let line = format!("{name} {}", s.next(&self.setting(name)));
        self.run_command(&line);
        if !matches!(name, "wrap" | "wrap_style" | "status_line" | "view_style") {
            self.status = Some(line);
        }
    }

    /// The current value of a setting cycle takes, as a config file
    /// spells it.
    fn setting(&self, name: &str) -> String {
        match name {
            "wrap" => yes_no(self.wrap_mode == Mode::Wrap),
            "wrap_style" => self.wrap_style.to_string(),
            "edge_markers" => yes_no(self.marks),
            "file_separators" => yes_no(self.seps),
            "auto_reload" => yes_no(self.auto),
            "status_line" => yes_no(!self.hide),
            "view_style" => self.view.to_string(),
            _ => String::new(),
        }
    }

    /// Switches between wrap and nowrap; the horizontal scroll starts
    /// over.
    fn set_mode(&mut self, m: Mode) {
        self.wrap_mode = m;
        self.xoff = 0;
    }

    /// Changes the view style. To or from raw the lines' text changes
    /// under the view, which keeps its place as over a reload.
    fn set_view(&mut self, v: ViewStyle) {
        self.view = v;
        let raw = v == ViewStyle::Raw;
        if self.text_buf().literal() == raw {
            return;
        }
        self.change_text(|b| b.set_literal(raw));
    }
}

/// Splits a prompt into the word being typed, "" after a space, and
/// the words before it.
fn last_word(text: &str) -> (String, Vec<String>) {
    let mut before: Vec<String> = text.split_whitespace().map(String::from).collect();
    let word = match before.last() {
        Some(_) if !text.ends_with(' ') => before.pop().unwrap_or_default(),
        _ => String::new(),
    };
    (word, before)
}

/// The words that may come after `before`, the words before the one
/// being completed: a command first, then its argument.
fn candidates(before: &[String]) -> Vec<String> {
    let strs: Vec<&str> = before.iter().map(String::as_str).collect();
    match strs[..] {
        [] => COMMANDS.iter().map(|&c| c.to_string()).collect(),
        ["cycle"] => setting::names().map(String::from).collect(),
        [s] => setting::lookup(s).map_or_else(Vec::new, |s| {
            s.values.iter().map(|&v| v.to_string()).collect()
        }),
        ["map", _] => Command::names(),
        ["map", _, "cycle"] => setting::names().map(String::from).collect(),
        _ => Vec::new(),
    }
}

fn common_prefix<'a>(a: &'a str, b: &str) -> &'a str {
    let n = a
        .char_indices()
        .zip(b.chars())
        .find(|((_, x), y)| x != y)
        .map_or(a.len().min(b.len()), |((i, _), _)| i);
    &a[..n]
}

fn yes_no(b: bool) -> String {
    if b { "yes" } else { "no" }.to_string()
}

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_word_splits() {
        let words = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(last_word(""), (String::new(), vec![]));
        assert_eq!(last_word("wr"), ("wr".to_string(), vec![]));
        assert_eq!(last_word("wrap "), (String::new(), words(&["wrap"])));
        assert_eq!(
            last_word("map x sel"),
            ("sel".to_string(), words(&["map", "x"]))
        );
    }

    #[test]
    fn common_prefix_of_strings() {
        assert_eq!(common_prefix("wrap", "wrap_style"), "wrap");
        assert_eq!(common_prefix("select_all", "select_down"), "select_");
        assert_eq!(common_prefix("ab", "cd"), "");
        assert_eq!(common_prefix("", "x"), "");
    }
}
