//! The search in the text: its pattern, direction and highlight.

use crate::app::text_view::{Row, TextView};
use crate::text::buffer::Pos;
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

/// Where a view was when a search prompt opened: searched from as the
/// line is typed, and gone back to.
pub struct Origin {
    cur: Pos,
    anchor: Option<Pos>,
    top: Row,
    xoff: usize,
    matcher: Matcher,
    backward: bool,
    highlight: bool,
}

impl TextView {
    pub fn matcher(&self) -> &Matcher {
        &self.matcher
    }

    pub fn highlight(&self) -> bool {
        self.highlight
    }

    pub fn origin(&mut self) -> Origin {
        Origin {
            cur: self.cur,
            anchor: self.anchor,
            top: self.top(),
            xoff: self.xoff,
            matcher: self.matcher.clone(),
            backward: self.backward,
            highlight: self.highlight,
        }
    }

    /// Puts the view back where `o` was. The text may have been
    /// reloaded since, so the cursor is clamped to it; the top row is
    /// snapped when next read.
    pub fn back_to(&mut self, o: &Origin) {
        self.cur = o.cur;
        self.anchor = o.anchor;
        self.top = o.top;
        self.xoff = o.xoff;
        self.matcher = o.matcher.clone();
        self.backward = o.backward;
        self.highlight = o.highlight;
        let n = self.buf.len();
        if n == 0 {
            self.cur = Pos::default();
            self.anchor = None;
            return;
        }
        self.cur.line = self.cur.line.min(n - 1);
        self.cur.col = self.cur.col.min(self.end_col(self.cur.line));
    }

    /// Searches for `m` from where `o` was, as `search` does from the
    /// cursor.
    pub fn search_from(&mut self, o: &Origin, m: Matcher, backward: bool) -> Outcome {
        self.back_to(o);
        self.search(Some(m), backward)
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

    /// Highlights `m`'s matches, the cursor where it is; n and N go
    /// forward from it.
    pub fn set_matcher(&mut self, m: Matcher) {
        self.highlight = !m.is_empty();
        self.matcher = m;
        self.backward = false;
    }

    /// Lands on `pos`, clamped to the text, its row in the middle of
    /// the screen or as near as the ends allow, with `m` highlighted as
    /// a search for it would leave it.
    pub fn land(&mut self, m: Matcher, pos: Pos) {
        self.set_matcher(m);
        let n = self.buf.len();
        if n == 0 {
            return;
        }
        let line = pos.line.min(n - 1);
        let col = pos.col.min(self.line_len(line));
        self.cur = Pos { line, col };
        self.drop();
        self.center_cursor_row();
        self.scroll_to_cursor();
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
