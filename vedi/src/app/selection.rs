use crate::app::App;
use crate::text::buffer::Pos;
use crate::view::screen::Terminal;

impl<T: Terminal> App<T> {
    /// The ordered selection bounds; `None` when there is no anchor or
    /// it equals the cursor.
    pub(super) fn selection(&self) -> Option<(Pos, Pos)> {
        let a = self.anchor?;
        if a == self.cur {
            return None;
        }
        Some((a.min(self.cur), a.max(self.cur)))
    }

    /// Anchors the selection at the cursor if there is none, so a
    /// motion extends it.
    pub(super) fn extend(&mut self) {
        self.anchor.get_or_insert(self.cur);
    }

    /// Clears the selection, unless the mark is set.
    pub(super) fn drop(&mut self) {
        if !self.marking {
            self.anchor = None;
        }
    }

    pub(super) fn select_all(&mut self) {
        self.anchor = Some(Pos::default());
        self.cur = self.end_pos();
    }

    /// Clears the selection; with none, the search highlight.
    pub(super) fn clear_selection(&mut self) {
        if self.anchor.is_some() {
            self.anchor = None;
            self.marking = false;
        } else {
            self.highlight = false;
        }
    }

    /// Sets the mark, so motions extend the selection; again, ends it.
    pub(super) fn set_mark(&mut self) {
        if self.marking {
            self.anchor = None;
            self.marking = false;
        } else {
            self.marking = true;
            self.extend();
        }
    }
}
