//! Reading the input again: on the key, or when the file changes on
//! disk.

use crate::app::App;
use crate::app::help::TextView;
use crate::app::motion::Row;
use crate::text::ansi::ViewStyle;
use crate::text::buffer::{Buffer, Pos};
use crate::text::reader::Gen;
use crate::view::screen::Terminal;

/// A reload being read, with its reader's generation.
pub(super) struct Pending {
    pub(super) generation: Gen,
    pub(super) buf: Buffer,
}

impl<T: Terminal> App<T> {
    /// Reads the input again. One reload runs at a time; a change
    /// during one asks for another after it. Only the key reports that
    /// it cannot.
    pub(super) fn reload(&mut self, by_key: bool) {
        let Some(open) = self.open.as_mut() else {
            if by_key {
                self.status = Some("not a file".into());
            }
            return;
        };
        if self.next.is_some() {
            self.dirty = true;
            return;
        }
        let generation = self.generation.wrapping_add(1);
        match open(generation) {
            Ok(buf) => {
                self.next = Some(Pending { generation, buf });
                self.swap_if_done();
            }
            Err(e) => {
                if by_key {
                    self.status = Some(format!("reload failed: {e}"));
                }
            }
        }
    }

    /// Puts a finished reload in the text's place, and starts the
    /// reload a change asked for meanwhile.
    pub(super) fn swap_if_done(&mut self) {
        let Some(p) = self.next.take_if(|p| p.buf.finished().is_some()) else {
            return;
        };
        self.generation = p.generation;
        self.put(p.buf);
        if self.dirty {
            self.dirty = false;
            self.reload(false);
        }
    }

    /// Puts `buf`, decoded as the view style has it, under the view, or
    /// under the view help set aside. The old buffer's files close with
    /// it.
    fn put(&mut self, mut buf: Buffer) {
        buf.set_literal(self.settings.view_style == ViewStyle::Raw);
        self.change_text(|b| *b = buf);
    }

    /// Changes the text under the view, or under the view help set
    /// aside, and keeps that view's place as `reload_view` does.
    pub(super) fn change_text(&mut self, f: impl FnOnce(&mut Buffer)) {
        if let Some(t) = &mut self.text {
            let place = Place::of(&t.buf, t.cur, t.anchor);
            f(&mut t.buf);
            t.reload(place, &mut self.scratch);
            return;
        }
        let place = Place::of(&self.buf, self.cur, self.anchor);
        f(&mut self.buf);
        self.text_changed(place);
    }

    /// Keeps the view's place after the lines changed under it, as
    /// `reload_view` does, then drops the layouts and scrolls to the
    /// cursor.
    fn text_changed(&mut self, place: Place) {
        reload_view(
            &self.buf,
            &mut self.scratch,
            &mut self.cur,
            &mut self.top,
            &mut self.anchor,
            place,
        );
        self.laid.clear();
        self.scroll_to_cursor();
    }
}

impl TextView {
    /// `reload_view` for the view help set aside; help's return scrolls
    /// to the cursor.
    pub(super) fn reload(&mut self, place: Place, scratch: &mut Vec<char>) {
        let TextView {
            buf,
            cur,
            top,
            anchor,
            ..
        } = self;
        reload_view(buf, scratch, cur, top, anchor, place);
    }
}

/// Where a view was before its text changed, for `reload_view`.
#[derive(Copy, Clone, Debug)]
pub(super) struct Place {
    /// The cursor was on the last line.
    pub(super) at_end: bool,
    /// The cursor's input, and the anchor's.
    pub(super) cur: usize,
    pub(super) anchor: Option<usize>,
}

impl Place {
    pub(super) fn of(buf: &Buffer, cur: Pos, anchor: Option<Pos>) -> Place {
        Place {
            at_end: cur.line + 1 >= buf.len(),
            cur: buf.part_at(cur.line),
            anchor: anchor.map(|a| buf.part_at(a.line)),
        }
    }
}

/// Keeps a view's place in text that changed under it: a cursor that
/// was on the last line goes to the new last line, and every position
/// is clamped, the line to the last and the column to the line's end.
/// The cursor and the anchor each stay in the input they were in: a
/// file above that shrank would otherwise hand them to the next. A
/// top past the new end starts over from the first row, so a text
/// shorter than the screen fills it from the top; a separator top
/// stays one, which `snap` drops if the line no longer starts an
/// input.
pub(super) fn reload_view(
    buf: &Buffer,
    scratch: &mut Vec<char>,
    cur: &mut Pos,
    top: &mut Row,
    anchor: &mut Option<Pos>,
    place: Place,
) {
    let n = buf.len();
    let clamp = |p: Pos, scratch: &mut Vec<char>| {
        if n == 0 {
            return Pos::default();
        }
        let line = p.line.min(n - 1);
        buf.text(line, scratch);
        Pos {
            line,
            col: p.col.min(scratch.len()),
        }
    };
    // `line` within input `part`; an input that emptied yields to the
    // next.
    let within = |part: usize, line: usize| {
        let parts = buf.parts();
        let Some(&start) = parts.get(part).filter(|_| n > 0) else {
            return line;
        };
        let lo = start.min(n - 1);
        let hi = parts.get(part + 1).map_or(n, |&e| e.max(lo + 1)).min(n) - 1;
        line.clamp(lo, hi)
    };
    let line = if place.at_end {
        n.saturating_sub(1)
    } else {
        cur.line
    };
    *cur = clamp(
        Pos {
            line: within(place.cur, line),
            col: cur.col,
        },
        scratch,
    );
    if top.line >= n {
        *top = Row::sep(0);
    } else if let Some(col) = top.col {
        *top = Row::text(clamp(
            Pos {
                line: top.line,
                col,
            },
            scratch,
        ));
    }
    if let (Some(a), Some(part)) = (anchor, place.anchor) {
        a.line = within(part, a.line);
        *a = clamp(*a, scratch);
    }
}
