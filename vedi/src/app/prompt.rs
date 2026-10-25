//! A line being typed on the status row, with the history of the lines
//! entered at it before, and what its keys do.

use crate::app::complete::Completion;
use crate::app::text_view::is_word;
use crate::app::{App, Flow, Focus, Kind};
use crate::input::key::{Key, KeyCode, Mods};
use crate::input::setting::Settings;
use crate::view::layout::Layout;
use crate::view::screen::Terminal;

#[derive(Default)]
pub(super) struct Prompt {
    /// The character before the line: /, ? or :.
    prefix: char,
    text: Vec<char>,
    /// Where the next character goes: an index into `text`, at the
    /// start of a glyph.
    cursor: usize,
    /// Lines entered, oldest first.
    history: Vec<String>,
    /// The entry `text` shows; `history.len()` is the draft.
    pos: usize,
    /// What was typed before Up recalled an entry.
    draft: Vec<char>,
    /// Tab completion, at a prompt opened with it.
    completion: Option<Completion>,
}

/// What a key did at a prompt.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) enum Edit {
    Editing,
    /// Enter: the line, which the prompt remembered.
    Submit(String),
    Cancel,
}

/// Whether a key closes a prompt without running it: Esc, Ctrl+g and
/// Ctrl+c.
fn cancels(key: Key) -> bool {
    match key.code {
        KeyCode::Esc => true,
        KeyCode::Char('g' | 'c') => key.mods == Mods::CTRL,
        _ => false,
    }
}

impl Prompt {
    /// Empties the prompt, `prefix` before it.
    pub fn open(&mut self, prefix: char) {
        self.completion = None;
        self.start(prefix);
    }

    /// `open`, with Tab completing against `settings`.
    pub fn open_completing(&mut self, prefix: char, settings: &Settings) {
        self.completion = Some(Completion::new(settings));
        self.start(prefix);
    }

    fn start(&mut self, prefix: char) {
        self.open_with(prefix, "");
    }

    /// `open`, with `text` on the line, the cursor at its end.
    pub fn open_with(&mut self, prefix: char, text: &str) {
        self.prefix = prefix;
        self.set(text);
        self.pos = self.history.len();
    }

    /// The row: the prefix and the line.
    pub fn line(&self) -> String {
        std::iter::once(self.prefix)
            .chain(self.text.iter().copied())
            .collect()
    }

    /// The cursor's place in `line`, in characters.
    pub fn cursor(&self) -> usize {
        self.cursor + 1
    }

    /// The matches the last Tab found, while the line is as it left it,
    /// and which of them it filled in.
    pub fn listing(&self) -> Option<(&[String], Option<usize>)> {
        self.completion
            .as_ref()
            .and_then(|c| c.listing(&self.text()))
    }

    /// The line without its prefix.
    pub fn text(&self) -> String {
        self.text.iter().collect()
    }

    /// Replaces the line, the cursor at its end.
    fn set(&mut self, text: &str) {
        self.text = text.chars().collect();
        self.cursor = self.text.len();
    }

    /// Handles a key. Enter submits the line, remembered unless empty;
    /// Esc, Ctrl+g and Ctrl+c cancel. At a prompt that completes, Tab
    /// completes, and Enter on a listed command fills in the space
    /// after it instead. Any other key edits.
    pub fn key(&mut self, key: Key) -> Edit {
        let key = key.normalized();
        let text = self.text();
        if let Some(c) = &mut self.completion {
            let fill = match key.code {
                KeyCode::Tab => c.tab(&text),
                KeyCode::Enter => c.enter(&text),
                _ => {
                    c.clear();
                    None
                }
            };
            if let Some(t) = &fill {
                self.set(t);
            }
            // Tab with nothing to fill in leaves the line as it is.
            if fill.is_some() || key.code == KeyCode::Tab {
                return Edit::Editing;
            }
        }
        if cancels(key) {
            return Edit::Cancel;
        }
        if key.code == KeyCode::Enter {
            let line = match &self.completion {
                Some(c) => c.submitted(&text),
                None => text,
            };
            if !line.is_empty() {
                self.remember(&line);
            }
            return Edit::Submit(line);
        }
        self.edit(key);
        Edit::Editing
    }

    /// Handles the keys every prompt shares: Up and Down walk the
    /// history, Down past the newest entry restoring what was typed
    /// before Up; Left, Right, Home, End and the word keys move the
    /// cursor, by the glyph, a character and the marks on it; Backspace
    /// deletes the glyph before it, with Ctrl or Alt a word, as Ctrl+w
    /// does, and with Cmd the line before it, as Ctrl+u does; a
    /// character types. Any other key does nothing.
    fn edit(&mut self, key: Key) {
        let mods = key.mods;
        match key.code {
            KeyCode::Up => {
                if self.pos == 0 {
                    return;
                }
                if self.pos == self.history.len() {
                    self.draft.clone_from(&self.text);
                }
                self.pos -= 1;
                self.set(&self.history[self.pos].clone());
            }
            KeyCode::Down => {
                if self.pos == self.history.len() {
                    return;
                }
                self.pos += 1;
                if self.pos == self.history.len() {
                    self.set(&self.draft.iter().collect::<String>());
                } else {
                    self.set(&self.history[self.pos].clone());
                }
            }
            KeyCode::Left if mods == Mods::NONE => self.cursor = self.glyph_before(self.cursor),
            KeyCode::Right if mods == Mods::NONE => {
                self.cursor = self.glyph_after(self.cursor);
            }
            KeyCode::Left if mods == Mods::CMD => self.cursor = 0,
            KeyCode::Right if mods == Mods::CMD => self.cursor = self.text.len(),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.text.len(),
            KeyCode::Left if mods == Mods::CTRL || mods == Mods::ALT => self.word_left(),
            KeyCode::Right if mods == Mods::CTRL || mods == Mods::ALT => self.word_right(),
            KeyCode::Char('b') if mods == Mods::ALT => self.word_left(),
            KeyCode::Char('f') if mods == Mods::ALT => self.word_right(),
            KeyCode::Backspace if mods == Mods::NONE => {
                let start = self.glyph_before(self.cursor);
                self.text.drain(start..self.cursor);
                self.cursor = start;
            }
            KeyCode::Backspace if mods == Mods::CTRL || mods == Mods::ALT => self.delete_word(),
            KeyCode::Char('w') if mods == Mods::CTRL => self.delete_word(),
            KeyCode::Backspace if mods == Mods::CMD => self.delete_to_start(),
            KeyCode::Char('u') if mods == Mods::CTRL => self.delete_to_start(),
            KeyCode::Char(c) if !mods.contains(Mods::CTRL) => {
                self.text.insert(self.cursor, c);
                self.cursor += 1;
            }
            _ => {}
        }
    }

