//! Maps (line, char index) positions to visual (row, cell) positions
//! for a width and wrap mode.

use std::fmt;
use std::str::FromStr;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// The cells cluster `g` takes. A terminal gives a cluster at most two
/// cells, whatever its chars sum to, so the measure is capped there.
pub(crate) fn cluster_width(g: &str) -> usize {
    g.width().min(2)
}

/// Whether a line wraps at the width or runs past it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Wrap,
    NoWrap,
}

/// Where `Wrap` ends a row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WrapStyle {
    /// At the screen's edge.
    #[default]
    Char,
    /// After a space or tab.
    Word,
}

impl FromStr for WrapStyle {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "char" => Ok(WrapStyle::Char),
            "word" => Ok(WrapStyle::Word),
            _ => Err(()),
        }
    }
}

impl fmt::Display for WrapStyle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            WrapStyle::Char => "char",
            WrapStyle::Word => "word",
        })
    }
}

/// The tab width when `Layout::tab` is zero.
pub const DEFAULT_TAB: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    pub width: usize,
    pub mode: Mode,
    /// Cells per tab stop; zero is `DEFAULT_TAB`.
    pub tab: usize,
    pub wrap_style: WrapStyle,
}

/// One visual row of a line: chars `[start, end)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub start: usize,
    pub end: usize,
}

/// A line laid out once: its cell columns and visual rows. Keep it;
/// every method is then cheap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Laid {
    xs: Vec<usize>,
    segs: Vec<Segment>,
}

/// C0, DEL and C1, as `draw::glyph` spells them.
fn is_control(c: char) -> bool {
    c.is_control()
}

impl Layout {
    /// The cells `c` takes on its own starting at cell `x`: a tab to the
    /// next stop, a C0 control two (drawn as `^X`), a C1 control one
    /// (drawn as U+FFFD), a combining mark none.
    /// `cells` measures chars in their clusters; this is for a char known
    /// to stand alone.
    pub fn char_width(&self, c: char, x: usize) -> usize {
        if c == '\t' {
            let tab = if self.tab == 0 { DEFAULT_TAB } else { self.tab };
            return tab - x % tab;
        }
        if is_control(c) {
            return if c.is_ascii() { 2 } else { 1 };
        }
        if c.is_ascii() {
            return 1;
        }
        c.width().unwrap_or(0)
    }

    /// `text.len() + 1` entries: where each char starts, then the total
    /// width. A grapheme cluster is measured whole: its first char takes
    /// the cells, the rest none.
    pub fn cells(&self, text: &[char]) -> Vec<usize> {
        let mut xs = Vec::with_capacity(text.len() + 1);
        let mut x = 0;
        // ASCII chars share a cluster only as \r\n, whose chars are
        // drawn as two controls, so ASCII skips the segmenter.
        if text.iter().all(char::is_ascii) {
            for &c in text {
                xs.push(x);
                x += self.char_width(c, x);
            }
        } else {
            let s: String = text.iter().collect();
            for g in s.graphemes(true) {
                let mut chars = g.chars();
                let first = chars.next().expect("a grapheme has a char");
                if is_control(first) {
                    for c in g.chars() {
                        xs.push(x);
                        x += self.char_width(c, x);
                    }
                    continue;
                }
                xs.push(x);
                x += cluster_width(g);
                xs.extend(chars.map(|_| x));
            }
        }
        xs.push(x);
        xs
    }

    /// Lays `text` out. `NoWrap`, or a width of zero, gives one row. `Wrap`
    /// moves a glyph that does not fit to the next row; with
    /// `WrapStyle::Word`, the word it is in, when that leaves something
    /// on the row. An empty line is one empty row.
    pub fn line(&self, text: &[char]) -> Laid {
        let xs = self.cells(text);
        let n = text.len();
        if self.mode == Mode::NoWrap || self.width == 0 {
            return Laid {
                xs,
                segs: vec![Segment { start: 0, end: n }],
            };
        }
        if self.wrap_style == WrapStyle::Word {
            let segs = self.word_rows(text, &xs);
            return Laid { xs, segs };
        }
        let mut segs = Vec::new();
        let (mut start, mut x0) = (0, 0);
        for i in 0..n {
            let w = xs[i + 1] - xs[i];
            if xs[i] - x0 + w > self.width && i > start {
                segs.push(Segment { start, end: i });
                (start, x0) = (i, xs[i]);
            }
        }
        segs.push(Segment { start, end: n });
        Laid { xs, segs }
    }

