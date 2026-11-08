//! Tab completion at the : prompt: a command, then its argument.

use std::path::PathBuf;

use crate::app::command::{commands, is_command};
use crate::app::options::Lister;
use crate::app::write::{self, Form};
use crate::input::action::Command;
use crate::input::setting::{self, Settings};

pub(super) struct Completion {
    /// The settings as the prompt opened: a setting's values are
    /// offered from the one after its current. None can change while
    /// the prompt is open.
    settings: Settings,
    /// Lists a directory, for write's name, and what its ~ stands for.
    list: Lister,
    home: Option<PathBuf>,
    /// What the last Tab found, shown while the line is as it left it.
    matches: Option<Vec<String>>,
    /// The match filled in; `None` for what was typed.
    match_pos: Option<usize>,
    /// The line before the first Tab.
    stem: String,
    /// The line as the last Tab left it.
    filled: String,
}

impl Completion {
    pub fn new(settings: &Settings, list: Lister, home: Option<PathBuf>) -> Completion {
        Completion {
            settings: settings.clone(),
            list,
            home,
            matches: None,
            match_pos: None,
            stem: String::new(),
            filled: String::new(),
        }
    }

    /// The matches the last Tab found, while `line` is as it left it,
    /// and which of them it filled in.
    pub fn listing(&self, line: &str) -> Option<(&[String], Option<usize>)> {
        match &self.matches {
            Some(ms) if line == self.filled => Some((ms, self.match_pos)),
            _ => None,
        }
    }

    /// Whether the listing is of commands.
    fn listing_commands(&self, line: &str) -> bool {
        self.listing(line).is_some() && last_word(&self.stem).1.is_empty()
    }

    /// Takes the listing down, as any key but Tab does.
    pub fn clear(&mut self) {
        self.matches = None;
    }

    /// Enter: while Tab's list of commands is showing and `line` holds
    /// one of them, the line to put up instead of running it, the space
    /// after the command added for the argument.
    pub fn enter(&mut self, line: &str) -> Option<String> {
        let command = self.filled.trim_end_matches(' ').to_string();
        let fill = self.listing_commands(line) && is_command(&command);
        self.matches = None;
        fill.then(|| self.fill(format!("{command} ")))
    }

    /// What Enter submits: `line` without the trailing space Tab added,
    /// which a recall would complete a word after that is not there.
    pub fn submitted(&self, line: &str) -> String {
        line.trim_end_matches(' ').to_string()
    }

    /// The stem up to the word being completed.
    fn base(&self) -> &str {
        let (word, _) = split(&self.stem);
        &self.stem[..self.stem.len() - word.len()]
    }