    /// Where each glyph starts, as the row draws them: a character
    /// that takes a cell, with the zero-width marks after it.
    fn glyph_starts(&self) -> Vec<bool> {
        let xs = Layout::default().cells(&self.text);
        (0..=self.text.len())
            .map(|i| i == self.text.len() || xs[i + 1] != xs[i])
            .collect()
    }

    /// The start of the glyph before `i`, or `i` at the start.
    fn glyph_before(&self, i: usize) -> usize {
        let starts = self.glyph_starts();
        let mut k = i.saturating_sub(1);
        while k > 0 && !starts[k] {
            k -= 1;
        }
        k
    }

    /// The start of the glyph after the one at `i`, or the end.
    fn glyph_after(&self, i: usize) -> usize {
        let starts = self.glyph_starts();
        let mut k = (i + 1).min(self.text.len());
        while k < self.text.len() && !starts[k] {
            k += 1;
        }
        k
    }

    /// The start of the word the cursor is in or after, backed up to
    /// the start of its glyph: a mark counts as a word character, and
    /// one on a space would be left alone.
    fn word_start(&self) -> usize {
        let mut i = self.cursor;
        while i > 0 && !is_word(self.text[i - 1]) {
            i -= 1;
        }
        while i > 0 && is_word(self.text[i - 1]) {
            i -= 1;
        }
        let starts = self.glyph_starts();
        while i > 0 && !starts[i] {
            i -= 1;
        }
        i
    }

    fn word_left(&mut self) {
        self.cursor = self.word_start();
    }

    /// To the end of the word the cursor is in or before.
    fn word_right(&mut self) {
        let mut i = self.cursor;
        while i < self.text.len() && !is_word(self.text[i]) {
            i += 1;
        }
        while i < self.text.len() && is_word(self.text[i]) {
            i += 1;
        }
        let starts = self.glyph_starts();
        while i < self.text.len() && !starts[i] {
            i += 1;
        }
        self.cursor = i;
    }

    fn delete_word(&mut self) {
        let start = self.word_start();
        self.text.drain(start..self.cursor);
        self.cursor = start;
    }

    fn delete_to_start(&mut self) {
        self.text.drain(..self.cursor);
        self.cursor = 0;
    }

    /// Types `text` as one line at the cursor: a control, a newline
    /// above all, is dropped rather than pressed, so a paste cannot
    /// submit. The listing goes, as with any key but Tab.
    pub fn paste(&mut self, text: &str) {
        if let Some(c) = &mut self.completion {
            c.clear();
        }
        let typed: Vec<char> = text.chars().filter(|c| !c.is_control()).collect();
        self.text
            .splice(self.cursor..self.cursor, typed.iter().copied());
        self.cursor += typed.len();
    }

    /// Adds `entry` to the history unless it repeats the newest.
    fn remember(&mut self, entry: &str) {
        if self.history.last().map(String::as_str) != Some(entry) {
            self.history.push(entry.to_string());
        }
    }
}

impl<T: Terminal> App<T> {
    /// The prompt open, if one is.
    pub(super) fn prompt(&self) -> Option<&Prompt> {
        match self.focus {
            Focus::Viewing => None,
            Focus::Prompting(Kind::Search { .. }) => Some(&self.search),
            Focus::Prompting(Kind::Command) => Some(&self.command),
            Focus::Prompting(Kind::Filter) => Some(&self.query),
        }
    }

    pub(super) fn prompt_mut(&mut self) -> Option<&mut Prompt> {
        match self.focus {
            Focus::Viewing => None,
            Focus::Prompting(Kind::Search { .. }) => Some(&mut self.search),
            Focus::Prompting(Kind::Command) => Some(&mut self.command),
            Focus::Prompting(Kind::Filter) => Some(&mut self.query),
        }
    }

    /// Gives a key to the open prompt. A line submitted runs once the
    /// prompt has closed, so the status line's row is the text's again
    /// by then. The & prompt's keys are its own.
    pub(super) fn prompt_key(&mut self, kind: Kind, key: Key) -> Flow {
        let edit = match kind {
            Kind::Search { .. } => self.search.key(key),
            Kind::Command => self.command.key(key),
            Kind::Filter => return self.filter_key(key),
        };
        match edit {
            Edit::Editing => Flow::Continue,
            Edit::Cancel => {
                self.focus(Focus::Viewing);
                Flow::Continue
            }
            Edit::Submit(line) => {
                self.focus(Focus::Viewing);
                match kind {
                    Kind::Search { back } => self.search_submit(&line, back),
                    Kind::Command => self.run_command(&line),
                    Kind::Filter => unreachable!("handled above"),
                }
            }
        }
    }
}
