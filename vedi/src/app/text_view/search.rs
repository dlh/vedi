//! The search in the text: its pattern, direction and highlight.

use crate::app::text_view::{Row, TextView, within};
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
    /// The cursor's input, and the anchor's: a reload keeps each in
    /// its own.
    cur_part: usize,
    anchor_part: Option<usize>,
    /// The cursor was on screen, so it stays on screen when the screen
    /// has shrunk since.
    shown: bool,
    top: Row,
    xoff: usize,
    matcher: Matcher,
    backward: bool,
    highlight: bool,
    count: Option<Count>,
    /// The text's edition then: a count from another is made again.
    edition: usize,
}

/// The pattern's matches, counted when a search lands.
#[derive(Copy, Clone, Debug)]
pub(super) struct Count {
    /// Matches up to the cursor: the one landed on, from 1.
    at: usize,
    of: usize,
    /// Lines counted; those after join as they come.
    scanned: usize,
    /// Where the search landed: n and N step `at` while the cursor is
    /// still there.
    landed: Option<Pos>,
}

impl TextView {
    pub fn matcher(&self) -> &Matcher {
        &self.matcher
    }

    pub fn highlight(&self) -> bool {
        self.highlight
    }

    /// The match the cursor landed on, from 1, and how many there are,
    /// while the highlight is on.
    pub fn count(&self) -> Option<(usize, usize)> {
        self.count.filter(|_| self.highlight).map(|c| (c.at, c.of))
    }

    pub fn backward(&self) -> bool {
        self.backward
    }

    /// Counts the matches, the cursor's place among them; on a match,
    /// n and N step from here.
    pub(super) fn recount(&mut self) {
        let cur = self.cur;
        let (at, of) = search::position(&self.buf, &self.matcher, cur);
        self.buf.text(cur.line, &mut self.scratch);
        let on_match = self.matcher.find(&self.scratch, cur.col) == Some(cur.col);
        self.count = Some(Count {
            at,
            of,
            scanned: self.buf.len(),
            landed: on_match.then_some(cur),
        });
    }

    /// Counts the lines read since the count was made.
    pub(super) fn extend_count(&mut self) {
        let n = self.buf.len();
        if let Some(c) = &mut self.count
            && c.scanned < n
        {
            c.of += search::count(&self.buf, &self.matcher, c.scanned..n);
            c.scanned = n;
        }
    }

    pub fn origin(&mut self) -> Origin {
        Origin {
            cur: self.cur,
            anchor: self.anchor,
            cur_part: self.buf.part_at(self.cur.line),
            anchor_part: self.anchor.map(|a| self.buf.part_at(a.line)),
            shown: self.cursor_visible(),
            top: self.top(),
            xoff: self.xoff,
            matcher: self.matcher.clone(),
            backward: self.backward,
            highlight: self.highlight,
            count: self.count,
            edition: self.edition,
        }
    }

    /// Puts the view back where `o` was. The text may have grown or
    /// been reloaded since: the cursor and the anchor are clamped to
    /// it, the count caught up or made again, and the top row is
    /// snapped when next read.
    pub fn back_to(&mut self, o: &Origin) {
        self.cur = o.cur;
        self.anchor = o.anchor;
        self.top = o.top;
        self.xoff = o.xoff;
        self.matcher = o.matcher.clone();
        self.backward = o.backward;
        self.highlight = o.highlight;
        self.count = o.count;
        if self.buf.is_empty() {
            self.cur = Pos::default();
            self.anchor = None;
        } else {
            self.cur.line = within(&self.buf, o.cur_part, self.cur.line);
            self.cur = self.clamped(self.cur);
            if let (Some(mut a), Some(part)) = (self.anchor, o.anchor_part) {
                a.line = within(&self.buf, part, a.line);
                self.anchor = Some(self.clamped(a));
            }
        }
        if self.edition != o.edition {
            if self.count.is_some() {
                self.recount();
            }
        } else {
            self.extend_count();
        }
        if o.shown {
            self.scroll_to_cursor();
        }
    }

    /// `p` within the text, which is not empty.
    fn clamped(&mut self, p: Pos) -> Pos {
        let line = p.line.min(self.buf.len() - 1);
        Pos {
            line,
            col: p.col.min(self.end_col(line)),
        }
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
                self.count = None;
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
        self.count = None;
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
        self.recount();
    }

    pub fn find_next(&mut self) -> Outcome {
        self.find(self.backward, true)
    }

    pub fn find_prev(&mut self) -> Outcome {
        self.find(!self.backward, true)
    }

    /// Goes to the next match before or after the cursor. A match at
    /// the cursor counts going forward unless `skip`; backward always
    /// skips. A match off the screen comes in on the top row. A search
    /// that finds nothing still turns the highlight on, with nothing
    /// to draw, so the status line counts it as 0/0.
    fn find(&mut self, backward: bool, skip: bool) -> Outcome {
        if self.matcher.is_empty() {
            return Outcome::NoPattern;
        }
        let from = self.cur;
        let found = if backward {
            search::prev(&self.buf, &self.matcher, from)
        } else {
            search::next(&self.buf, &self.matcher, from, skip)
        };
        self.highlight = true;
        let Some(f) = found else {
            self.count = Some(Count {
                at: 0,
                of: 0,
                scanned: self.buf.len(),
                landed: None,
            });
            return Outcome::NotFound;
        };
        self.cur = f.pos;
        self.drop();
        // Stepping from the match landed on spares a pass over the text.
        match self.count {
            Some(c) if c.landed == Some(from) && c.of > 0 && f.pos != from => {
                let at = if backward {
                    if c.at <= 1 { c.of } else { c.at - 1 }
                } else {
                    c.at % c.of + 1
                };
                self.count = Some(Count {
                    at,
                    landed: Some(f.pos),
                    ..c
                });
            }
            Some(c) if c.landed == Some(from) && f.pos == from => {}
            _ => self.recount(),
        }
        self.cursor_row_to_top();
        self.scroll_to_cursor();
        if f.wrapped {
            Outcome::Wrapped
        } else {
            Outcome::Found
        }
    }
}