    /// Tab: fills in the word being typed at the end of `line` from the
    /// candidates for its place: with one, the word and a space; with
    /// several, what they share, or the first of them when that adds
    /// nothing, listing them; with none, nothing. A setting's whole
    /// value becomes the one after it, marked in the list of them all.
    /// Tab again, the line as the last left it, fills in each match in
    /// turn and then what was typed. Returns the line as filled in.
    pub fn tab(&mut self, line: &str) -> Option<String> {
        if let Some((ms, at)) = self.listing(line) {
            let pos = at.map_or(0, |p| p + 1);
            let (text, at) = match ms.get(pos) {
                Some(m) => (format!("{}{}", self.base(), done(m)), Some(pos)),
                None => (self.stem.clone(), None),
            };
            self.match_pos = at;
            return Some(self.fill(text));
        }
        self.stem = line.to_string();
        let (word, before) = split(&self.stem);
        let all = self.candidates(&before, word);
        let found: Vec<String> = all
            .iter()
            .filter(|c| c.starts_with(word))
            .cloned()
            .collect();
        let base = self.base().to_string();
        match &found[..] {
            [] => None,
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
                    _ => format!("{base}{}", done(one)),
                };
                Some(self.fill(text))
            }
            [first, rest @ ..] => {
                let shared = rest.iter().fold(first.as_str(), |s, c| common_prefix(s, c));
                let (text, at) = if shared.len() > word.len() {
                    (format!("{base}{shared}"), None)
                } else {
                    (format!("{base}{}", done(first)), Some(0))
                };
                self.match_pos = at;
                self.matches = Some(found);
                Some(self.fill(text))
            }
        }
    }

    /// What a Tab leaves on the prompt.
    fn fill(&mut self, text: String) -> String {
        self.filled.clone_from(&text);
        text
    }

    /// The words that may come after `before`, the words before `word`,
    /// the one being completed: a command first, then its argument;
    /// after write, a flag when `word` begins with -, else a file name.
    /// Names come sorted; a setting's values from the one after its
    /// current, so the first is a change; cycle's setting's values in
    /// their order.
    fn candidates(&self, before: &[&str], word: &str) -> Vec<String> {
        let mut names: Vec<String> = match before {
            [] => commands().map(String::from).collect(),
            ["cycle"] | ["map", _, "cycle"] => setting::cycled().map(String::from).collect(),
            ["cycle", s, ..] | ["map", _, "cycle", s, ..] => {
                return setting::lookup(s).map_or_else(Vec::new, |s| {
                    s.values().iter().map(|&v| v.to_string()).collect()
                });
            }
            ["write" | "write!"] if word.starts_with('-') => {
                vec!["--color".into(), "--raw".into()]
            }
            // A name here beginning with - gets ./ before it, or write
            // would read it as the flag; after one, it cannot.
            ["write" | "write!"] => self
                .paths(word)
                .into_iter()
                .map(|p| {
                    if p.starts_with('-') {
                        format!("./{p}")
                    } else {
                        p
                    }
                })
                .collect(),
            ["write" | "write!", _] => self.paths(word),
            [s] => {
                return setting::lookup(s).map_or_else(Vec::new, |s| {
                    let values = s.values();
                    let next = s.next(&s.value(&self.settings));
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

    /// The names in the directory of `word`, a path being typed, that
    /// begin as its last part does, each after that directory as typed
    /// and a directory with a / after it; a hidden name only after a
    /// dot. A leading ~ is the home directory, as write has it.
    fn paths(&self, word: &str) -> Vec<String> {
        if word == "~" {
            return vec!["~/".into()];
        }
        let (dir, part) = word.split_at(word.rfind('/').map_or(0, |i| i + 1));
        let path = match dir {
            "" => PathBuf::from("."),
            _ => write::expand(self.home.as_deref(), dir),
        };
        let Ok(entries) = (self.list)(&path) else {
            return Vec::new();
        };
        entries
            .iter()
            .filter(|e| e.name.starts_with(part))
            .filter(|e| part.starts_with('.') || !e.name.starts_with('.'))
            .map(|e| format!("{dir}{}{}", e.name, if e.dir { "/" } else { "" }))
            .collect()
    }
}

/// A candidate filled in whole, with the space that lets the next word
/// type straight on; a directory takes none, so Tab again goes into it.
fn done(candidate: &str) -> String {
    if candidate.ends_with('/') {
        candidate.to_string()
    } else {
        format!("{candidate} ")
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

/// `last_word`, except that write's name is the rest of the line after
/// the command and its flag, spaces included.
fn split(text: &str) -> (&str, Vec<&str>) {
    write_name(text).unwrap_or_else(|| last_word(text))
}

/// write's name as typed so far, and the words before it; `None` for
/// another command, or a first word beginning with -, a flag.
fn write_name(text: &str) -> Option<(&str, Vec<&str>)> {
    let (verb, rest) = text.trim_start().split_once(char::is_whitespace)?;
    if !matches!(verb, "write" | "write!") {
        return None;
    }
    let mut before = vec![verb];
    let mut name = rest.trim_start();
    if let Some((flag, after)) = name.split_once(char::is_whitespace)
        && Form::flag(flag).is_some()
    {
        before.push(flag);
        name = after.trim_start();
    }
    (before.len() == 2 || !name.starts_with('-')).then_some((name, before))
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
    fn split_takes_writes_name_whole() {
        assert_eq!(split("write"), ("write", vec![]));
        assert_eq!(split("write "), ("", vec!["write"]));
        assert_eq!(split("write my f"), ("my f", vec!["write"]));
        assert_eq!(split("write! --raw a b"), ("a b", vec!["write!", "--raw"]));
        assert_eq!(split("write --c"), ("--c", vec!["write"]));
        assert_eq!(split("write --raw -x"), ("-x", vec!["write", "--raw"]));
        assert_eq!(split("wrap_style w"), ("w", vec!["wrap_style"]));
    }

    #[test]
    fn common_prefix_of_strings() {
        assert_eq!(common_prefix("wrap", "wrap_style"), "wrap");
        assert_eq!(common_prefix("select_all", "select_down"), "select_");
        assert_eq!(common_prefix("ab", "cd"), "");
        assert_eq!(common_prefix("", "x"), "");
    }
}