    /// Breaks after the last space or tab that fits, so a row begins with
    /// a word. A word wider than the row breaks by glyph, as a run of
    /// spaces does.
    fn word_rows(&self, text: &[char], xs: &[usize]) -> Vec<Segment> {
        let mut segs = Vec::new();
        let (mut start, mut x0) = (0, 0);
        // Where the row may break; whether the glyph before is a space.
        let (mut brk, mut space) = (0, false);
        for (i, &c) in text.iter().enumerate() {
            let w = xs[i + 1] - xs[i];
            if w == 0 {
                continue;
            }
            if space {
                brk = i;
            }
            space = c == ' ' || c == '\t';
            // Twice when a wide glyph still does not fit after its word moved.
            while xs[i] - x0 + w > self.width && i > start {
                let at = if brk > start { brk } else { i };
                segs.push(Segment { start, end: at });
                (start, x0) = (at, xs[at]);
            }
        }
        segs.push(Segment {
            start,
            end: text.len(),
        });
        segs
    }

    /// `ln`, laid out by `self`, with an empty row after its last when that
    /// row is full: a place for the cursor on the newline, which has no
    /// cell of its own.
    pub fn newline_row(&self, mut ln: Laid) -> Laid {
        let n = ln.xs.len() - 1;
        let last = ln.segs[ln.segs.len() - 1];
        if self.mode == Mode::Wrap && self.width > 0 && ln.xs[n] - ln.xs[last.start] >= self.width {
            ln.segs.push(Segment { start: n, end: n });
        }
        ln
    }
}

impl Laid {
    /// The cell columns, as `Layout::cells` gives them.
    pub fn cells(&self) -> &[usize] {
        &self.xs
    }

    /// The chars `[i, j)` drawn as one glyph with char `col`: it and the
    /// rest of its cluster. The newline after the last char is a glyph of
    /// its own.
    pub fn glyph(&self, col: usize) -> (usize, usize) {
        let n = self.xs.len() - 1;
        let mut i = col;
        while i > 0 && i < n && self.xs[i + 1] == self.xs[i] {
            i -= 1;
        }
        let mut j = i + 1;
        while j < n && self.xs[j + 1] == self.xs[j] {
            j += 1;
        }
        (i, j)
    }

    /// The visual rows.
    pub fn segments(&self) -> &[Segment] {
        &self.segs
    }

    /// The number of visual rows.
    pub fn rows(&self) -> usize {
        self.segs.len()
    }

    /// The row holding char `col`; `col == len` is on the last.
    pub fn segment_at(&self, col: usize) -> usize {
        self.segs
            .partition_point(|s| s.end <= col)
            .min(self.segs.len() - 1)
    }

    /// The row and cell within it of char `col`, clamped to the line.
    pub fn pos(&self, col: usize) -> (usize, usize) {
        let col = col.min(self.xs.len() - 1);
        let row = self.segment_at(col);
        (row, self.xs[col] - self.xs[self.segs[row].start])
    }

    /// The char whose cells hold `x` on `row`, clamped to the rows. Past
    /// the row's end it is the line's end on the last row and the last
    /// char with a cell on any other, so the cursor stays on that row.
    pub fn col(&self, row: usize, x: usize) -> usize {
        let row = row.min(self.segs.len() - 1);
        let s = self.segs[row];
        let x0 = self.xs[s.start];
        if let Some(i) = (s.start..s.end).find(|&i| self.xs[i + 1] - x0 > x) {
            return i;
        }
        if row == self.segs.len() - 1 {
            return self.xs.len() - 1;
        }
        let mut i = s.end - 1;
        while i > s.start && self.xs[i + 1] == self.xs[i] {
            i -= 1;
        }
        i
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    fn wrap(width: usize) -> Layout {
        Layout {
            width,
            ..Layout::default()
        }
    }

    fn words(width: usize) -> Layout {
        Layout {
            width,
            wrap_style: WrapStyle::Word,
            ..Layout::default()
        }
    }

    fn nowrap(width: usize) -> Layout {
        Layout {
            width,
            mode: Mode::NoWrap,
            ..Layout::default()
        }
    }

    /// Name, layout, text, segments.
    type Row<'a> = (&'a str, Layout, &'a str, &'a [(usize, usize)]);

    fn segs(v: &[(usize, usize)]) -> Vec<Segment> {
        v.iter()
            .map(|&(start, end)| Segment { start, end })
            .collect()
    }

    #[test]
    fn cells() {
        let tests: &[(&str, &[usize])] = &[
            ("", &[0]),
            ("abc", &[0, 1, 2, 3]),
            ("a\tb", &[0, 1, 8, 9]),
            ("\t\t", &[0, 8, 16]),
            ("日本", &[0, 2, 4]),
            ("a\rb", &[0, 1, 3, 4]),
            ("e\u{301}x", &[0, 1, 1, 2]),
            ("👨\u{200d}👩\u{200d}👧x", &[0, 2, 2, 2, 2, 2, 3]),
            ("🇺🇸x", &[0, 2, 2, 3]),
            ("❤\u{fe0f}x", &[0, 2, 2, 3]),
        ];
        for &(input, want) in tests {
            assert_eq!(Layout::default().cells(&chars(input)), want, "{input:?}");
        }
    }

