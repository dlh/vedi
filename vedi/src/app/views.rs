//! The text views, one shown over the rest: the text, the filter's
//! list over it, help over either.

use std::mem;

use crate::app::text_view::TextView;

/// What a view is, in the order they stack: a view is raised only over
/// a lower role.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Role {
    Text,
    List,
    Help,
}

pub(super) struct Views {
    /// The view shown.
    pub(super) shown: TextView,
    role: Role,
    /// The views the shown one is over, lowest first: the text, then
    /// the list when help is over it.
    under: Vec<(Role, TextView)>,
}

impl Views {
    pub(super) fn new(text: TextView) -> Views {
        Views {
            shown: text,
            role: Role::Text,
            under: Vec::with_capacity(2),
        }
    }

    /// The shown view's role.
    #[inline]
    pub(super) fn role(&self) -> Role {
        self.role
    }

    /// Whether a view of `role` is shown or under the shown one.
    #[inline]
    pub(super) fn holds(&self, role: Role) -> bool {
        self.role == role || self.under.iter().any(|(r, _)| *r == role)
    }

    /// The text's view, shown or under.
    #[inline]
    pub(super) fn text(&self) -> &TextView {
        self.under.first().map_or(&self.shown, |(_, v)| v)
    }

    #[inline]
    pub(super) fn text_mut(&mut self) -> &mut TextView {
        self.under.first_mut().map_or(&mut self.shown, |(_, v)| v)
    }

    /// The text's buffer and the view of `role`, which must be held.
    pub(super) fn text_and(&mut self, role: Role) -> (&TextView, &mut TextView) {
        debug_assert!(self.holds(role) && role != Role::Text);
        if self.role == role {
            (&self.under[0].1, &mut self.shown)
        } else {
            let (text, rest) = self.under.split_at_mut(1);
            let (_, v) = rest.iter_mut().find(|(r, _)| *r == role).unwrap();
            (&text[0].1, v)
        }
    }

    /// Shows `view` over the shown one, which it sets aside.
    pub(super) fn raise(&mut self, role: Role, view: TextView) {
        debug_assert!(role > self.role);
        let aside = mem::replace(&mut self.shown, view);
        self.under.push((self.role, aside));
        self.role = role;
    }

    /// Takes the shown view off, which must be of `role`, and shows the
    /// one it was over.
    pub(super) fn lower(&mut self, role: Role) -> TextView {
        debug_assert_eq!(self.role, role);
        let (r, v) = self.under.pop().expect("a view is over the text");
        self.role = r;
        mem::replace(&mut self.shown, v)
    }
}
