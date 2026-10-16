//! The / and ? prompts, and n and N.

use crate::app::motion::Row;
use crate::app::prompt::{Prompt, PromptEdit};
use crate::app::{App, Focus};
use crate::input::key::Key;
use crate::text::search::{self, Found, Matcher};
use crate::view::screen::Terminal;

pub(super) struct State {
    /// The / and ? prompts' line.
    pub query: Prompt,
    pub matcher: Matcher,
    /// The last search was ?, and so n goes up and N down.
    pub backward: bool,
    pub highlight: bool,
}

impl State {
    pub fn new() -> State {
        State {
            query: Prompt::default(),
            matcher: Matcher::new(""),
            backward: false,
            highlight: false,
        }
    }
}

impl<T: Terminal> App<T> {
    /// Opens the / prompt, or the ? prompt if `back`.
    pub(super) fn open_search(&mut self, back: bool) {
        self.focus = Focus::Searching { back };
        self.mouse.end_drag();
        self.search.query.open();
    }

    /// Edits the / and ? prompts. Enter searches; with nothing typed it
    /// repeats the last pattern the prompt's way.
    pub(super) fn search_key(&mut self, key: Key, back: bool) {
        match self.search.query.edit(key) {
            PromptEdit::Editing => {}
            PromptEdit::Cancel => self.focus = self.shown(),
            PromptEdit::Submit => {
                self.focus = self.shown();
                let s = &mut self.search;
                s.backward = back;
                if s.query.is_empty() {
                    self.find(back, true);
                    return;
                }
                let query = s.query.text();
                s.query.remember(&query);
                s.matcher = Matcher::new(&query);
                self.find(back, false);
            }
        }
    }

    /// What the prompt was opened over: the text, or help.
    fn shown(&self) -> Focus {
        if self.text.is_some() {
            Focus::Helping
        } else {
            Focus::Viewing
        }
    }

    /// Goes to the next match before or after the cursor. A match at
    /// the cursor counts going forward unless `skip`; backward always
    /// skips.
    pub(super) fn find(&mut self, backward: bool, skip: bool) {
        if self.search.matcher.is_empty() {
            self.status = Some("no search pattern".into());
            return;
        }
        let m = &self.search.matcher;
        let found = if backward {
            search::prev(&self.buf, m, self.cur)
        } else {
            search::next(&self.buf, m, self.cur, skip)
        };
        self.jump_to(found);
    }

    fn jump_to(&mut self, found: Option<Found>) {
        let Some(f) = found else {
            self.status = Some(format!("not found: {}", self.search.matcher.pattern()));
            return;
        };
        self.cur = f.pos;
        self.drop();
        self.search.highlight = true;
        if f.wrapped {
            self.status = Some("search wrapped".into());
        }
        self.scroll_match_to_top();
        self.scroll_to_cursor();
    }

    /// Makes the cursor's row the top one if it is off the screen,
    /// stopping at the last screenful.
    fn scroll_match_to_top(&mut self) {
        let rows = self.text_rows();
        if rows == 0 || self.buf.is_empty() {
            return;
        }
        let crow = self.snap(Row::text(self.cur));
        let top = self.snap(self.top);
        if crow >= top {
            let mut bottom = top;
            for _ in 0..rows - 1 {
                bottom = self.next_row(bottom);
            }
            if bottom >= crow {
                return;
            }
        }
        self.place_top(crow);
    }
}