    #[test]
    fn segments() {
        let tests: &[Row] = &[
            (
                "wrap even",
                wrap(4),
                "abcdefghij",
                &[(0, 4), (4, 8), (8, 10)],
            ),
            ("wrap exact", wrap(4), "abcdefgh", &[(0, 4), (4, 8)]),
            ("wrap short", wrap(10), "abc", &[(0, 3)]),
            ("wrap empty", wrap(10), "", &[(0, 0)]),
            ("wide moves down", wrap(5), "日本語", &[(0, 2), (2, 3)]),
            ("tab moves down", wrap(6), "abcd\tx", &[(0, 4), (4, 6)]),
            ("rune wider than width", wrap(1), "日本", &[(0, 1), (1, 2)]),
            ("nowrap", nowrap(4), "abcdefghij", &[(0, 10)]),
            ("zero width", wrap(0), "abc", &[(0, 3)]),
            (
                "words break after a space",
                words(6),
                "aaa bb cc",
                &[(0, 4), (4, 9)],
            ),
            ("words fit exactly", words(6), "aaa bb", &[(0, 6)]),
            (
                "words keep the space on the row",
                words(6),
                "aaaa b ccc",
                &[(0, 5), (5, 10)],
            ),
            (
                "long word breaks by rune",
                words(4),
                "abcdefghij",
                &[(0, 4), (4, 8), (8, 10)],
            ),
            (
                "long word after a short one",
                words(4),
                "ab cdefghij",
                &[(0, 3), (3, 7), (7, 11)],
            ),
            (
                "long run of spaces breaks by rune",
                words(6),
                "aaaa     b",
                &[(0, 6), (6, 10)],
            ),
            (
                "words break after a tab",
                words(10),
                "ab\tcd efgh",
                &[(0, 3), (3, 10)],
            ),
            (
                "words without spaces",
                words(5),
                "日本語",
                &[(0, 2), (2, 3)],
            ),
            (
                "a mark stays with its space",
                words(5),
                "abc \u{301}de",
                &[(0, 5), (5, 7)],
            ),
            (
                "wide rune past a moved word",
                words(5),
                " abcd日",
                &[(0, 1), (1, 5), (5, 6)],
            ),
            (
                "words nowrap",
                Layout {
                    width: 4,
                    mode: Mode::NoWrap,
                    tab: 0,
                    wrap_style: WrapStyle::Word,
                },
                "ab cd ef",
                &[(0, 8)],
            ),
            ("words zero width", words(0), "ab cd", &[(0, 5)]),
        ];
        for &(name, l, input, want) in tests {
            let ln = l.line(&chars(input));
            assert_eq!(ln.segments(), segs(want), "{name}");
            assert_eq!(ln.rows(), want.len(), "{name}");
        }
    }

    #[test]
    fn wide_cluster_moves_whole_to_next_row() {
        let ln = wrap(3).line(&chars("a日b"));
        assert_eq!(ln.segments(), segs(&[(0, 2), (2, 3)]));
        let ln = wrap(2).line(&chars("a👨\u{200d}👩b"));
        assert_eq!(ln.segments(), segs(&[(0, 1), (1, 4), (4, 5)]));
    }

    #[test]
    fn zero_width_joins_previous() {
        let ln = Layout::default().line(&chars("e\u{301}"));
        assert_eq!(ln.cells(), [0, 1, 1]);
        assert_eq!(ln.glyph(0), (0, 2));
        assert_eq!(ln.glyph(1), (0, 2));
    }

    #[test]
    fn crlf_is_two_controls() {
        assert_eq!(Layout::default().cells(&chars("\r\n")), [0, 2, 4]);
        assert_eq!(Layout::default().cells(&chars("é\r\n")), [0, 1, 3, 5]);
    }

    #[test]
    fn lone_zero_width_takes_no_cell() {
        for s in ["\u{200b}", "\u{ad}", "\u{feff}", "\u{301}"] {
            assert_eq!(Layout::default().cells(&chars(s)), [0, 0], "{s:?}");
        }
    }

    // Col cannot land on a char with no cell of its own, which is intended.
    fn zero_width_at(text: &[char], col: usize) -> bool {
        col < text.len() && Layout::default().char_width(text[col], 0) == 0
    }

