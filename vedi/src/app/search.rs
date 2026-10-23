//! The / and ? prompts, and n and N.

use crate::app::text_view::Outcome;
use crate::app::{App, Flow, Focus, Kind};
use crate::text::search::Matcher;
use crate::view::screen::Terminal;

impl<T: Terminal> App<T> {
    /// Opens the / prompt, or the ? prompt if `back`.
    pub(super) fn open_search(&mut self, back: bool) {
        self.focus(Focus::Prompting(Kind::Search { back }));
        self.mouse.end_drag();
        self.search.open(if back { '?' } else { '/' });
    }

    /// Searches for `line`; empty, it repeats the last pattern the
    /// prompt's way.
    pub(super) fn search_submit(&mut self, line: &str, back: bool) -> Flow {
        let pattern = (!line.is_empty()).then(|| Matcher::new(line));
        let found = self.view.search(pattern, back);
        self.report(found)
    }

    /// What to say of what a search found.
    pub(super) fn report(&self, found: Outcome) -> Flow {
        match found {
            Outcome::NoPattern => Flow::Say("no search pattern".into()),
            Outcome::NotFound => Flow::Say(format!("not found: {}", self.view.matcher().pattern())),
            Outcome::Wrapped => Flow::Say("search wrapped".into()),
            Outcome::Found => Flow::Continue,
        }
    }
}
