use std::ops::{BitOr, Index, IndexMut};
use std::str::FromStr;

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

    /// This drawn over `under`: its colors where it sets them, and the
    /// attributes of both.
    pub const fn over(self, under: Style) -> Style {
        Style {
            fg: match self.fg {
                Color::Default => under.fg,
                fg => fg,
            },
            bg: match self.bg {
                Color::Default => under.bg,
                bg => bg,
            },
            attrs: Attrs(under.attrs.0 | self.attrs.0),
        }
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
/// What the status line says was done: the base yellow, apart from a
/// match's bright one on the row above.
pub const NOTE: Style = PLAIN.fg(Color::Palette(0)).bg(Color::Palette(3));
/// What it says could not be done.
pub const ERROR: Style = PLAIN.fg(Color::Palette(15)).bg(Color::Palette(1));
/// The bar after wrap_width's last cell and the ~ below the text: dim,
/// so they sit behind the text rather than among it.
pub const GUIDE: Style = PLAIN.dim();
pub const SEP_NAME: Style = PLAIN.bold();
/// A hint's label: bright cyan, far from a match's yellow and the
/// current match's magenta, which it is drawn over.
pub const HINT: Style = PLAIN.fg(Color::Palette(0)).bg(Color::Palette(14)).bold();
/// Cyan, which reads on light and dark palettes alike, so the row
/// stands out from the text while the name stays as legible as it.
pub const SEP_RULE: Style = PLAIN.fg(Color::Palette(6));
/// A control character, drawn as ^X.
pub const CONTROL: Style = PLAIN.reverse();
/// The status line, but for a prompt's row, which stays plain so the
/// terminal's cursor shows as it does in the text.
pub const STATUS: Style = PLAIN.reverse();

/// What the pager draws in a style of its own, as `color` names it.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Element {
    Selection,
    Match,
    CurrentMatch,
    Hint,
    Note,
    Error,
    Status,
    EdgeMarker,
    Control,
    Guide,
    Separator,
    SeparatorName,
}

impl Element {
    pub const ALL: [Element; 12] = [
        Element::Selection,
        Element::Match,
        Element::CurrentMatch,
        Element::Hint,
        Element::Note,
        Element::Error,
        Element::Status,
        Element::EdgeMarker,
        Element::Control,
        Element::Guide,
        Element::Separator,
        Element::SeparatorName,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Element::Selection => "selection",
            Element::Match => "match",
            Element::CurrentMatch => "current_match",
            Element::Hint => "hint",
            Element::Note => "note",
            Element::Error => "error",
            Element::Status => "status",
            Element::EdgeMarker => "edge_marker",
            Element::Control => "control",
            Element::Guide => "guide",
            Element::Separator => "separator",
            Element::SeparatorName => "separator_name",
        }
    }
}

impl FromStr for Element {
    type Err = ();

    fn from_str(s: &str) -> Result<Element, ()> {
        Element::ALL.into_iter().find(|e| e.name() == s).ok_or(())
    }
}

/// Each element's style.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Theme([Style; Element::ALL.len()]);

impl Default for Theme {
    fn default() -> Theme {
        Theme([
            SELECTION, MATCH, CUR_MATCH, HINT, NOTE, ERROR, STATUS, EDGE, CONTROL, GUIDE, SEP_RULE,
            SEP_NAME,
        ])
    }
}

impl Index<Element> for Theme {
    type Output = Style;

    fn index(&self, e: Element) -> &Style {
        &self.0[e as usize]
    }
}

impl IndexMut<Element> for Theme {
    fn index_mut(&mut self, e: Element) -> &mut Style {
        &mut self.0[e as usize]
    }
}

const COLORS: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];

const ATTRS: [(&str, Attrs); 6] = [
    ("bold", Attrs::BOLD),
    ("dim", Attrs::DIM),
    ("italic", Attrs::ITALIC),
    ("underline", Attrs::UNDERLINE),
    ("reverse", Attrs::REVERSE),
    ("strike", Attrs::STRIKE),
];

/// The words a style is written in but numbers and #rrggbb.
pub fn style_words() -> Vec<String> {
    let names = COLORS.iter().map(|c| c.to_string());
    let bright = COLORS.iter().map(|c| format!("bright_{c}"));
    let attrs = ATTRS.iter().map(|(a, _)| a.to_string());
    let other = ["default", "on"].map(String::from);
    names.chain(bright).chain(attrs).chain(other).collect()
}

