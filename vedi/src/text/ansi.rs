//! Raw terminal output into text plus style runs.

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use crate::view::style::{Attrs, Color, Style};

const ESC: u8 = 0x1b;

/// An OSC 8 hyperlink. An empty `url` is no link, though it may carry
/// an `id`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Link {
    pub url: Arc<str>,
    pub id: Arc<str>,
}

/// A maximal range of chars `[start, end)` drawn in one style and link.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Run {
    pub start: usize,
    pub end: usize,
    pub style: Style,
    pub link: Option<Link>,
}

/// Converts lines to text and style runs. The state carries across
/// lines, as a terminal's would: a color stays until a reset, a link
/// until closed, a title until set again. The link is kept apart from
/// the style because a reset inside a link (ls --hyperlink does this)
/// must not end it. A clone holds the state at that point.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Parser {
    style: Style,
    link: Option<Link>,
    title: Arc<str>,
}

impl Parser {
    /// The window title in effect after the last line: "" for none, or
    /// one cleared since.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Replaces `text` and `runs` with one line's visible chars and the
    /// runs covering them. A line is given without its ending. SGR changes
    /// the style and OSC 8 the link; every other escape is skipped whole,
    /// as is one cut off by the end of the line. C0 controls other than
    /// `\t` and `\r` are dropped.
    pub fn parse(&mut self, line: &[u8], text: &mut Vec<char>, runs: &mut Vec<Run>) {
        runs.clear();
        self.parse_into(line, text, Some(runs));
    }

    /// `parse` without the runs.
    pub fn text(&mut self, line: &[u8], dst: &mut Vec<char>) {
        self.parse_into(line, dst, None);
    }

    /// Applies the line's escapes and reads nothing else, leaving the
    /// parser where `parse` would. Returns whether the line set the title,
    /// to the one it had included.
    pub fn skip(&mut self, mut line: &[u8]) -> bool {
        let mut titled = false;
        while let Some(i) = line.iter().position(|&c| c == ESC) {
            let Some((n, esc)) = escape(&line[i..]) else {
                break;
            };
            titled |= self.apply(esc);
            line = &line[i + n..];
        }
        titled
    }

    fn apply(&mut self, esc: Escape) -> bool {
        match esc {
            Escape::Sgr(params) => {
                self.style = apply_sgr(self.style, params);
                false
            }
            Escape::Osc(osc) => {
                // A control character, as an ESC, would end the OSC 8 the
                // renderer writes early and run what follows it.
                if let Some((url, id)) = link(osc) {
                    let (url, id) = (lossy(url), lossy(id));
                    if !url.chars().chain(id.chars()).any(char::is_control) {
                        self.link = (!url.is_empty() || !id.is_empty()).then_some(Link { url, id });
                    }
                }
                match window_title(osc) {
                    Some(title) => {
                        // Dropped as from content: a C1, as an ESC, would
                        // act on the terminal from the status line.
                        let title: String =
                            lossy(title).chars().filter(|c| !c.is_control()).collect();
                        self.title = title.into();
                        true
                    }
                    None => false,
                }
            }
            Escape::Other => false,
        }
    }

    fn parse_into(&mut self, line: &[u8], text: &mut Vec<char>, mut runs: Option<&mut Vec<Run>>) {
        text.clear();
        text.reserve(line.len());
        let mut start = 0;
        let (mut style, mut link) = (self.style, self.link.clone());
        let mut i = 0;
        while i < line.len() {
            match line[i] {
                ESC => {
                    let Some((n, esc)) = escape(&line[i..]) else {
                        break;
                    };
                    self.apply(esc);
                    if self.style != style || self.link != link {
                        close_run(&mut runs, &mut start, text.len(), style, &link);
                        (style, link) = (self.style, self.link.clone());
                    }
                    i += n;
                }
                c @ (b'\t' | b'\r') => {
                    text.push(c.into());
                    i += 1;
                }
                0..0x20 | 0x7f => i += 1,
                _ => {
                    let (c, n) = decode(&line[i..]);
                    // C0 is handled above; this drops C1, which some
                    // terminals take for escapes.
                    if !c.is_control() {
                        text.push(c);
                    }
                    i += n;
                }
            }
        }
        close_run(&mut runs, &mut start, text.len(), style, &link);
    }
}

fn close_run(
    runs: &mut Option<&mut Vec<Run>>,
    start: &mut usize,
    end: usize,
    style: Style,
    link: &Option<Link>,
) {
    if let Some(runs) = runs
        && end > *start
    {
        runs.push(Run {
            start: *start,
            end,
            style,
            link: link.clone(),
        });
    }
    *start = end;
}

/// Replaces `dst` with the line's bytes outside its escapes: what `parse`
/// decodes, before it drops controls.
pub fn strip(mut line: &[u8], dst: &mut Vec<u8>) {
    dst.clear();
    while let Some(i) = line.iter().position(|&c| c == ESC) {
        dst.extend_from_slice(&line[..i]);
        let Some((n, _)) = escape(&line[i..]) else {
            return;
        };
        line = &line[i + n..];
    }
    dst.extend_from_slice(line);
}

