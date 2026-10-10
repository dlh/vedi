//! A line being typed on the status row, and the history of the lines
//! entered at it before.

use crate::input::key::{Key, KeyCode, Mods};

#[derive(Default)]
pub(super) struct Prompt {
    pub text: Vec<char>,
    /// Lines entered, oldest first.
    history: Vec<String>,
    /// The entry `text` shows; `history.len()` is the draft.
    pos: usize,
    /// What was typed before Up recalled an entry.
    draft: Vec<char>,
}

/// What a key did at a prompt.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(super) enum PromptEdit {
    Editing,
    Submit,
    Cancel,
}

/// Whether a key closes a prompt without running it: Esc, Ctrl+g and
/// Ctrl+c.
pub(super) fn cancels(key: Key) -> bool {
    match key.code {
        KeyCode::Esc => true,
        KeyCode::Char('g' | 'c') => key.mods == Mods::CTRL,
        _ => false,
    }
}

impl Prompt {
    /// Empties the prompt.
    pub fn open(&mut self) {
        self.text.clear();
        self.pos = self.history.len();
    }

    pub fn text(&self) -> String {
        self.text.iter().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Handles the keys every prompt shares: Up and Down walk the
    /// history, Down past the newest entry restoring what was typed
    /// before Up; Backspace deletes; a character types. Enter submits
    /// and `cancels` cancels; any other key does nothing.
    pub fn edit(&mut self, key: Key) -> PromptEdit {
        let key = key.normalized();
        if cancels(key) {
            return PromptEdit::Cancel;
        }
        match key.code {
            KeyCode::Enter => return PromptEdit::Submit,
            KeyCode::Up => {
                if self.pos == 0 {
                    return PromptEdit::Editing;
                }
                if self.pos == self.history.len() {
                    self.draft.clone_from(&self.text);
                }
                self.pos -= 1;
                self.text = self.history[self.pos].chars().collect();
            }
            KeyCode::Down => {
                if self.pos == self.history.len() {
                    return PromptEdit::Editing;
                }
                self.pos += 1;
                if self.pos == self.history.len() {
                    self.text.clone_from(&self.draft);
                } else {
                    self.text = self.history[self.pos].chars().collect();
                }
            }
            KeyCode::Backspace => {
                self.text.pop();
            }
            KeyCode::Char(c) if !key.mods.contains(Mods::CTRL) => self.text.push(c),
            _ => {}
        }
        PromptEdit::Editing
    }

    /// Adds the text to the history unless it repeats the newest entry.
    pub fn remember(&mut self) {
        let text = self.text();
        if self.history.last() != Some(&text) {
            self.history.push(text);
        }
    }
}
