//! The : prompt: a line number, a setting for the rest of the run, or
//! what a key does, with Tab completion.

use crate::app::prompt::{Prompt, cancels};
use crate::app::{App, Flow, Focus};
use crate::input::action::Command;
use crate::input::config::{self, Config};
use crate::input::key::{Key, KeyCode};
use crate::input::setting::{self, Cycle, Settings};
use crate::sys::clipboard::CommandCopier;
use crate::text::ansi::ViewStyle;
use crate::text::buffer::Pos;
use crate::view::screen::Terminal;

/// What the : prompt runs itself; the config verbs follow.
const BUILTINS: &[&str] = &["goto", "cycle", "help", "reload", "quit"];

/// What the : prompt takes.
fn commands() -> impl Iterator<Item = &'static str> {
    BUILTINS.iter().copied().chain(setting::at_prompt())
}

fn is_command(name: &str) -> bool {
    commands().any(|c| c == name)
}

#[derive(Default)]
pub(super) struct State {
    pub line: Prompt,
    /// What the last Tab found, shown while the line is as it left it.
    matches: Option<Vec<String>>,
    /// The match filled in; `None` for what was typed.
    match_pos: Option<usize>,
    /// The line before the first Tab.
    stem: String,
    /// The line as the last Tab left it.
    filled: String,
}

impl State {
    /// The matches the last Tab found, while the line is as it left it,
    /// and which of them it filled in.
    pub fn listing(&self) -> Option<(&[String], Option<usize>)> {
        match &self.matches {
            Some(ms) if self.line.text() == self.filled => Some((ms, self.match_pos)),
            _ => None,
        }
    }

    /// Whether the listing is of commands.
    fn listing_commands(&self) -> bool {
        self.listing().is_some() && last_word(&self.stem).1.is_empty()
    }

    /// The stem up to the word being completed.
    fn base(&self) -> &str {
        let (word, _) = last_word(&self.stem);
        &self.stem[..self.stem.len() - word.len()]
    }

    /// Fills in the word being typed at the end of the : prompt from the
    /// candidates for its place: with one, the word and a space; with
    /// several, what they share, or the first of them when that adds
    /// nothing, listing them; with none, nothing. A setting's whole
    /// value becomes the one after it, marked in the list of them all.
    /// Tab again, the line as the last left it, fills in each match in
    /// turn and then what was typed.
    fn complete(&mut self, settings: &Settings) {
        if let Some((ms, at)) = self.listing() {
            let pos = at.map_or(0, |p| p + 1);
            let (text, at) = match ms.get(pos) {
                Some(m) => (format!("{}{m} ", self.base()), Some(pos)),
                None => (self.stem.clone(), None),
            };
            self.match_pos = at;
            self.fill(&text);
            return;
        }
        self.stem = self.line.text();
        let (word, before) = last_word(&self.stem);
        let all = candidates(&before, settings);
        let found: Vec<String> = all
            .iter()
            .filter(|c| c.starts_with(word))
            .cloned()
            .collect();
        let base = self.base();
        match &found[..] {
            [] => {}
            [one] => {
                let at = all.iter().position(|c| c == one);
                let text = match at {
                    Some(p) if one == word && is_values(&before) => {
                        let p = (p + 1) % all.len();
                        let text = format!("{base}{} ", all[p]);
                        self.match_pos = Some(p);
                        self.matches = Some(all);
                        text
                    }
                    _ => format!("{base}{one} "),
                };
                self.fill(&text);
            }
            [first, rest @ ..] => {
                let shared = rest.iter().fold(first.as_str(), |s, c| common_prefix(s, c));
                let (text, at) = if shared.len() > word.len() {
                    (format!("{base}{shared}"), None)
                } else {
                    (format!("{base}{first} "), Some(0))
                };
                self.match_pos = at;
                self.matches = Some(found);
                self.fill(&text);
            }
        }
    }

    /// What a Tab leaves on the prompt.
    fn fill(&mut self, text: &str) {
        self.line.text = text.chars().collect();
        self.filled = text.to_string();
    }
}

impl<T: Terminal> App<T> {
    pub(super) fn open_command(&mut self) {
        self.focus = Focus::Commanding;
        self.mouse.end_drag();
        self.command.line.open();
    }

