use std::fmt;

/// What a terminal would show. A pager that draws with tcell sends only
/// the cells that changed, so a wait must look at the screen, not the
/// byte stream.
pub struct Screen(vt100::Parser);

impl Screen {
    pub fn new(rows: u16, cols: u16) -> Self {
        Screen(vt100::Parser::new(rows, cols, 0))
    }

    pub fn write(&mut self, bytes: &[u8]) {
        self.0.process(bytes);
    }

    /// Whether a row shows text.
    pub fn contains(&self, text: &str) -> bool {
        let (_, cols) = self.0.screen().size();
        self.0.screen().rows(0, cols).any(|row| row.contains(text))
    }
}

/// A line per row, for a failure message.
impl fmt::Display for Screen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (_, cols) = self.0.screen().size();
        for row in self.0.screen().rows(0, cols) {
            writeln!(f, "{}", row.trim_end())?;
        }
        Ok(())
    }
}

/// A query a pager sends at startup that a terminal answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Query {
    /// The primary device attributes.
    Da1,
    /// The status.
    Dsr,
    /// The background color.
    Bg,
}

impl Query {
    /// The answer as a VT220 with status OK on black. A pager waits on
    /// these, since they come after the answers to its other queries.
    pub fn reply(self) -> &'static [u8] {
        match self {
            Query::Da1 => b"\x1b[?62;22c",
            Query::Dsr => b"\x1b[0n",
            Query::Bg => b"\x1b]11;rgb:0000/0000/0000\x1b\\",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Text,
    Esc,
    /// The one byte after a charset designator.
    Esc2,
    Csi,
    /// A control string; `osc` says whether it is an OSC.
    Str,
    StrEsc,
}

/// Finds the queries in a pager's output, which vt100 drops. State
/// carries across writes: a sequence may straddle reads.
pub struct Queries {
    state: State,
    /// The CSI parameters and intermediates, or the OSC, so far.
    seq: Vec<u8>,
    osc: bool,
}

impl Queries {
    pub fn new() -> Self {
        Queries {
            state: State::Text,
            seq: Vec::new(),
            osc: false,
        }
    }

    /// The queries in bytes, in order.
    pub fn scan(&mut self, bytes: &[u8]) -> Vec<Query> {
        let mut out = Vec::new();
        for &b in bytes {
            match self.state {
                State::Text => {
                    if b == 0x1b {
                        self.state = State::Esc;
                    }
                }
                State::Esc => {
                    self.state = State::Text;
                    self.seq.clear();
                    match b {
                        b'[' => self.state = State::Csi,
                        b']' | b'P' | b'_' | b'^' | b'X' => {
                            self.state = State::Str;
                            self.osc = b == b']';
                        }
                        b'(' | b')' | b'*' | b'+' | b'#' | b'%' => self.state = State::Esc2,
                        _ => {}
                    }
                }
                State::Esc2 => self.state = State::Text,
                State::Csi => {
                    if (0x40..=0x7e).contains(&b) {
                        out.extend(self.csi(b));
                        self.state = State::Text;
                    } else {
                        self.seq.push(b);
                    }
                }
                State::Str => match b {
                    7 => out.extend(self.end_str()),
                    0x1b => self.state = State::StrEsc,
                    _ => self.seq.push(b),
                },
                State::StrEsc => out.extend(self.end_str()),
            }
        }
        out
    }

    fn csi(&self, end: u8) -> Option<Query> {
        if self.seq.first().is_some_and(|b| (b'<'..=b'?').contains(b)) {
            return None;
        }
        match end {
            b'c' => Some(Query::Da1),
            b'n' if self.first() == Some(5) => Some(Query::Dsr),
            _ => None,
        }
    }

    fn first(&self) -> Option<u32> {
        let p = self.seq.split(|&b| b == b';').next()?;
        std::str::from_utf8(p).ok()?.parse().ok()
    }

    fn end_str(&mut self) -> Option<Query> {
        self.state = State::Text;
        (self.osc && self.seq == b"11;?").then_some(Query::Bg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(s: &Screen) -> String {
        s.to_string().trim_end_matches('\n').to_string()
    }

    #[test]
    fn text() {
        for (input, want) in [
            ("\x1b[31mred\x1b[0m", "red"),
            ("\x1b]8;;http://x\x07link\x1b]8;;\x1b\\", "link"),
            ("\x1b(Bplain", "plain"),
            ("a\r\nb", "a\nb"),
            ("caf\u{e9}", "café"),
            ("\x1b[2;3Hx", "\n  x"),
            ("abc\x1b[2D\x1b[K", "a"),
        ] {
            let mut s = Screen::new(3, 10);
            s.write(input.as_bytes());
            assert_eq!(shown(&s), want, "write({input:?})");
        }
    }

    /// A sequence, or a rune, cut by a read boundary leaks nothing.
    #[test]
    fn split() {
        let mut s = Screen::new(1, 10);
        s.write(b"\x1b[3");
        s.write(b"1mr\xc3");
        s.write(b"\xa9d");
        assert_eq!(shown(&s), "réd");
    }

    /// A pager that rewrites only the changed cells still leaves the
    /// whole line on screen.
    #[test]
    fn redraw() {
        let mut s = Screen::new(2, 12);
        s.write(b"\x1b[1;1H00004979:xx");
        s.write(b"\x1b[1;5H1");
        assert!(s.contains("00001979:xx"), "screen:\n{s}");
    }

    /// A line feed on the last row scrolls, and the scroll region holds
    /// it.
    #[test]
    fn scroll() {
        let mut s = Screen::new(3, 4);
        s.write(b"a\r\nb\r\nc\r\nd");
        assert_eq!(s.to_string(), "b\nc\nd\n", "scroll");
        s.write(b"\x1b[1;2r\x1b[2;1H\ne");
        assert_eq!(s.to_string(), "c\ne\nd\n", "region");
        s.write(b"\x1b[r\x1b[1;1H\x1b[M");
        assert_eq!(s.to_string(), "e\nd\n\n", "delete line");
    }

    /// The queries a terminal answers are found once, in order, and
    /// private-mode queries and other strings are not.
    #[test]
    fn asked() {
        let mut q = Queries::new();
        assert_eq!(
            q.scan(b"\x1b[?u\x1b[>q\x1b]10;?\x07\x1bP$qm\x1b\\\x1b[6n"),
            []
        );
        assert_eq!(
            q.scan(b"\x1b[c\x1b]11;?\x07\x1b]11;?\x1b\\\x1b[5n"),
            [Query::Da1, Query::Bg, Query::Bg, Query::Dsr]
        );
        assert_eq!(q.scan(b""), []);
        assert_eq!(
            q.scan(b"\x1b[05n\x1b[5;1n\x1b[n\x1b[?5n"),
            [Query::Dsr, Query::Dsr]
        );
    }

    /// A query cut by a read boundary is still found.
    #[test]
    fn asked_split() {
        let mut q = Queries::new();
        assert_eq!(q.scan(b"\x1b]11"), []);
        assert_eq!(q.scan(b";?\x1b"), []);
        assert_eq!(q.scan(b"\\\x1b["), [Query::Bg]);
        assert_eq!(q.scan(b"c"), [Query::Da1]);
    }
}
