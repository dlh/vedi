//! Tab completion at the : prompt: a command, then its argument.

use crate::app::command::{commands, is_command};
use crate::input::action::Command;
use crate::input::setting::{self, Settings};

pub(super) struct Completion {
    /// The settings as the prompt opened: a setting's values are
    /// offered from the one after its current. None can change while
    /// the prompt is open.
    settings: Settings,
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
    pub fn new(settings: &Settings) -> Completion {
        Completion {
            settings: settings.clone(),
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
        let (word, _) = last_word(&self.stem);
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
                Some(m) => (format!("{}{m} ", self.base()), Some(pos)),
                None => (self.stem.clone(), None),
            };
            self.match_pos = at;
            return Some(self.fill(text));
        }
        self.stem = line.to_string();
        let (word, before) = last_word(&self.stem);
        let all = candidates(&before, &self.settings);
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
                    _ => format!("{base}{one} "),
                };
                Some(self.fill(text))
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
                Some(self.fill(text))
            }
        }
    }

    /// What a Tab leaves on the prompt.
    fn fill(&mut self, text: String) -> String {
        self.filled.clone_from(&text);
        text
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
                let next = s.next(&s.value(settings));
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