    /// Edits the : prompt. Enter runs what was typed and closes the
    /// prompt, unless Tab's list of commands is showing and the prompt
    /// holds one of them: then it adds the space after the command and
    /// keeps the prompt open, for the argument. Esc, Ctrl+g and Ctrl+c
    /// just close it.
    pub(super) fn command_key(&mut self, key: Key) -> Flow {
        let key = key.normalized();
        let c = &mut self.command;
        // Any key but Tab takes the listing down.
        let listing_commands = c.listing_commands();
        if key.code != KeyCode::Tab {
            c.matches = None;
        }
        let command = c.filled.trim_end_matches(' ');
        match key.code {
            _ if cancels(key) => self.focus = Focus::Viewing,
            KeyCode::Enter if listing_commands && is_command(command) => {
                let text = format!("{command} ");
                c.fill(&text);
            }
            KeyCode::Enter => {
                self.focus = Focus::Viewing;
                if !c.line.is_empty() {
                    c.line.remember();
                }
                let line = c.line.text();
                return self.run_command(&line);
            }
            KeyCode::Tab => c.complete(&self.settings),
            _ => {
                c.line.edit(key);
            }
        }
        Flow::Continue
    }

    /// Runs a : line, reporting an error on the status line.
    pub(super) fn run_command(&mut self, line: &str) -> Flow {
        match self.command(line) {
            Ok(flow) => flow,
            Err(e) => {
                self.status = Some(e);
                Flow::Continue
            }
        }
    }

    /// A number alone, or goto and a number, goes to that 1-based line,
    /// clamped to the input; cycle steps a setting; help, reload and
    /// quit do what their keys do; a config verb sets what it would in
    /// a file, for the rest of the run. Nothing typed does nothing.
    fn command(&mut self, line: &str) -> Result<Flow, String> {
        let f: Vec<&str> = line.split_whitespace().collect();
        match f[..] {
            [] => {}
            [n] | ["goto", n] if is_number(n) => self.goto(n),
            ["goto", ..] => return Err("goto takes a line number".into()),
            [verb @ ("help" | "reload" | "quit"), _, ..] => {
                return Err(format!("{verb} takes no argument"));
            }
            ["cycle" | "help" | "reload" | "quit", ..] => {
                let c: Command = line.parse().map_err(|e| format!("{e}"))?;
                return Ok(self.view_key(Some(c)));
            }
            [verb, ..] if is_command(verb) => self.set(verb, line)?,
            [verb, ..] => return Err(format!("unknown command {verb:?}")),
        }
        Ok(Flow::Continue)
    }

    fn goto(&mut self, n: &str) {
        // Past usize is past the end all the same.
        let n = n.parse::<usize>().unwrap_or(usize::MAX);
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
    }

    /// Sets what a config line would.
    fn set(&mut self, verb: &str, line: &str) -> Result<(), String> {
        let mut c = Config {
            settings: self.settings.clone(),
            ..Config::default()
        };
        config::parse_line(line, &mut c)?;
        self.settings = c.settings;
        match verb {
            // The horizontal scroll starts over.
            "wrap" => self.xoff = 0,
            // A change during a reload asked for another.
            "auto_reload" if !self.settings.auto_reload => self.dirty = false,
            "view_style" => self.set_literal(),
            "clipboard_cmd" => {
                self.copier = Box::new(CommandCopier::new(c.clipboard_cmd.unwrap_or_default()))
            }
            "map" => self.keys = self.keys.apply(&c.keys),
            _ => {}
        }
        self.scroll_to_cursor();
        Ok(())
    }

    /// Sets a setting to the value after its current one, the first
    /// after the last, and reports the new value on the status line
    /// unless the line shows it anyway.
    pub(super) fn cycle(&mut self, name: &str) {
        let Some(s) = setting::verb(name) else {
            return;
        };
        match s.cycle {
            Cycle::No => {}
            Cycle::Toggle => self.toggle(name),
            Cycle::Values(_) => {
                let line = format!("{name} {}", s.next(&self.settings.value(name)));
                self.run_command(&line);
                if !s.shown {
                    self.status = Some(line);
                }
            }
        }
    }

