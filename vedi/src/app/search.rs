//! The / and ? prompts, and n and N.

use crate::app::prompt::PromptEdit;
use crate::app::text_view::Outcome;
use crate::app::{App, Focus};
use crate::input::key::Key;
use crate::text::search::Matcher;
use crate::view::screen::Terminal;

impl<T: Terminal> App<T> {
    /// Opens the / prompt, or the ? prompt if `back`.
    pub(super) fn open_search(&mut self, back: bool) {
        self.focus(Focus::Searching { back });
        self.mouse.end_drag();
        self.search.open();
    }

    /// Edits the / and ? prompts. Enter searches; with nothing typed it
    /// repeats the last pattern the prompt's way.
    pub(super) fn search_key(&mut self, key: Key, back: bool) {
        match self.search.edit(key) {
            PromptEdit::Editing => {}
            PromptEdit::Cancel => self.focus(Focus::Viewing),
            PromptEdit::Submit => {
                self.focus(Focus::Viewing);
                let pattern = if self.search.is_empty() {
                    None
                } else {
                    let query = self.search.text();
                    self.search.remember(&query);
                    Some(Matcher::new(&query))
                };
                let found = self.view.search(pattern, back);
                self.report(found);
            }
        }
    }

    /// Puts what a search found on the status line.
    pub(super) fn report(&mut self, found: Outcome) {
        self.status = match found {
            Outcome::NoPattern => Some("no search pattern".into()),
            Outcome::NotFound => Some(format!("not found: {}", self.view.matcher().pattern())),
            Outcome::Wrapped => Some("search wrapped".into()),
            Outcome::Found => return,
        };
    }
}