/// Replaces `dst` with the line's bytes as chars, escapes and controls
/// among them: the text `ViewStyle::Raw` shows.
pub fn literal(line: &[u8], dst: &mut Vec<char>) {
    dst.clear();
    let mut i = 0;
    while i < line.len() {
        let (c, n) = decode(&line[i..]);
        // C0 is drawn as ^X; C1 has no spelling and some terminals take it
        // for an escape.
        if !('\u{80}'..='\u{9f}').contains(&c) {
            dst.push(c);
        }
        i += n;
    }
}

/// How much of the text's styling is shown.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum ViewStyle {
    /// Colors, attributes and links.
    #[default]
    Color,
    /// Links alone.
    Plain,
    /// None: the escapes are text.
    Raw,
}

impl FromStr for ViewStyle {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "color" => Ok(ViewStyle::Color),
            "plain" => Ok(ViewStyle::Plain),
            "raw" => Ok(ViewStyle::Raw),
            _ => Err(()),
        }
    }
}

impl fmt::Display for ViewStyle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            ViewStyle::Color => "color",
            ViewStyle::Plain => "plain",
            ViewStyle::Raw => "raw",
        })
    }
}

/// Filters text bound for a terminal down to what `Parser` reads: chars,
/// SGR and OSC 8; with `Plain`, no SGR; with `Raw`, no escape, each ESC
/// and control spelled `^X` as the pager draws it. Text from anywhere can
/// then color itself and do nothing else: not move the cursor, not write
/// the clipboard.
#[derive(Clone, Debug)]
pub struct Printer {
    view: ViewStyle,
    styled: bool,
    linked: bool,
}

impl Printer {
    pub fn new(view: ViewStyle) -> Printer {
        Printer {
            view,
            styled: false,
            linked: false,
        }
    }

    /// Replaces `out` with the line, without its ending, filtered. A `\r`
    /// is spelled `^M`, as the pager draws it: raw, it would let later text
    /// cover earlier.
    pub fn line(&mut self, line: &[u8], out: &mut Vec<u8>) {
        out.clear();
        let mut i = 0;
        while i < line.len() {
            match line[i] {
                ESC if self.view != ViewStyle::Raw => {
                    let Some((n, esc)) = escape(&line[i..]) else {
                        return;
                    };
                    match esc {
                        Escape::Sgr(params) => {
                            if self.view == ViewStyle::Color {
                                out.extend_from_slice(&line[i..i + n]);
                                self.styled = !params.is_empty() && params != b"0";
                            }
                        }
                        Escape::Osc(osc) => {
                            if let Some((url, _)) = link(osc)
                                && std::str::from_utf8(osc)
                                    .is_ok_and(|s| !s.chars().any(char::is_control))
                            {
                                out.extend_from_slice(&line[i..i + n]);
                                self.linked = !url.is_empty();
                            }
                        }
                        _ => {}
                    }
                    i += n;
                }
                b'\r' => {
                    out.extend_from_slice(b"^M");
                    i += 1;
                }
                c @ (0..0x20 | 0x7f) if c != b'\t' => {
                    if self.view == ViewStyle::Raw {
                        out.extend_from_slice(&[b'^', c ^ 0x40]);
                    }
                    i += 1;
                }
                c @ 0..0x80 => {
                    out.push(c);
                    i += 1;
                }
                _ => {
                    let (c, n) = decode(&line[i..]);
                    if !c.is_control() {
                        out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
                    }
                    i += n;
                }
            }
        }
    }

    /// Replaces `out` with what ends a style or link the lines left on, so
    /// neither runs into what the terminal shows next.
    pub fn end(&mut self, out: &mut Vec<u8>) {
        out.clear();
        if self.styled {
            out.extend_from_slice(b"\x1b[m");
        }
        if self.linked {
            out.extend_from_slice(b"\x1b]8;;\x1b\\");
        }
        (self.styled, self.linked) = (false, false);
    }
}

/// Whether a CSI ending in m is SGR: digits and their separators only. One
/// with a private marker, ESC [ > 4 m, sets a terminal mode instead; one
/// with other junk a terminal ignores whole.
fn is_sgr(params: &[u8]) -> bool {
    params
        .iter()
        .all(|&c| c.is_ascii_digit() || c == b';' || c == b':')
}