    /// Switches a toggle verb off, then on again.
    fn toggle(&mut self, name: &str) {
        if name == "open_cmd" {
            self.toggle_open_cmd();
        }
    }

    /// Reads the files through open_cmd, or as they are. The selection
    /// goes: the lines it covered are not the new text's. A reload
    /// still being read is dropped: it may be waiting on a command
    /// that never ends.
    fn toggle_open_cmd(&mut self) {
        if self.open.is_none() {
            self.status = Some("not a file".into());
            return;
        }
        self.through_open_cmd = !self.through_open_cmd;
        self.anchor = None;
        self.marking = false;
        self.next = None;
        self.dirty = false;
        self.reload_then(true, true);
    }

    /// The command the cursor's input is read through, or '' for none,
    /// as the status line reports it; the text may be set aside under
    /// help.
    pub(super) fn open_cmd_status(&self) -> String {
        let (buf, cur) = match &self.text {
            Some(t) => (&t.buf, t.cur),
            None => (&self.buf, self.cur),
        };
        let (part, _) = buf.part(cur.line);
        let argv = match &self.open_cmd {
            Some(f) if self.through_open_cmd => self.names.get(part).and_then(|n| f(n)),
            _ => None,
        };
        let cmd = argv.map_or_else(|| "''".to_string(), |a| a.join(" "));
        format!("open_cmd {cmd}")
    }

    /// Decodes the text as the view style has it. To or from raw the
    /// lines' text changes under the view, which keeps its place as
    /// over a reload.
    fn set_literal(&mut self) {
        let raw = self.settings.view_style == ViewStyle::Raw;
        if self.text_buf().literal() == raw {
            return;
        }
        self.change_text(|b| b.set_literal(raw));
    }
}

/// Splits a prompt into the word being typed, "" after a space, and
/// the words before it.
fn last_word(text: &str) -> (&str, Vec<&str>) {
    let mut before: Vec<&str> = text.split_whitespace().collect();
    let word = match before.last() {
        Some(_) if !text.ends_with(' ') => before.pop().unwrap_or_default(),
        _ => "",
    };
    (word, before)
}

/// The words that may come after `before`, the words before the one
/// being completed: a command first, then its argument. Names come
/// sorted; a setting's values from the one after its current, so the
/// first is a change.
fn candidates(before: &[&str], settings: &Settings) -> Vec<String> {
    let mut names: Vec<String> = match before {
        [] => commands().map(String::from).collect(),
        ["cycle"] | ["map", _, "cycle"] => setting::names().map(String::from).collect(),
        [s] => {
            return setting::lookup(s).map_or_else(Vec::new, |s| {
                let values = s.values();
                let next = s.next(&settings.value(s.name));
                let at = values.iter().position(|&v| v == next).unwrap_or(0);
                let (tail, head) = values.split_at(at);
                head.iter().chain(tail).map(|&v| v.to_string()).collect()
            });
        }
        ["map", _] => Command::names(),
        _ => Vec::new(),
    };
    names.sort();
    names
}

/// Whether the word after `before` is a setting's value.
fn is_values(before: &[&str]) -> bool {
    matches!(before, [s] if setting::lookup(s).is_some())
}

fn common_prefix<'a>(a: &'a str, b: &str) -> &'a str {
    let n = a
        .char_indices()
        .zip(b.chars())
        .find(|((_, x), y)| x != y)
        .map_or(a.len().min(b.len()), |((i, _), _)| i);
    &a[..n]
}

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_word_splits() {
        assert_eq!(last_word(""), ("", vec![]));
        assert_eq!(last_word("wr"), ("wr", vec![]));
        assert_eq!(last_word("wrap "), ("", vec!["wrap"]));
        assert_eq!(last_word("map x sel"), ("sel", vec!["map", "x"]));
    }

    #[test]
    fn common_prefix_of_strings() {
        assert_eq!(common_prefix("wrap", "wrap_style"), "wrap");
        assert_eq!(common_prefix("select_all", "select_down"), "select_");
        assert_eq!(common_prefix("ab", "cd"), "");
        assert_eq!(common_prefix("", "x"), "");
    }
}
