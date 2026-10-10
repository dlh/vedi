use std::ops::BitOr;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum Color {
    /// The terminal's own foreground or background.
    #[default]
    Default,
    Palette(u8),
    Rgb(u8, u8, u8),
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Attrs(u8);

impl Attrs {
    pub const BOLD: Attrs = Attrs(1);
    pub const DIM: Attrs = Attrs(1 << 1);
    pub const ITALIC: Attrs = Attrs(1 << 2);
    pub const UNDERLINE: Attrs = Attrs(1 << 3);
    pub const REVERSE: Attrs = Attrs(1 << 4);
    pub const STRIKE: Attrs = Attrs(1 << 5);

    /// Whether every attribute in `other` is set.
    pub const fn contains(self, other: Attrs) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn insert(&mut self, other: Attrs) {
        self.0 |= other.0;
    }

    pub fn remove(&mut self, other: Attrs) {
        self.0 &= !other.0;
    }
}

impl BitOr for Attrs {
    type Output = Attrs;

    fn bitor(self, other: Attrs) -> Attrs {
        Attrs(self.0 | other.0)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub attrs: Attrs,
}

impl Style {
    pub const fn fg(self, fg: Color) -> Style {
        Style { fg, ..self }
    }

    pub const fn bg(self, bg: Color) -> Style {
        Style { bg, ..self }
    }

    pub const fn bold(self) -> Style {
        self.with(Attrs::BOLD)
    }

    pub const fn dim(self) -> Style {
        self.with(Attrs::DIM)
    }

    pub const fn reverse(self) -> Style {
        self.with(Attrs::REVERSE)
    }

    /// Adds `attrs` to those already set.
    pub const fn with(self, attrs: Attrs) -> Style {
        Style {
            attrs: Attrs(self.attrs.0 | attrs.0),
            ..self
        }
    }
}

const PLAIN: Style = Style {
    fg: Color::Default,
    bg: Color::Default,
    attrs: Attrs(0),
};

/// Selected cells, whatever the text's own color.
pub const SELECTION: Style = PLAIN.reverse();
/// Black on bright yellow, which themes that mute the base palette tend
/// to leave vivid.
pub const MATCH: Style = PLAIN.fg(Color::Palette(0)).bg(Color::Palette(11));
/// A dark background, which a light cursor shows against as it does not
/// against `MATCH`.
pub const CUR_MATCH: Style = PLAIN.fg(Color::Palette(15)).bg(Color::Palette(5));
/// The `<`, `>` and `\` marking text off screen: reverse, as controls
/// are, so they read as the pager's and not the text's.
pub const EDGE: Style = PLAIN.reverse();
pub const SEP_NAME: Style = PLAIN.bold();
/// Dim, so the separator's rule recedes from the text.
pub const SEP_RULE: Style = PLAIN.dim();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attrs_set_and_clear() {
        let mut a = Attrs::BOLD | Attrs::ITALIC;
        assert!(a.contains(Attrs::BOLD) && a.contains(Attrs::ITALIC));
        assert!(!a.contains(Attrs::DIM));
        a.insert(Attrs::DIM);
        a.remove(Attrs::BOLD);
        assert_eq!(a, Attrs::DIM | Attrs::ITALIC);
        assert!(a.contains(Attrs::DIM | Attrs::ITALIC));
        assert!(!a.contains(Attrs::DIM | Attrs::BOLD));
    }

    #[test]
    fn builders() {
        let s = Style::default()
            .fg(Color::Palette(1))
            .bg(Color::Rgb(1, 2, 3))
            .bold()
            .dim()
            .reverse()
            .with(Attrs::STRIKE);
        assert_eq!(s.fg, Color::Palette(1));
        assert_eq!(s.bg, Color::Rgb(1, 2, 3));
        assert_eq!(
            s.attrs,
            Attrs::BOLD | Attrs::DIM | Attrs::REVERSE | Attrs::STRIKE
        );
        assert_eq!(Style::default().fg, Color::Default);
        assert_eq!(Style::default().attrs, Attrs::default());
    }

    #[test]
    fn pager_styles() {
        let d = Style::default();
        assert_eq!(SELECTION, d.reverse());
        assert_eq!(EDGE, d.reverse());
        assert_eq!(MATCH, d.fg(Color::Palette(0)).bg(Color::Palette(11)));
        assert_eq!(CUR_MATCH, d.fg(Color::Palette(15)).bg(Color::Palette(5)));
        assert_eq!(SEP_NAME, d.bold());
        assert_eq!(SEP_RULE, d.dim());
    }
}