enum Escape<'a> {
    /// The parameters between `ESC [` and `m`, possibly empty.
    Sgr(&'a [u8]),
    /// The body between `ESC ]` and its terminator, possibly empty.
    Osc(&'a [u8]),
    Other,
}

/// Scans the sequence at `b[0] == ESC` into its length and kind; `None`
/// when `b` ends first.
fn escape(b: &[u8]) -> Option<(usize, Escape<'_>)> {
    match *b.get(1)? {
        b'[' => {
            let i = 2 + b[2..].iter().position(|c| (0x40..=0x7e).contains(c))?;
            let kind = if b[i] == b'm' && is_sgr(&b[2..i]) {
                Escape::Sgr(&b[2..i])
            } else {
                Escape::Other
            };
            Some((i + 1, kind))
        }
        // OSC, DCS, APC, PM, SOS: a string ending at ESC \, or BEL for OSC.
        kind @ (b']' | b'P' | b'_' | b'^' | b'X') => {
            let osc = kind == b']';
            (2..b.len()).find_map(|i| {
                let n = match b[i] {
                    0x07 if osc => i + 1,
                    ESC if b.get(i + 1) == Some(&b'\\') => i + 2,
                    _ => return None,
                };
                Some((
                    n,
                    if osc {
                        Escape::Osc(&b[2..i])
                    } else {
                        Escape::Other
                    },
                ))
            })
        }
        // Charset select: one more byte.
        b'(' | b')' | b'*' | b'+' => (b.len() >= 3).then_some((3, Escape::Other)),
        _ => Some((2, Escape::Other)),
    }
}

/// Decodes an OSC 8 body `8;params;url` into the url and its id param;
/// `None` for any other OSC. An empty url ends the link.
fn link(osc: &[u8]) -> Option<(&[u8], &[u8])> {
    let (params, url) = split_once(osc.strip_prefix(b"8;")?, b';')?;
    let id = params
        .split(|&c| c == b':')
        .filter_map(|kv| kv.strip_prefix(b"id="))
        .next_back()
        .unwrap_or_default();
    Some((url, id))
}

/// Decodes an OSC 0 or 2 body `2;title` into the title. OSC 1 names the
/// icon alone.
fn window_title(osc: &[u8]) -> Option<&[u8]> {
    match osc {
        [b'0' | b'2', b';', title @ ..] => Some(title),
        _ => None,
    }
}

fn lossy(b: &[u8]) -> Arc<str> {
    String::from_utf8_lossy(b).into()
}

/// The char at the start of `b`, which is not empty, and its length. Each
/// byte of an invalid sequence is one U+FFFD, as a terminal draws it.
fn decode(b: &[u8]) -> (char, usize) {
    let n = match b[0] {
        0..0x80 => 1,
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => 0,
    };
    b.get(..n)
        .and_then(|s| std::str::from_utf8(s).ok())
        .and_then(|s| s.chars().next())
        .map_or((char::REPLACEMENT_CHARACTER, 1), |c| (c, n))
}

/// Applies SGR parameters to `s`: `1;38;5;208`, `38:2::255:0:0` (the
/// T.416 colon form), or empty for a reset.
fn apply_sgr(mut s: Style, mut params: &[u8]) -> Style {
    if params.is_empty() {
        return Style::default();
    }
    let mut more = true;
    while more {
        let group;
        (group, params, more) = cut(params, b';');
        let (code, sub, has_sub) = cut(group, b':');
        let n = atoi(code);
        match n {
            0 => s = Style::default(),
            1 => s.attrs.insert(Attrs::BOLD),
            2 => s.attrs.insert(Attrs::DIM),
            3 => s.attrs.insert(Attrs::ITALIC),
            // 4:0 is "no underline"; 4:1 to 4:5 are underline styles.
            4 if has_sub && cut(sub, b':').0 == b"0" => s.attrs.remove(Attrs::UNDERLINE),
            4 => s.attrs.insert(Attrs::UNDERLINE),
            7 => s.attrs.insert(Attrs::REVERSE),
            9 => s.attrs.insert(Attrs::STRIKE),
            22 => s.attrs.remove(Attrs::BOLD | Attrs::DIM),
            23 => s.attrs.remove(Attrs::ITALIC),
            24 => s.attrs.remove(Attrs::UNDERLINE),
            27 => s.attrs.remove(Attrs::REVERSE),
            29 => s.attrs.remove(Attrs::STRIKE),
            30..=37 => s.fg = Color::Palette((n - 30) as u8),
            39 => s.fg = Color::Default,
            40..=47 => s.bg = Color::Palette((n - 40) as u8),
            49 => s.bg = Color::Default,
            90..=97 => s.fg = Color::Palette((n - 90 + 8) as u8),
            100..=107 => s.bg = Color::Palette((n - 100 + 8) as u8),
            // 58 is underline color: its arguments are taken, then dropped.
            38 | 48 | 58 => {
                let (mut args, mut sep) = (sub, b':');
                if !has_sub && more {
                    (args, params, more) = take_color_args(params);
                    sep = b';';
                }
                match (n, extended_color(args, sep)) {
                    (38, Some(c)) => s.fg = c,
                    (48, Some(c)) => s.bg = c,
                    _ => {}
                }
            }
            _ => {}
        }
    }
    s
}

/// Cuts the semicolon-form arguments of a 38, 48 or 58 from the groups
/// after it: `5;n` or `2;r;g;b`. A malformed tail is taken whole and
/// yields nothing.
fn take_color_args(rest: &[u8]) -> (&[u8], &[u8], bool) {
    let k = match cut(rest, b';').0 {
        b"5" => 2,
        b"2" => 4,
        _ => 0,
    };
    if k == 0 || count(rest, b';') + 1 < k {
        return (b"", b"", false);
    }
    cut_after(rest, b';', k)
}

/// Decodes `5 n` or `2 [colorspace] r g b` arguments, separated by `sep`.
fn extended_color(args: &[u8], sep: u8) -> Option<Color> {
    let (kind, rest, _) = cut(args, sep);
    let n = count(args, sep) + 1;
    match kind {
        b"5" if n >= 2 => Some(Color::Palette(clamp(atoi(rest)))),
        b"2" if n >= 4 => {
            let (_, rgb, _) = cut_after(args, sep, n - 3);
            let (r, gb, _) = cut(rgb, sep);
            let (g, b, _) = cut(gb, sep);
            Some(Color::Rgb(clamp(atoi(r)), clamp(atoi(g)), clamp(atoi(b))))
        }
        _ => None,
    }
}

fn split_once(b: &[u8], sep: u8) -> Option<(&[u8], &[u8])> {
    let i = b.iter().position(|&c| c == sep)?;
    Some((&b[..i], &b[i + 1..]))
}

/// Splits `b` at the first `sep`: the field before it, the rest after, and
/// whether there was one.
fn cut(b: &[u8], sep: u8) -> (&[u8], &[u8], bool) {
    split_once(b, sep).map_or((b, b"", false), |(h, t)| (h, t, true))
}

/// Splits `b` after its `k`-th field, which must exist.
fn cut_after(b: &[u8], sep: u8, k: usize) -> (&[u8], &[u8], bool) {
    let mut end = 0;
    for _ in 0..k {
        let Some(i) = b[end..].iter().position(|&c| c == sep) else {
            return (b, b"", false);
        };
        end += i + 1;
    }
    (&b[..end - 1], &b[end..], true)
}

fn count(b: &[u8], sep: u8) -> usize {
    b.iter().filter(|&&c| c == sep).count()
}

/// The decimal in `b`: 0 for anything but digits and a sign, the nearest
/// extreme out of range.
fn atoi(b: &[u8]) -> i64 {
    let (neg, digits) = match b {
        [b'-', rest @ ..] => (true, rest),
        [b'+', rest @ ..] => (false, rest),
        _ => (false, b),
    };
    let mut n: i64 = 0;
    for &c in digits {
        if !c.is_ascii_digit() {
            return 0;
        }
        n = n.saturating_mul(10).saturating_add((c - b'0').into());
    }
    if neg { -n } else { n }
}

fn clamp(n: i64) -> u8 {
    n.clamp(0, 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::style::{Attrs, Color, Style};

    fn run(start: usize, end: usize, style: Style) -> Run {
        Run {
            start,
            end,
            style,
            link: None,
        }
    }

    fn linked(start: usize, end: usize, style: Style, url: &str, id: &str) -> Run {
        Run {
            start,
            end,
            style,
            link: Some(Link {
                url: url.into(),
                id: id.into(),
            }),
        }
    }

    fn parse(p: &mut Parser, line: &[u8]) -> (String, Vec<Run>) {
        let (mut text, mut runs) = (Vec::new(), Vec::new());
        p.parse(line, &mut text, &mut runs);
        (text.into_iter().collect(), runs)
    }

    fn runs_of(line: &[u8]) -> Vec<Run> {
        parse(&mut Parser::default(), line).1
    }

    fn red() -> Style {
        Style::default().fg(Color::Palette(1))
    }

    #[test]
    fn parse_text() {
        let tests: &[(&str, &[u8], &str)] = &[
            ("plain", "héllo".as_bytes(), "héllo"),
            ("csi skipped", b"a\x1b[2Kb", "ab"),
            ("osc bel", b"a\x1b]0;title\x07b", "ab"),
            ("osc st", b"a\x1b]8;;\x1b\\b", "ab"),
            ("charset", b"a\x1b(Bb", "ab"),
            ("dcs st", b"a\x1bPq#0;2;0;0;0#0~~@@\x1b\\b", "ab"),
            ("apc st", b"a\x1b_Gf=100,a=T;AAAA\x1b\\b", "ab"),
            ("pm st", b"a\x1b^x\x1b\\b", "ab"),
            ("sos st", b"a\x1bXx\x1b\\b", "ab"),
            ("truncated apc", b"abc\x1b_Gf=100", "abc"),
            ("esc single", b"a\x1b7b", "ab"),
            ("c0 dropped", b"a\x07b\x08c", "abc"),
            ("del dropped", b"a\x7fb", "ab"),
            (
                "c1 dropped",
                "a\u{9b}2Jb x\u{9d}52;c;aGk=\u{9c}y".as_bytes(),
                "a2Jb x52;c;aGk=y",
            ),
            ("tab and cr kept", b"a\tb\rc", "a\tb\rc"),
            ("truncated csi", b"abc\x1b[3", "abc"),
            ("truncated esc", b"abc\x1b", "abc"),
            ("truncated osc", b"abc\x1b]8;;foo", "abc"),
            ("truncated charset", b"abc\x1b(", "abc"),
            ("invalid utf8", b"a\xffb", "a\u{fffd}b"),
            ("empty", b"", ""),
        ];
        for &(name, input, want) in tests {
            let (text, runs) = parse(&mut Parser::default(), input);
            assert_eq!(text, want, "{name}: text");
            let n = text.chars().count();
            let want_runs = if n == 0 {
                vec![]
            } else {
                vec![run(0, n, Style::default())]
            };
            assert_eq!(runs, want_runs, "{name}: runs");
        }
    }

    #[test]
    fn invalid_utf8_is_replacement() {
        let (mut text, mut runs) = (Vec::new(), Vec::new());
        Parser::default().parse(b"a\xffb", &mut text, &mut runs);
        assert_eq!(text, ['a', '\u{FFFD}', 'b']);
    }

    #[test]
    fn parse_styles() {
        let d = Style::default();
        let red = red();
        let rgb = Color::Rgb(1, 2, 3);
        let tests: &[(&str, &[u8], Vec<Run>)] = &[
            (
                "fg then reset",
                b"\x1b[31mred\x1b[0m plain",
                vec![run(0, 3, red), run(3, 9, d)],
            ),
            (
                "modifier mode is not a reset",
                b"\x1b[31mred\x1b[>4;2m x\x1b[?5m y",
                vec![run(0, 7, red)],
            ),
            (
                "bold underline",
                b"\x1b[1;4mx",
                vec![run(0, 1, d.with(Attrs::BOLD | Attrs::UNDERLINE))],
            ),
            (
                "256 semicolon",
                b"\x1b[38;5;208mx",
                vec![run(0, 1, d.fg(Color::Palette(208)))],
            ),
            (
                "256 colon",
                b"\x1b[38:5:208mx",
                vec![run(0, 1, d.fg(Color::Palette(208)))],
            ),
            (
                "rgb semicolon",
                b"\x1b[38;2;1;2;3mx",
                vec![run(0, 1, d.fg(rgb))],
            ),
            (
                "rgb colon colorspace",
                b"\x1b[38:2::1:2:3mx",
                vec![run(0, 1, d.fg(rgb))],
            ),
            (
                "rgb bg colon",
                b"\x1b[48:2:1:2:3mx",
                vec![run(0, 1, d.bg(rgb))],
            ),
            (
                "bright fg",
                b"\x1b[93mx",
                vec![run(0, 1, d.fg(Color::Palette(11)))],
            ),
            (
                "bright bg",
                b"\x1b[103mx",
                vec![run(0, 1, d.bg(Color::Palette(11)))],
            ),
            (
                "basic bg",
                b"\x1b[44mx",
                vec![run(0, 1, d.bg(Color::Palette(4)))],
            ),
            (
                "empty sgr resets",
                b"\x1b[31ma\x1b[mb",
                vec![run(0, 1, red), run(1, 2, d)],
            ),
            (
                "bold off",
                b"\x1b[1mbold\x1b[22mnot",
                vec![run(0, 4, d.bold()), run(4, 7, d)],
            ),
            (
                "two sgrs one run",
                b"\x1b[31m\x1b[1mx",
                vec![run(0, 1, red.bold())],
            ),
            ("same style no split", b"a\x1b[mb", vec![run(0, 2, d)]),
            (
                "all attrs",
                b"\x1b[2;3;7;9mx",
                vec![run(
                    0,
                    1,
                    d.with(Attrs::DIM | Attrs::ITALIC | Attrs::REVERSE | Attrs::STRIKE),
                )],
            ),
            (
                "attrs off",
                b"\x1b[3;7;9m\x1b[23;27;29mx",
                vec![run(0, 1, d)],
            ),
            ("fg default", b"\x1b[31m\x1b[39mx", vec![run(0, 1, d)]),
            ("bg default", b"\x1b[44m\x1b[49mx", vec![run(0, 1, d)]),
            (
                "underline style on",
                b"\x1b[4:3mx",
                vec![run(0, 1, d.with(Attrs::UNDERLINE))],
            ),
            (
                "underline style off",
                b"\x1b[4m\x1b[4:0mx",
                vec![run(0, 1, d)],
            ),
            (
                "underline color ignored",
                b"\x1b[58:2:1:2:3mx",
                vec![run(0, 1, d)],
            ),
            ("malformed 256", b"\x1b[38;5mx", vec![run(0, 1, d)]),
            (
                "malformed rgb takes the tail",
                b"\x1b[38;2;1;1mx",
                vec![run(0, 1, d)],
            ),
            (
                "rgb then bold",
                b"\x1b[38;2;1;2;3;1mx",
                vec![run(0, 1, d.fg(rgb).bold())],
            ),
            (
                "huge code ignored",
                b"\x1b[31m\x1b[99999999999999999999mx",
                vec![run(0, 1, red)],
            ),
            ("unknown code ignored", b"\x1b[99mx", vec![run(0, 1, d)]),
            (
                "kitty line prefix",
                b"\x1b[m\x1b[31mx",
                vec![run(0, 1, red)],
            ),
        ];
        for (name, input, want) in tests {
            assert_eq!(&runs_of(input), want, "{name}");
        }
    }

    #[test]
    fn sgr_blink_and_double_underline_are_ignored() {
        for code in [&b"\x1b[5mx"[..], b"\x1b[6mx", b"\x1b[21mx", b"\x1b[25mx"] {
            let mut line = b"\x1b[31m".to_vec();
            line.extend_from_slice(code);
            assert_eq!(runs_of(&line), [run(0, 1, red())], "{code:?}");
        }
    }

    #[test]
    fn osc8_empty_url_keeps_id() {
        assert_eq!(
            runs_of(b"\x1b]8;id=k;\x1b\\a"),
            [linked(0, 1, Style::default(), "", "k")]
        );
    }

    #[test]
    fn style_carries_across_lines() {
        let mut p = Parser::default();
        parse(&mut p, b"\x1b[31mline one");
        assert_eq!(parse(&mut p, b"two").1, [run(0, 3, red())]);
        assert_eq!(
            parse(&mut p, b"\x1b[mthree").1,
            [run(0, 5, Style::default())]
        );
    }

    #[test]
    fn parse_links() {
        let d = Style::default();
        let red = red();
        let tests: &[(&str, &[u8], Vec<Run>)] = &[
            (
                "link st",
                b"\x1b]8;;http://x\x1b\\a\x1b]8;;\x1b\\b",
                vec![linked(0, 1, d, "http://x", ""), run(1, 2, d)],
            ),
            (
                "link bel",
                b"\x1b]8;;http://x\x07a\x1b]8;;\x07b",
                vec![linked(0, 1, d, "http://x", ""), run(1, 2, d)],
            ),
            (
                "link id",
                b"\x1b]8;id=k;http://x\x1b\\a",
                vec![linked(0, 1, d, "http://x", "k")],
            ),
            (
                "other params ignored",
                b"\x1b]8;foo=1:id=k;http://x\x1b\\a",
                vec![linked(0, 1, d, "http://x", "k")],
            ),
            (
                "sgr inside link",
                b"\x1b]8;;http://x\x1b\\\x1b[31ma\x1b[0mb\x1b]8;;\x1b\\",
                vec![
                    linked(0, 1, red, "http://x", ""),
                    linked(1, 2, d, "http://x", ""),
                ],
            ),
            (
                "new link drops id",
                b"\x1b]8;id=k;http://x\x1b\\a\x1b]8;;http://y\x1b\\b",
                vec![
                    linked(0, 1, d, "http://x", "k"),
                    linked(1, 2, d, "http://y", ""),
                ],
            ),
            (
                "other osc ignored",
                b"\x1b]0;title\x07a",
                vec![run(0, 1, d)],
            ),
            (
                "malformed osc 8",
                b"\x1b]8;http://x\x07a",
                vec![run(0, 1, d)],
            ),
            (
                "control in url or id",
                b"\x1b]8;;http://x\x1b\\a\x1b]8;;http://y\x1b]52;c;aGk=\x1b\\b\x1b]8;id=\x7f;http://z\x07c",
                vec![linked(0, 3, d, "http://x", "")],
            ),
        ];
        for (name, input, want) in tests {
            assert_eq!(&runs_of(input), want, "{name}");
        }
    }

    #[test]
    fn link_carries_across_lines() {
        let mut p = Parser::default();
        parse(&mut p, b"\x1b]8;;http://x\x1b\\one");
        assert_eq!(
            parse(&mut p, b"two").1,
            [linked(0, 3, Style::default(), "http://x", "")]
        );
    }

    // Skip leaves the parser where parse would, line after line, escapes
    // cut off at a line end included.
    #[test]
    fn skip_matches_parse() {
        let lines: &[&[u8]] = &[
            b"plain",
            b"\x1b[31mred",
            b"still red",
            b"\x1b]8;;http://x\x1b\\link",
            b"\x1b[1mbold in link",
            b"\x1b[0mreset keeps link",
            b"\x1b]8;;\x07unlinked",
            b"\x1b(Bcharset",
            b"cut \x1b[3",
            b"after cut",
            "\x1b[38;2;1;2;3mrgb ü \x1b[4:0mno underline".as_bytes(),
            b"\x1b]0;title\x07title",
            b"\x1b]2;\x07cleared",
            b"",
        ];
        let (mut by_parse, mut by_skip) = (Parser::default(), Parser::default());
        for l in lines {
            parse(&mut by_parse, l);
            by_skip.skip(l);
            assert_eq!(by_parse, by_skip, "after {:?}", String::from_utf8_lossy(l));
        }
    }

    // The window title OSC 0 or 2 sets carries across lines like the
    // link until set again; an empty title clears it.
    #[test]
    fn title() {
        let tests: &[(&str, &[u8], &str)] = &[
            ("osc 2 bel", b"\x1b]2;vedi\x07text", "vedi"),
            ("osc 2 st", b"\x1b]2;vedi\x1b\\text", "vedi"),
            ("osc 0", b"\x1b]0;both\x07", "both"),
            ("osc 1 is the icon", b"\x1b]1;icon\x07", "was"),
            ("empty clears", b"\x1b]2;\x07", ""),
            ("last wins", b"\x1b]2;one\x07\x1b[31m\x1b]0;two\x07", "two"),
            ("semicolons kept", b"\x1b]2;a;b\x07", "a;b"),
            ("controls dropped", b"\x1b]2;a\xc2\x9bb\x7fc\x07", "abc"),
            ("no osc keeps it", b"plain \x1b[1mtext", "was"),
            ("link is not a title", b"\x1b]8;;http://x\x07", "was"),
            ("cut off", b"\x1b]2;ved", "was"),
            ("sgr reset keeps it", b"\x1b[0m", "was"),
        ];
        for &(name, input, want) in tests {
            let mut p = Parser::default();
            parse(&mut p, b"\x1b]2;was\x07");
            parse(&mut p, input);
            assert_eq!(p.title(), want, "{name}");
        }
    }

    // Skip says whether the line set a title, the same one again and an
    // empty one included.
    #[test]
    fn skip_reports_title() {
        let tests: &[(&[u8], bool)] = &[
            (b"\x1b]2;hunk\x07@@", true),
            (b"\x1b[34m\x1b]0;hunk\x1b\\@@\x1b[0m", true),
            (b"\x1b]2;\x07", true),
            (b"plain \x1b[1mtext", false),
            (b"\x1b]8;;http://x\x07link", false),
            (b"\x1b]1;icon\x07", false),
            (b"\x1b]2;cut", false),
        ];
        let mut p = Parser::default();
        for &(input, want) in tests {
            assert_eq!(p.skip(input), want, "{:?}", String::from_utf8_lossy(input));
        }
    }

    // text gives parse's chars, into a reused vec, and moves the state the
    // same way.
    #[test]
    fn text_matches_parse() {
        let lines: &[&[u8]] = &[
            b"a\x1b[31mb",
            b"c\td\x01e",
            "ü\x1b]8;;u\x07x".as_bytes(),
            b"cut \x1b[3",
            b"after",
        ];
        let (mut p, mut q) = (Parser::default(), Parser::default());
        let mut dst = vec!['?'; 8];
        for l in lines {
            let (want, _) = parse(&mut p, l);
            q.text(l, &mut dst);
            assert_eq!(dst.iter().collect::<String>(), want);
            assert_eq!(p, q);
        }
    }

    // A clone keeps the state at the clone.
    #[test]
    fn parser_is_a_value() {
        let mut p = Parser::default();
        parse(&mut p, b"\x1b[31m");
        let mut q = p.clone();
        parse(&mut p, b"\x1b[0m");
        assert_eq!(parse(&mut q, b"x").1, [run(0, 1, red())]);
    }

    // Strip keeps every byte outside an escape, and nothing from a cut-off
    // escape on.
    #[test]
    fn strip_drops_escapes() {
        let tests: &[(&[u8], &[u8])] = &[
            (b"plain", b"plain"),
            (b"", b""),
            (b"fo\x1b[31mo\x1b[0m", b"foo"),
            (b"\x1b]0;title\x07a\x1b]8;;http://x\x1b\\b", b"ab"),
            (b"a\x1b(Bb\x1b=c", b"abc"),
            (b"a\x01b\tc\r", b"a\x01b\tc\r"),
            (b"caf\xc3\xa9 \xff", b"caf\xc3\xa9 \xff"),
            (b"ab\x1b[3", b"ab"),
            (b"ab\x1b]0;foo", b"ab"),
        ];
        let mut dst = b"stale".to_vec();
        for &(input, want) in tests {
            strip(input, &mut dst);
            assert_eq!(dst, want, "{:?}", String::from_utf8_lossy(input));
        }
    }

    fn printed(p: &mut Printer, line: &[u8]) -> Vec<u8> {
        let mut out = b"stale".to_vec();
        p.line(line, &mut out);
        out
    }

    fn ended(p: &mut Printer) -> Vec<u8> {
        let mut out = b"stale".to_vec();
        p.end(&mut out);
        out
    }

    // A Printer keeps text, SGR and OSC 8 and drops what else could act on
    // the terminal: other escapes, one cut off by the line's end, controls,
    // and a CSI ending in m that is not SGR. A carriage return is spelled
    // out.
    #[test]
    fn printer_keeps_colors_and_links() {
        let tests: &[(&[u8], &[u8])] = &[
            (b"plain", b"plain"),
            (b"", b""),
            (b"a\tb", b"a\tb"),
            (b"\x1b[31mred\x1b[0m", b"\x1b[31mred\x1b[0m"),
            (
                b"\x1b[38:2::1:2:3;4:3mx\x1b[m",
                b"\x1b[38:2::1:2:3;4:3mx\x1b[m",
            ),
            (
                b"\x1b]8;id=a;http://x\x07link\x1b]8;;\x1b\\",
                b"\x1b]8;id=a;http://x\x07link\x1b]8;;\x1b\\",
            ),
            (b"\x1b]2;hunk\x07@@ one", b"@@ one"),
            (b"a\x1b]52;c;aGk=\x07b", b"ab"),
            (b"a\x1b]1337;File=x\x07b", b"ab"),
            (b"a\x1b[2J\x1b[H\x1b[3Ab", b"ab"),
            (b"a\x1b[>4;2mb", b"ab"),
            (b"a\x1b[?1049hb", b"ab"),
            (b"a\x1bP+q\x1b\\b\x1bcc", b"abc"),
            (b"ab\x1b]52;cut", b"ab"),
            (b"ab\x1b[31", b"ab"),
            (b"real\rfake\x08\x07\x7f\x0e", b"real^Mfake"),
            ("a\u{9b}2Jb\u{9d}52;c\u{9c}".as_bytes(), b"a2Jb52;c"),
            (b"a\x9b2Jb", "a\u{fffd}2Jb".as_bytes()),
            ("\x1b]8;;http://x\u{9c}\x07a".as_bytes(), b"a"),
            (b"\x1b]8;;http://\xffx\x07a", b"a"),
        ];
        for &(input, want) in tests {
            let got = printed(&mut Printer::new(ViewStyle::Color), input);
            assert_eq!(
                got,
                want,
                "line({:?}) = {:?}",
                String::from_utf8_lossy(input),
                String::from_utf8_lossy(&got)
            );
        }
    }

    // End resets a style and closes a link the text left on, across lines,
    // and adds nothing when it left neither.
    #[test]
    fn printer_end() {
        let tests: &[(&[&[u8]], &[u8])] = &[
            (&[b"plain"], b""),
            (&[b"\x1b[31mred\x1b[0m", b"\x1b[1mb\x1b[m"], b""),
            (&[b"\x1b[31mred", b"more"], b"\x1b[m"),
            (&[b"\x1b[0;1mbold"], b"\x1b[m"),
            (&[b"\x1b]8;;http://x\x07link"], b"\x1b]8;;\x1b\\"),
            (&[b"\x1b]8;;http://x\x07link\x1b]8;;\x07"], b""),
            (
                &[b"\x1b[4m\x1b]8;;http://x\x07link"],
                b"\x1b[m\x1b]8;;\x1b\\",
            ),
        ];
        for &(lines, want) in tests {
            let mut p = Printer::new(ViewStyle::Color);
            for line in lines {
                printed(&mut p, line);
            }
            assert_eq!(ended(&mut p), want, "after {lines:?}");
            assert_eq!(ended(&mut p), b"", "second end after {lines:?}");
        }
    }

    // A plain Printer drops SGR and keeps links, and has no style to end.
    #[test]
    fn printer_plain() {
        let mut p = Printer::new(ViewStyle::Plain);
        let line = printed(&mut p, b"\x1b[1;31mred\x1b[0m \x1b]8;;http://x\x07link");
        assert_eq!(line, b"red \x1b]8;;http://x\x07link");
        assert_eq!(ended(&mut p), b"\x1b]8;;\x1b\\");
    }

    // Literal decodes every byte as text: an escape's, a control's, and an
    // invalid one as U+FFFD.
    #[test]
    fn literal_keeps_escapes() {
        let mut dst = vec!['?'; 4];
        literal(b"\x1b[1ma\x07\tb\xff\xc3\xa9\xc2\x9b2J", &mut dst);
        assert_eq!(
            dst.iter().collect::<String>(),
            "\x1b[1ma\x07\tb\u{fffd}\u{e9}2J"
        );
    }

    // A raw Printer spells ESC and every control but tab as the pager draws
    // them, so no sequence reaches the terminal.
    #[test]
    fn printer_raw() {
        let mut p = Printer::new(ViewStyle::Raw);
        let line = printed(
            &mut p,
            "\x1b[31mred\x1b]8;;http://x\x07a\tb\r\x7f\u{9b}2J".as_bytes(),
        );
        assert_eq!(line, b"^[[31mred^[]8;;http://x^Ga\tb^M^?2J");
        assert_eq!(ended(&mut p), b"");
    }

    #[test]
    fn view_style_names() {
        for (name, view) in [
            ("color", ViewStyle::Color),
            ("plain", ViewStyle::Plain),
            ("raw", ViewStyle::Raw),
        ] {
            assert_eq!(name.parse::<ViewStyle>(), Ok(view));
            assert_eq!(view.to_string(), name);
        }
        assert_eq!("bold".parse::<ViewStyle>(), Err(()));
    }
}
