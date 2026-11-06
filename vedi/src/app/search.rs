//! The / and ? prompts, searching as the line is typed, and n and N.

use crate::app::text_view::Outcome;
use crate::app::{App, Flow, Focus, Kind};
use crate::text::search::Matcher;
use crate::view::screen::Terminal;

impl<T: Terminal> App<T> {
    /// Opens the / prompt, or the ? prompt if `back`.
    pub(super) fn open_search(&mut self, back: bool) {
        self.focus(Focus::Prompting(Kind::Search { back }));
        self.end_drag();
        self.origin = Some(self.views.shown.origin());
        self.search.open(if back { '?' } else { '/' });
    }

    /// Searches for the line as it is now, from the origin; empty, it
    /// shows the origin as it was.
    pub(super) fn search_typed(&mut self, back: bool) {
        let Some(o) = &self.origin else {
            return;
        };
        let line = self.search.text();
        if line.is_empty() {
            self.views.shown.back_to(o);
        } else {
            self.views.shown.search_from(o, Matcher::new(&line), back);
        }
    }

    /// Enter: searches for `line` from the origin once more, so a
    /// line read since counts; empty, it repeats the last pattern the
    /// prompt's way.
    pub(super) fn search_submit(&mut self, line: &str, back: bool) -> Flow {
        let Some(o) = self.origin.take() else {
            return Flow::Continue;
        };
        self.views.shown.back_to(&o);
        let pattern = (!line.is_empty()).then(|| Matcher::new(line));
        let found = self.views.shown.search(pattern, back);
        self.report(found)
    }

    /// Esc, Ctrl+g or Ctrl+c: the view as it was when the prompt
    /// opened.
    pub(super) fn search_cancel(&mut self) {
        if let Some(o) = self.origin.take() {
            self.views.shown.back_to(&o);
        }
    }

    /// What to say of what a search found.
    pub(super) fn report(&self, found: Outcome) -> Flow {
        match found {
            Outcome::NoPattern => Flow::Fail("no search pattern".into()),
            Outcome::NotFound => Flow::Fail(format!(
                "not found: {}",
                self.views.shown.matcher().pattern()
            )),
            Outcome::Wrapped => Flow::Say("search wrapped".into()),
            Outcome::Found => Flow::Continue,
        }
    }
}