    #[test]
    fn pos_col_round_trip() {
        let texts = [
            "",
            "abc",
            "abcdefghij",
            "日本語テキスト",
            "a\tb\tc",
            "x\ry",
            "ab cde  f ghijklmnop q",
            "e\u{301}abc",
        ];
        for s in texts {
            let text = chars(s);
            for mode in [Mode::Wrap, Mode::NoWrap] {
                for wrap_style in [WrapStyle::Char, WrapStyle::Word] {
                    for width in 1..=12 {
                        let l = Layout {
                            width,
                            mode,
                            tab: 0,
                            wrap_style,
                        };
                        let ln = l.line(&text);
                        for col in 0..=text.len() {
                            let (row, x) = ln.pos(col);
                            let got = ln.col(row, x);
                            assert!(
                                got == col || zero_width_at(&text, col),
                                "{s:?} {mode:?} {wrap_style} w={width}: col(pos({col})=({row},{x})) = {got}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn pos() {
        let ln = wrap(4).line(&chars("abcdefghij"));
        for (col, row, x) in [(0, 0, 0), (3, 0, 3), (4, 1, 0), (9, 2, 1), (10, 2, 2)] {
            assert_eq!(ln.pos(col), (row, x), "pos({col})");
        }
        assert_eq!(nowrap(4).line(&chars("abcdefghij")).pos(9), (0, 9));
    }

    #[test]
    fn col_never_inside_wide_rune() {
        let ln = nowrap(10).line(&chars("日本"));
        for (x, want) in [0, 0, 1, 1, 2, 2].into_iter().enumerate() {
            assert_eq!(ln.col(0, x), want, "col(x={x})");
        }
    }

    #[test]
    fn col_past_row_end() {
        let ln = wrap(4).line(&chars("abcdefghij"));
        assert_eq!(ln.col(0, 99), 3);
        assert_eq!(ln.col(2, 99), 10);
        assert_eq!(ln.col(99, 0), 8);
        // abce\u301 | fgh: the first row ends in a mark, which has no
        // cell; past the end is its base.
        let ln = wrap(4).line(&chars("abce\u{301}fgh"));
        assert_eq!(ln.col(0, 99), 3);
    }

    #[test]
    fn newline_row() {
        let tests: &[Row] = &[
            ("full", wrap(4), "abcd", &[(0, 4), (4, 4)]),
            (
                "full last row",
                wrap(4),
                "abcdefgh",
                &[(0, 4), (4, 8), (8, 8)],
            ),
            ("wider than width", wrap(1), "日", &[(0, 1), (1, 1)]),
            ("short", wrap(4), "abc", &[(0, 3)]),
            ("empty", wrap(4), "", &[(0, 0)]),
            ("nowrap", nowrap(4), "abcd", &[(0, 4)]),
            ("zero width", wrap(0), "abcd", &[(0, 4)]),
            (
                "words short last row",
                words(6),
                "aaa bb cc dd",
                &[(0, 4), (4, 10), (10, 12)],
            ),
            ("words full", words(6), "aaa bb", &[(0, 6), (6, 6)]),
        ];
        for &(name, l, input, want) in tests {
            let ln = l.newline_row(l.line(&chars(input)));
            assert_eq!(ln.segments(), segs(want), "{name}");
        }
    }

    #[test]
    fn segment_at() {
        let ln = wrap(4).line(&chars("abcdefghij"));
        for (col, want) in [(0, 0), (3, 0), (4, 1), (8, 2), (10, 2)] {
            assert_eq!(ln.segment_at(col), want, "segment_at({col})");
        }
    }

    #[test]
    fn tab_width() {
        let tests: &[(usize, &str, &[usize])] = &[
            (4, "a\tb", &[0, 1, 4, 5]),
            (4, "\t\t", &[0, 4, 8]),
            (4, "abcd\tx", &[0, 1, 2, 3, 4, 8, 9]),
            (1, "a\tb", &[0, 1, 2, 3]),
            (0, "a\tb", &[0, 1, 8, 9]),
        ];
        for &(tab, input, want) in tests {
            let l = Layout { tab, ..wrap(20) };
            assert_eq!(l.cells(&chars(input)), want, "tab {tab} {input:?}");
        }
        let l = Layout { tab: 4, ..wrap(6) };
        assert_eq!(
            l.line(&chars("ab\tcdef")).segments(),
            segs(&[(0, 5), (5, 7)])
        );
    }

    #[test]
    fn glyph() {
        // 👨ZWJ👩ZWJ👧 x e ́ : glyphs [0,5) [5,6) [6,8), then the newline.
        let ln = Layout::default().line(&chars("👨\u{200d}👩\u{200d}👧xe\u{301}"));
        let want = [
            (0, 5),
            (0, 5),
            (0, 5),
            (0, 5),
            (0, 5),
            (5, 6),
            (6, 8),
            (6, 8),
            (8, 9),
        ];
        for (col, want) in want.into_iter().enumerate() {
            assert_eq!(ln.glyph(col), want, "glyph({col})");
        }
    }

    #[test]
    fn wrap_style_round_trips() {
        for s in ["char", "word"] {
            assert_eq!(s.parse::<WrapStyle>().unwrap().to_string(), s);
        }
        assert!("line".parse::<WrapStyle>().is_err());
    }
}
