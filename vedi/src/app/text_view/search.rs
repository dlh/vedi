//! The search in the text: its pattern, direction and highlight.

use crate::app::text_view::TextView;
use crate::text::search::{self, Matcher};

/// What a search found.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Outcome {
    NoPattern,
    NotFound,
    Found,
    /// Found, past the end or the start.
    Wrapped,
}

impl TextView {
    pub fn matcher(&self) -> &Matcher {
        &self.matcher
    }

    pub fn highlight(&self) -> bool {
        self.highlight
    }

    /// Searches for `pattern`, or for the last one again, from the
    /// cursor, `backward` or not; n and N then follow that direction.
    pub fn search(&mut self, pattern: Option<Matcher>, backward: bool) -> Outcome {
        self.backward = backward;
        match pattern {
            Some(m) => {
                self.matcher = m;
                self.find(backward, false)
            }
            None => self.find(backward, true),
        }
    }

    pub fn find_next(&mut self) -> Outcome {
        self.find(self.backward, true)
    }

    pub fn find_prev(&mut self) -> Outcome {
        self.find(!self.backward, true)
    }

    /// Goes to the next match before or after the cursor. A match at
    /// the cursor counts going forward unless `skip`; backward always
    /// skips. A match off the screen comes in on the top row.
    fn find(&mut self, backward: bool, skip: bool) -> Outcome {
        if self.matcher.is_empty() {
            return Outcome::NoPattern;
        }
        let found = if backward {
            search::prev(&self.buf, &self.matcher, self.cur)
        } else {
            search::next(&self.buf, &self.matcher, self.cur, skip)
        };
        let Some(f) = found else {
            return Outcome::NotFound;
        };
        self.cur = f.pos;
        self.drop();
        self.highlight = true;
        self.cursor_row_to_top();
        self.scroll_to_cursor();
        if f.wrapped {
            Outcome::Wrapped
        } else {
            Outcome::Found
        }
    }
}