impl FromStr for Color {
    type Err = ();

    /// A name, bright_ and a name, 0 to 255, #rrggbb or default.
    fn from_str(s: &str) -> Result<Color, ()> {
        if s == "default" {
            return Ok(Color::Default);
        }
        if let Ok(n) = s.parse::<u8>() {
            return Ok(Color::Palette(n));
        }
        if let Some(hex) = s.strip_prefix('#') {
            let byte = |i: usize| {
                let h = hex
                    .get(i..i + 2)
                    .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()));
                h.and_then(|h| u8::from_str_radix(h, 16).ok())
            };
            return match (hex.len(), byte(0), byte(2), byte(4)) {
                (6, Some(r), Some(g), Some(b)) => Ok(Color::Rgb(r, g, b)),
                _ => Err(()),
            };
        }
        let (name, base) = match s.strip_prefix("bright_") {
            Some(name) => (name, 8),
            None => (s, 0),
        };
        let n = COLORS.iter().position(|&c| c == name).ok_or(())?;
        Ok(Color::Palette(base + n as u8))
    }
}

impl FromStr for Style {
    type Err = String;

    /// `[fg] [on bg] [attr...]`, attributes in any place.
    fn from_str(s: &str) -> Result<Style, String> {
        let mut st = PLAIN;
        let (mut fg, mut bg) = (false, false);
        let mut words = s.split_whitespace();
        while let Some(w) = words.next() {
            if let Some(&(_, a)) = ATTRS.iter().find(|(name, _)| *name == w) {
                st.attrs.insert(a);
            } else if w == "on" {
                let c = words.next().ok_or("on takes a color")?;
                if bg {
                    return Err("a style has one background".into());
                }
                st.bg = c.parse().map_err(|()| format!("unknown color {c:?}"))?;
                bg = true;
            } else if fg || bg {
                return Err(format!("{w:?} is no attribute"));
            } else {
                st.fg = w.parse().map_err(|()| format!("unknown color {w:?}"))?;
                fg = true;
            }
        }
        Ok(st)
    }
}

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
        assert_eq!(NOTE, d.fg(Color::Palette(0)).bg(Color::Palette(3)));
        assert_eq!(ERROR, d.fg(Color::Palette(15)).bg(Color::Palette(1)));
        assert_eq!(SEP_NAME, d.bold());
        assert_eq!(HINT, d.fg(Color::Palette(0)).bg(Color::Palette(14)).bold());
        assert_eq!(SEP_RULE, d.fg(Color::Palette(6)));
    }

    #[test]
    fn element_names() {
        for e in Element::ALL {
            assert_eq!(e.name().parse(), Ok(e));
        }
        assert_eq!("matches".parse::<Element>(), Err(()));
    }

    #[test]
    fn parse_color() {
        let d = Style::default();
        for (s, want) in [
            ("", d),
            ("red", d.fg(Color::Palette(1))),
            ("bright_white", d.fg(Color::Palette(15))),
            ("200", d.fg(Color::Palette(200))),
            ("#0aFf10", d.fg(Color::Rgb(10, 255, 16))),
            ("default", d),
            ("on blue", d.bg(Color::Palette(4))),
            ("black on bright_yellow", MATCH),
            ("bold black on 14", HINT),
            ("reverse", SELECTION),
            (
                "dim italic underline strike",
                d.dim()
                    .with(Attrs::ITALIC | Attrs::UNDERLINE | Attrs::STRIKE),
            ),
        ] {
            assert_eq!(s.parse(), Ok(want), "{s:?}");
        }
        for (s, err) in [
            ("purple", "unknown color \"purple\""),
            ("bright_", "unknown color \"bright_\""),
            ("256", "unknown color \"256\""),
            ("#12345", "unknown color \"#12345\""),
            ("#12345g", "unknown color \"#12345g\""),
            ("on", "on takes a color"),
            ("on mauve", "unknown color \"mauve\""),
            ("red blue", "\"blue\" is no attribute"),
            ("on red blue", "\"blue\" is no attribute"),
            ("on red on blue", "a style has one background"),
        ] {
            assert_eq!(s.parse::<Style>(), Err(err.to_string()), "{s:?}");
        }
    }
}
