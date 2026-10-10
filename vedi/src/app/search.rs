//! The / and ? prompts, and n and N.

use crate::app::motion::Row;
use crate::app::prompt::PromptEdit;
use crate::app::{App, Focus};
use crate::input::key::Key;
use crate::text::search::{self, Found, Matcher};
use crate::view::screen::Terminal;

impl<T: Terminal> App<T> {
    /// Opens the / prompt, or the ? prompt if `back`.
    pub(super) fn open_search(&mut self, back: bool) {
        self.focus = Focus::Searching { back };
        self.dragging = false;
        self.query.open();
    }

    /// Edits the / and ? prompts. Enter searches; with nothing typed it
    /// repeats the last pattern the prompt's way.
    pub(super) fn search_key(&mut self, key: Key, back: bool) {
        match self.query.edit(key) {
            PromptEdit::Editing => {}
            PromptEdit::Cancel => self.focus = self.shown(),
            PromptEdit::Submit => {
                self.focus = self.shown();
                self.backward = back;
                if self.query.is_empty() {
                    self.find(back, true);
                    return;
                }
                self.query.remember();
                self.matcher = Matcher::new(&self.query.text());
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
        if self.matcher.is_empty() {
            self.status = Some("no search pattern".into());
            return;
        }
        let found = if backward {
            search::prev(&self.buf, &self.matcher, self.cur)
        } else {
            search::next(&self.buf, &self.matcher, self.cur, skip)
        };
        self.jump_to(found);
    }

    fn jump_to(&mut self, found: Option<Found>) {
        let Some(f) = found else {
            self.status = Some(format!("not found: {}", self.matcher.pattern()));
            return;
        };
        self.cur = f.pos;
        self.drop();
        self.highlight = true;
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
