use std::fmt;
use std::ops::BitOr;
use std::str::FromStr;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum KeyCode {
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Enter,
    Esc,
    Tab,
    Backspace,
    Char(char),
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Mods(u8);

impl Mods {
    pub const NONE: Mods = Mods(0);
    pub const SHIFT: Mods = Mods(1);
    pub const CTRL: Mods = Mods(1 << 1);
    pub const ALT: Mods = Mods(1 << 2);
    pub const CMD: Mods = Mods(1 << 3);

    /// Whether every modifier in `other` is set.
    pub const fn contains(self, other: Mods) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn insert(&mut self, other: Mods) {
        self.0 |= other.0;
    }

    pub fn remove(&mut self, other: Mods) {
        self.0 &= !other.0;
    }
}

impl BitOr for Mods {
    type Output = Mods;

    fn bitor(self, other: Mods) -> Mods {
        Mods(self.0 | other.0)
    }
}

/// A key as the key map knows it, once [`Key::normalized`].
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct Key {
    pub code: KeyCode,
    pub mods: Mods,
}

/// The US layout's Shift form of each character that has one. The
/// kitty keyboard protocol reports a modified key by its unshifted
/// character, so ⌥< arrives as , with Shift and Alt.
fn shifted(c: char) -> Option<char> {
    Some(match c {
        '`' => '~',
        '1' => '!',
        '2' => '@',
        '3' => '#',
        '4' => '$',
        '5' => '%',
        '6' => '^',
        '7' => '&',
        '8' => '*',
        '9' => '(',
        '0' => ')',
        '-' => '_',
        '=' => '+',
        '[' => '{',
        ']' => '}',
        '\\' => '|',
        ';' => ':',
        '\'' => '"',
        ',' => '<',
        '.' => '>',
        '/' => '?',
        _ => return None,
    })
}

const NAMES: [(&str, KeyCode); 12] = [
    ("Up", KeyCode::Up),
    ("Down", KeyCode::Down),
    ("Left", KeyCode::Left),
    ("Right", KeyCode::Right),
    ("Home", KeyCode::Home),
    ("End", KeyCode::End),
    ("PgUp", KeyCode::PageUp),
    ("PgDn", KeyCode::PageDown),
    ("Enter", KeyCode::Enter),
    ("Esc", KeyCode::Esc),
    ("Tab", KeyCode::Tab),
    ("Backspace", KeyCode::Backspace),
];

const PREFIXES: [(&str, Mods); 4] = [
    ("Shift+", Mods::SHIFT),
    ("Ctrl+", Mods::CTRL),
    ("Alt+", Mods::ALT),
    ("Cmd+", Mods::CMD),
];

impl Key {
    pub fn new(code: KeyCode, mods: Mods) -> Key {
        Key { code, mods }
    }

    /// The key comparable to a parsed name. A control character is its
    /// letter with Ctrl, and Ctrl+h, Ctrl+i and Ctrl+m are Backspace,
    /// Tab and Enter, the keys a legacy terminal cannot tell them from.
    /// Those and Esc name themselves, so Ctrl is dropped from them;
    /// Ctrl+Backspace, which only a terminal that reports Backspace
    /// apart from Ctrl+h sends, keeps it.
    /// NUL is Ctrl+Space. Shift on a character that has a shifted form
    /// is that form, as a legacy terminal sends it: , with Shift is <.
    /// Otherwise Shift is dropped from a character, Shift+g is G,
    /// except under Cmd, where the kitty protocol sends ⇧⌘G as G with
    /// Shift, so an uppercase letter with Cmd is the lowercase one with
    /// Shift.
    pub fn normalized(self) -> Key {
        let Key { mut code, mut mods } = self;
        if let KeyCode::Char(c) = code {
            match c {
                '\0' => {
                    code = KeyCode::Char(' ');
                    mods.insert(Mods::CTRL);
                }
                '\x7f' => code = KeyCode::Backspace,
                '\x01'..='\x1a' => {
                    code = KeyCode::Char(char::from(b'a' - 1 + c as u8));
                    mods.insert(Mods::CTRL);
                }
                _ => {}
            }
        }
        if let KeyCode::Char(c) = code
            && mods.contains(Mods::CTRL)
            && c.is_ascii_alphabetic()
        {
            code = match c.to_ascii_lowercase() {
                'h' => KeyCode::Backspace,
                'i' => KeyCode::Tab,
                'm' => KeyCode::Enter,
                c => return Key::new(KeyCode::Char(c), mods),
            };
            mods.remove(Mods::CTRL);
        }
        let shift_form = match code {
            KeyCode::Char(c) if mods.contains(Mods::SHIFT) => shifted(c),
            _ => None,
        };
        match code {
            KeyCode::Enter | KeyCode::Esc | KeyCode::Tab => mods.remove(Mods::CTRL),
            KeyCode::Char(' ') if mods.contains(Mods::CTRL) => {}
            KeyCode::Char(_) if let Some(s) = shift_form => {
                code = KeyCode::Char(s);
                mods.remove(Mods::SHIFT);
            }
            KeyCode::Char(_) if !mods.contains(Mods::CMD) => mods.remove(Mods::SHIFT),
            KeyCode::Char(c) if c.is_uppercase() => {
                code = KeyCode::Char(c.to_lowercase().next().unwrap_or(c));
                mods.insert(Mods::SHIFT);
            }
            _ => {}
        }
        Key::new(code, mods)
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum KeyError {
    Unknown(String),
    /// Ctrl on a character other than a letter or Space, or Shift on
    /// one without Cmd: Shift makes another character, or none.
    Modifiers(String),
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            KeyError::Unknown(name) => write!(f, "unknown key {name:?}"),
            KeyError::Modifiers(name) => write!(f, "{name:?}: modifiers on a character key"),
        }
    }
}

impl std::error::Error for KeyError {}

/// A key name: Up Down Left Right Home End PgUp PgDn Enter Esc Tab
/// Backspace Space or one character, after any of Shift+ Ctrl+ Alt+
/// Cmd+.
impl FromStr for Key {
    type Err = KeyError;

    fn from_str(name: &str) -> Result<Key, KeyError> {
        let mut tok = name;
        let mut mods = Mods::NONE;
        while let Some((rest, m)) = PREFIXES
            .iter()
            .find_map(|&(p, m)| tok.strip_prefix(p).map(|rest| (rest, m)))
        {
            tok = rest;
            mods.insert(m);
        }
        if let Some(&(_, code)) = NAMES.iter().find(|&&(n, _)| n == tok) {
            return Ok(Key::new(code, mods).normalized());
        }
        if tok == "Space" {
            tok = " ";
        }
        let mut chars = tok.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else {
            return Err(KeyError::Unknown(name.to_string()));
        };
        let ctrl = mods.contains(Mods::CTRL);
        if ctrl && !(c == ' ' || c.is_ascii_alphabetic())
            || !ctrl && mods.contains(Mods::SHIFT) && !mods.contains(Mods::CMD)
        {
            return Err(KeyError::Modifiers(name.to_string()));
        }
        Ok(Key::new(KeyCode::Char(c), mods).normalized())
    }
}

/// The help name, spelled as the config file spells it so it parses
/// back: Ctrl+ Alt+ Cmd+ Shift+, then the key.
impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (p, m) in [
            ("Ctrl+", Mods::CTRL),
            ("Alt+", Mods::ALT),
            ("Cmd+", Mods::CMD),
            ("Shift+", Mods::SHIFT),
        ] {
            if self.mods.contains(m) {
                f.write_str(p)?;
            }
        }
        match self.code {
            KeyCode::Char(' ') => f.write_str("Space"),
            KeyCode::Char(c) => write!(f, "{c}"),
            code => {
                let name = NAMES.iter().find(|&&(_, k)| k == code).map(|&(n, _)| n);
                f.write_str(name.unwrap_or_default())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use KeyCode::*;

    fn key(code: KeyCode, mods: Mods) -> Key {
        Key::new(code, mods)
    }

    fn ch(c: char, mods: Mods) -> Key {
        key(Char(c), mods)
    }

    const NONE: Mods = Mods::NONE;
    const SHIFT: Mods = Mods::SHIFT;
    const CTRL: Mods = Mods::CTRL;
    const ALT: Mods = Mods::ALT;
    const CMD: Mods = Mods::CMD;

    #[test]
    fn normalized() {
        let tests = [
            ("up", key(Up, NONE), key(Up, NONE)),
            ("shift up keeps shift", key(Up, SHIFT), key(Up, SHIFT)),
            (
                "shift on a character is dropped",
                ch('<', SHIFT),
                ch('<', NONE),
            ),
            ("capital stays capital", ch('G', SHIFT), ch('G', NONE)),
            ("ctrl f", ch('f', CTRL), ch('f', CTRL)),
            ("ctrl F is ctrl f", ch('F', CTRL), ch('f', CTRL)),
            (
                "ctrl shift f keeps shift",
                ch('F', CTRL | SHIFT),
                ch('f', CTRL | SHIFT),
            ),
            ("ctrl f as a control char", ch('\x06', NONE), ch('f', CTRL)),
            ("ctrl h is backspace", ch('h', CTRL), key(Backspace, NONE)),
            ("bs is backspace", ch('\x08', NONE), key(Backspace, NONE)),
            (
                "ctrl backspace keeps ctrl",
                key(Backspace, CTRL),
                key(Backspace, CTRL),
            ),
            ("ctrl i is tab", ch('i', CTRL), key(Tab, NONE)),
            ("ctrl m is enter", ch('m', CTRL), key(Enter, NONE)),
            ("cr is enter", ch('\r', NONE), key(Enter, NONE)),
            ("enter", key(Enter, NONE), key(Enter, NONE)),
            ("ctrl enter is enter", key(Enter, CTRL), key(Enter, NONE)),
            ("esc", key(Esc, NONE), key(Esc, NONE)),
            ("alt v", ch('v', ALT), ch('v', ALT)),
            ("cmd c", ch('c', CMD), ch('c', CMD)),
            ("cmd shift g", ch('g', CMD | SHIFT), ch('g', CMD | SHIFT)),
            ("cmd G is cmd shift g", ch('G', CMD), ch('g', CMD | SHIFT)),
            (
                "ctrl shift left",
                key(Left, CTRL | SHIFT),
                key(Left, CTRL | SHIFT),
            ),
            (
                "kitty alt shift comma is alt <",
                ch(',', SHIFT | ALT),
                ch('<', ALT),
            ),
            (
                "kitty alt shift period is alt >",
                ch('.', SHIFT | ALT),
                ch('>', ALT),
            ),
            (
                "kitty cmd shift slash is cmd ?",
                ch('/', SHIFT | CMD),
                ch('?', CMD),
            ),
            ("legacy alt < is alt <", ch('<', ALT), ch('<', ALT)),
            ("ctrl space", ch(' ', CTRL), ch(' ', CTRL)),
            ("nul is ctrl space", ch('\0', CTRL), ch(' ', CTRL)),
            (
                "nul without mod is ctrl space",
                ch('\0', NONE),
                ch(' ', CTRL),
            ),
            ("ctrl alt space", ch(' ', CTRL | ALT), ch(' ', CTRL | ALT)),
            ("del is backspace", ch('\x7f', NONE), key(Backspace, NONE)),
        ];
        for (name, raw, want) in tests {
            let got = raw.normalized();
            assert_eq!(got, want, "{name}");
            assert_eq!(got.normalized(), got, "{name}: normalized twice");
        }
    }

    #[test]
    fn shifted_forms() {
        let table = [
            ('`', '~'),
            ('1', '!'),
            ('2', '@'),
            ('3', '#'),
            ('4', '$'),
            ('5', '%'),
            ('6', '^'),
            ('7', '&'),
            ('8', '*'),
            ('9', '('),
            ('0', ')'),
            ('-', '_'),
            ('=', '+'),
            ('[', '{'),
            (']', '}'),
            ('\\', '|'),
            (';', ':'),
            ('\'', '"'),
            (',', '<'),
            ('.', '>'),
            ('/', '?'),
        ];
        for (c, s) in table {
            assert_eq!(ch(c, SHIFT).normalized(), ch(s, NONE), "{c:?}");
            assert_eq!(
                ch(c, SHIFT | ALT).normalized(),
                ch(s, ALT),
                "{c:?} with Alt"
            );
        }
    }

    #[test]
    fn parse() {
        let tests = [
            ("Up", key(Up, NONE)),
            ("Shift+Down", key(Down, SHIFT)),
            ("Ctrl+Shift+Left", key(Left, CTRL | SHIFT)),
            ("Shift+Ctrl+Left", key(Left, CTRL | SHIFT)),
            ("Alt+Right", key(Right, ALT)),
            ("Home", key(Home, NONE)),
            ("End", key(End, NONE)),
            ("PgUp", key(PageUp, NONE)),
            ("Shift+PgDn", key(PageDown, SHIFT)),
            ("Enter", key(Enter, NONE)),
            ("Esc", key(Esc, NONE)),
            ("Tab", key(Tab, NONE)),
            ("Backspace", key(Backspace, NONE)),
            ("Space", ch(' ', NONE)),
            ("Ctrl+Space", ch(' ', CTRL)),
            ("Ctrl+Alt+Space", ch(' ', CTRL | ALT)),
            ("j", ch('j', NONE)),
            ("G", ch('G', NONE)),
            ("<", ch('<', NONE)),
            ("é", ch('é', NONE)),
            ("Ctrl+f", ch('f', CTRL)),
            ("Ctrl+F", ch('f', CTRL)),
            ("Ctrl+m", key(Enter, NONE)),
            ("Alt+v", ch('v', ALT)),
            ("Alt+<", ch('<', ALT)),
            ("Cmd+c", ch('c', CMD)),
            ("Cmd+Shift+g", ch('g', CMD | SHIFT)),
            ("Cmd+G", ch('g', CMD | SHIFT)),
            ("Cmd+Up", key(Up, CMD)),
        ];
        for (name, want) in tests {
            assert_eq!(name.parse::<Key>(), Ok(want), "{name}");
        }
        let bad = [
            ("F1", r#"unknown key "F1""#),
            ("Spacex", r#"unknown key "Spacex""#),
            ("Ctrl+", r#"unknown key "Ctrl+""#),
            ("", r#"unknown key """#),
            ("Shift+g", r#""Shift+g": modifiers on a character key"#),
            (
                "Shift+Space",
                r#""Shift+Space": modifiers on a character key"#,
            ),
            ("Ctrl+<", r#""Ctrl+<": modifiers on a character key"#),
        ];
        for (name, want) in bad {
            match name.parse::<Key>() {
                Err(err) => assert_eq!(err.to_string(), want, "{name}"),
                Ok(k) => panic!("{name:?} parsed as {k:?}"),
            }
        }
    }

    /// The help name is the config spelling.
    #[test]
    fn display() {
        let tests = [
            ("Up", "Up"),
            ("Down", "Down"),
            ("Left", "Left"),
            ("Right", "Right"),
            ("Home", "Home"),
            ("End", "End"),
            ("PgUp", "PgUp"),
            ("PgDn", "PgDn"),
            ("Enter", "Enter"),
            ("Esc", "Esc"),
            ("Tab", "Tab"),
            ("Backspace", "Backspace"),
            ("Space", "Space"),
            ("Ctrl+Space", "Ctrl+Space"),
            ("Alt+Ctrl+Space", "Ctrl+Alt+Space"),
            ("j", "j"),
            ("G", "G"),
            ("<", "<"),
            ("Ctrl+f", "Ctrl+f"),
            ("Ctrl+F", "Ctrl+f"),
            ("Alt+v", "Alt+v"),
            ("Alt+V", "Alt+V"),
            ("Shift+Up", "Shift+Up"),
            ("Ctrl+Shift+Left", "Ctrl+Shift+Left"),
            ("Shift+Alt+Right", "Alt+Shift+Right"),
            ("Cmd+c", "Cmd+c"),
            ("Cmd+Shift+g", "Cmd+Shift+g"),
            ("Cmd+G", "Cmd+Shift+g"),
            ("Cmd+Left", "Cmd+Left"),
        ];
        for (name, want) in tests {
            let k: Key = name.parse().unwrap();
            let s = k.to_string();
            assert_eq!(s, want, "{name}");
            assert_eq!(s.parse::<Key>(), Ok(k), "{name}: round trip");
        }
    }
}
