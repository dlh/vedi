//! Plain substrings in the buffer, with smartcase.

use std::ops::Range;

use crate::text::ansi;
use crate::text::buffer::{Buffer, Pos};
use crate::text::index::BLOCK_LINES;

/// A fixed pattern; one without upper-case letters ignores case.
#[derive(Clone, Debug)]
pub struct Matcher {
    src: String,
    pat: Vec<char>,
    fold: bool,
    /// `None` when bytes cannot tell: folding past ASCII, or U+FFFD,
    /// which stands for any invalid byte.
    bytes: Option<Prefilter>,
}

/// For ruling lines out by their bytes, undecoded.
#[derive(Clone, Debug)]
struct Prefilter {
    /// `pat` in UTF-8.
    raw: Vec<u8>,
    /// Every matching line holds one of these: the pattern's rarest
    /// byte, in both cases when folding.
    anchor: Vec<u8>,
    /// A folding pattern with k or i, which K and İ match.
    loose: bool,
}

impl Prefilter {
    fn anchored(&self, b: &[u8]) -> bool {
        self.anchor.iter().any(|c| b.contains(c))
    }
}

/// `char`'s simple lower case, the full mapping's first char: one char
/// for one keeps a match's length, and only İ lowers to more than one.
fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// The usual bytes of text, from the most frequent.
const COMMON: &[u8] = b" etaoinsr0123456789hld.-_/cumfpgwybvkxjqz";

/// How seldom text holds `c`; a byte not in `COMMON` is rarest.
fn rarity(c: u8) -> usize {
    COMMON.iter().position(|&x| x == c).unwrap_or(COMMON.len())
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    let Some((&first, rest)) = needle.split_first() else {
        return true;
    };
    let mut h = hay;
    while let Some(i) = h.iter().position(|&c| c == first) {
        if h[i + 1..].starts_with(rest) {
            return true;
        }
        h = &h[i + 1..];
    }
    false
}

impl Matcher {
    pub fn new(pattern: &str) -> Matcher {
        let mut pat: Vec<char> = pattern.chars().collect();
        let fold = !pat.iter().any(|c| c.is_uppercase());
        if fold {
            pat.iter_mut().for_each(|c| *c = lower(*c));
        }
        let mut m = Matcher {
            src: pattern.into(),
            pat,
            fold,
            bytes: None,
        };
        if m.pat
            .iter()
            .any(|&c| c == char::REPLACEMENT_CHARACTER || fold && !c.is_ascii())
        {
            return m;
        }
        let raw: Vec<u8> = m.pat.iter().collect::<String>().into_bytes();
        let mut loose = false;
        let mut anchor = None;
        for &c in &raw {
            // K and İ hold neither byte, so they are no anchor.
            if fold && (c == b'k' || c == b'i') {
                loose = true;
                continue;
            }
            if anchor.is_none_or(|a| rarity(c) >= rarity(a)) {
                anchor = Some(c);
            }
        }
        let Some(a) = anchor else {
            return m;
        };
        let mut anchor = vec![a];
        if fold && a.is_ascii_lowercase() {
            anchor.push(a.to_ascii_uppercase());
        }
        m.bytes = Some(Prefilter { raw, anchor, loose });
        m
    }

    /// The pattern as given to `new`.
    pub fn pattern(&self) -> &str {
        &self.src
    }

    pub fn is_empty(&self) -> bool {
        self.pat.is_empty()
    }

    /// In chars.
    pub fn len(&self) -> usize {
        self.pat.len()
    }

    /// Whether `line`, undecoded and without its `\n`, may hold a match:
    /// false only when its text cannot. `literal` is the buffer's: the
    /// text is then every byte.
    fn may_match(&self, line: &[u8], scratch: &mut Vec<u8>, literal: bool) -> bool {
        let Some(p) = &self.bytes else {
            return true;
        };
        if !p.anchored(line) {
            return false;
        }
        if literal {
            if !self.fold {
                // A C1 control (C2 80 to C2 9F) is dropped from literal
                // text too; contains is memchr, so ASCII pays little.
                return line.contains(&0xc2)
                    && line
                        .windows(2)
                        .any(|w| w[0] == 0xc2 && (0x80..=0x9f).contains(&w[1]))
                    || contains(line, &p.raw);
            }
            scratch.clear();
            scratch.extend_from_slice(line);
        } else {
            ansi::strip(line, scratch);
        }
        let mut prev = 0;
        for c in scratch.iter_mut() {
            match *c {
                0x80.. if p.loose => return true,
                // C1: dropped in every view.
                0x80..=0x9f if prev == 0xc2 => return true,
                // Dropped from the text, so a match may span it.
                0..0x20 | 0x7f if !literal && *c != b'\t' && *c != b'\r' => return true,
                b'A'..=b'Z' if self.fold => c.make_ascii_lowercase(),
                _ => {}
            }
            prev = *c;
        }
        contains(scratch, &p.raw)
    }

    fn match_at(&self, text: &[char], i: usize) -> bool {
        let Some(t) = text.get(i..i + self.pat.len()) else {
            return false;
        };
        !self.is_empty()
            && t.iter()
                .zip(&self.pat)
                .all(|(&c, &p)| p == if self.fold { lower(c) } else { c })
    }

    /// The first match starting at or after `from`.
    pub fn find(&self, text: &[char], from: usize) -> Option<usize> {
        let last = text.len().checked_sub(self.pat.len())?;
        (from..=last).find(|&i| self.match_at(text, i))
    }

    /// The last match starting before `before`.
    pub fn find_last(&self, text: &[char], before: usize) -> Option<usize> {
        let last = text.len().checked_sub(self.pat.len())?;
        (0..before.min(last + 1))
            .rev()
            .find(|&i| self.match_at(text, i))
    }

    /// Every match's start; matches may overlap.
    pub fn all(&self, text: &[char]) -> Vec<usize> {
        std::iter::successors(self.find(text, 0), |&i| self.find(text, i + 1)).collect()
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Found {
    pub pos: Pos,
    /// The search went past the buffer's end, or its start.
    pub wrapped: bool,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Dir {
    Forward,
    Back,
}

/// Searches lines whole, decoding only those whose bytes may match;
/// its buffers are reused from line to line.
struct Walk<'a> {
    buf: &'a Buffer,
    m: &'a Matcher,
    text: Vec<char>,
    raw: Vec<u8>,
    plain: Vec<u8>,
    /// `buf`'s, when the walk began.
    literal: bool,
}

impl<'a> Walk<'a> {
    fn new(buf: &'a Buffer, m: &'a Matcher) -> Walk<'a> {
        Walk {
            buf,
            m,
            text: Vec::new(),
            raw: Vec::new(),
            plain: Vec::new(),
            literal: buf.literal(),
        }
    }

    fn line(&mut self, i: usize) -> &[char] {
        self.buf.text(i, &mut self.text);
        &self.text
    }

    /// The first match on the first line of `lines` to hold one, or
    /// going back the last match on the last.
    fn find(&mut self, lines: Range<usize>, dir: Dir) -> Option<Pos> {
        if lines.is_empty() {
            return None;
        }
        let blocks = lines.start / BLOCK_LINES..=(lines.end - 1) / BLOCK_LINES;
        match dir {
            Dir::Forward => blocks.into_iter().find_map(|k| self.block(k, &lines, dir)),
            Dir::Back => blocks.rev().find_map(|k| self.block(k, &lines, dir)),
        }
    }

    /// `find` within block `k`.
    fn block(&mut self, k: usize, lines: &Range<usize>, dir: Dir) -> Option<Pos> {
        let Walk {
            buf,
            m,
            text,
            raw,
            plain,
            literal,
        } = self;
        let base = k * BLOCK_LINES;
        let mut starts = [0; BLOCK_LINES + 1];
        let mut n = BLOCK_LINES;
        // Unread bytes cannot rule a line out, so then each is decoded.
        let filter = m
            .bytes
            .as_ref()
            .and_then(|p| Some((p, buf.raw_block(k, raw).ok()?)));
        if let Some((p, raw)) = filter {
            if !p.anchored(raw) {
                return None;
            }
            n = 0;
            let mut at = 0;
            while at < raw.len() && n < BLOCK_LINES {
                at += raw[at..]
                    .iter()
                    .position(|&c| c == b'\n')
                    .unwrap_or(raw.len() - at)
                    + 1;
                n += 1;
                starts[n] = at;
            }
        }
        for step in 0..n {
            let j = match dir {
                Dir::Forward => step,
                Dir::Back => n - 1 - step,
            };
            let li = base + j;
            if !lines.contains(&li) {
                continue;
            }
            if let Some((_, raw)) = filter {
                let line = &raw[starts[j]..(starts[j + 1] - 1).min(raw.len())];
                if !m.may_match(line, plain, *literal) {
                    continue;
                }
            }
            buf.text(li, text);
            let col = match dir {
                Dir::Forward => m.find(text, 0),
                Dir::Back => m.find_last(text, usize::MAX),
            };
            if let Some(col) = col {
                return Some(Pos { line: li, col });
            }
        }
        None
    }
}

/// The first match at or after `from` (strictly after when `after`),
/// wrapping around the end.
pub fn next(buf: &Buffer, m: &Matcher, from: Pos, after: bool) -> Option<Found> {
    let n = buf.len();
    if n == 0 || m.is_empty() {
        return None;
    }
    let found = |pos, wrapped| Some(Found { pos, wrapped });
    let at = |col| Pos {
        line: from.line,
        col,
    };
    let mut w = Walk::new(buf, m);
    if let Some(i) = m.find(w.line(from.line), from.col + usize::from(after)) {
        return found(at(i), false);
    }
    if let Some(pos) = w.find(from.line + 1..n, Dir::Forward) {
        return found(pos, false);
    }
    if let Some(pos) = w.find(0..from.line, Dir::Forward) {
        return found(pos, true);
    }
    m.find(w.line(from.line), 0)
        .and_then(|i| found(at(i), true))
}

/// The last match before `from`, wrapping around the start.
pub fn prev(buf: &Buffer, m: &Matcher, from: Pos) -> Option<Found> {
    let n = buf.len();
    if n == 0 || m.is_empty() {
        return None;
    }
    let found = |pos, wrapped| Some(Found { pos, wrapped });
    let at = |col| Pos {
        line: from.line,
        col,
    };
    let mut w = Walk::new(buf, m);
    if let Some(i) = m.find_last(w.line(from.line), from.col) {
        return found(at(i), false);
    }
    if let Some(pos) = w.find(0..from.line, Dir::Back) {
        return found(pos, false);
    }
    if let Some(pos) = w.find(from.line + 1..n, Dir::Back) {
        return found(pos, true);
    }
    m.find_last(w.line(from.line), usize::MAX)
        .and_then(|i| found(at(i), true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::buffer::{Progress, Storage};
    use crate::text::index::Indexer;
    use std::sync::Arc;

    fn buffer(input: &[u8], literal: bool) -> Buffer {
        let mut ix = Indexer::new();
        let mut b = Buffer::new(Storage::Memory(Vec::new()), literal);
        let indexed = ix.feed(input);
        b.apply(Progress::Data {
            chunk: Some(Arc::from(input)),
            indexed,
        });
        let indexed = ix.finish();
        b.apply(Progress::Data {
            chunk: None,
            indexed,
        });
        b.apply(Progress::Done {
            err: None,
            trailing_newline: input.ends_with(b"\n"),
        });
        b
    }

    fn mk(lines: &[&str]) -> Buffer {
        buffer(lines.join("\n").as_bytes(), false)
    }

    fn at(line: usize, col: usize) -> Pos {
        Pos { line, col }
    }

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    fn found(line: usize, col: usize, wrapped: bool) -> Option<Found> {
        Some(Found {
            pos: at(line, col),
            wrapped,
        })
    }

    #[test]
    fn smartcase() {
        assert_eq!(Matcher::new("abc").find(&chars("xABCx"), 0), Some(1));
        assert_eq!(Matcher::new("Abc").find(&chars("xabcx"), 0), None);
        assert_eq!(Matcher::new("Abc").find(&chars("xAbcx"), 0), Some(1));
        assert_eq!(Matcher::new("").find(&chars("abc"), 0), None);
        assert!(Matcher::new("").is_empty());
        assert_eq!(Matcher::new("Abc").pattern(), "Abc");
        assert_eq!(Matcher::new("héllo").len(), 5);
    }

    #[test]
    fn find_from_and_last() {
        let m = Matcher::new("ab");
        let text = chars("ab ab ab");
        assert_eq!(m.find(&text, 1), Some(3));
        assert_eq!(m.find(&text, 7), None);
        assert_eq!(m.find_last(&text, 6), Some(3));
        assert_eq!(m.find_last(&text, 0), None);
        assert_eq!(Matcher::new("aa").all(&chars("aaaa")), [0, 1, 2]);
    }

    #[test]
    fn next_wraps() {
        let b = mk(&["foo", "bar foo", "foo"]);
        let m = Matcher::new("foo");
        for (from, after, want) in [
            (at(0, 0), false, found(0, 0, false)),
            (at(0, 0), true, found(1, 4, false)),
            (at(1, 4), true, found(2, 0, false)),
            (at(2, 0), true, found(0, 0, true)),
            (at(1, 5), false, found(2, 0, false)),
        ] {
            assert_eq!(next(&b, &m, from, after), want, "{from:?} after={after}");
        }
        assert_eq!(next(&b, &Matcher::new("zzz"), Pos::default(), false), None);
        assert_eq!(next(&buffer(b"", false), &m, Pos::default(), false), None);
    }

    #[test]
    fn prev_wraps() {
        let b = mk(&["foo", "bar foo", "foo"]);
        let m = Matcher::new("foo");
        for (from, want) in [
            (at(2, 0), found(1, 4, false)),
            (at(1, 4), found(0, 0, false)),
            (at(0, 0), found(2, 0, true)),
            (at(1, 7), found(1, 4, false)),
        ] {
            assert_eq!(prev(&b, &m, from), want, "{from:?}");
        }
    }

    #[test]
    fn single_match_wraps_to_itself() {
        let b = mk(&["x", "foo", "y"]);
        assert_eq!(
            next(&b, &Matcher::new("foo"), at(1, 0), true),
            found(1, 0, true)
        );
    }

    #[test]
    fn replacement_matches_invalid_byte() {
        let b = buffer(b"a\xff", false);
        assert_eq!(
            next(&b, &Matcher::new("a\u{FFFD}"), Pos::default(), false),
            found(0, 0, false)
        );
    }

    /// The bytes of a line rule it out only when its text cannot match.
    #[test]
    fn may_match() {
        for (pat, line, want) in [
            ("foo", "a foo", true),
            ("foo", "a FOO", true),
            ("foo", "fo\x1b[31mo", true),
            ("foo", "bar", false),
            ("foo", "fo", false),
            ("foo", "\x1b]0;foo\x07bar", false),
            ("foo", "ab\x1b]0;foo", false),
            ("Foo", "a foo", false),
            ("Foo", "a Foo", true),
            // A dropped control may sit inside a match.
            ("foo", "f\x01oo", true),
            ("foo", "f\x7foo", true),
            ("foo", "f\too", false),
            ("foo", "f\u{9b}oo", true),
            ("Foo", "F\u{9b}oo", true),
            // K and İ fold to k and i.
            ("kelvin", "\u{212A}elvin", true),
            ("istanbul", "\u{0130}stanbul", true),
            ("elvin", "\u{212A}elvin", true),
            ("foo", "\u{212A}elvin", false),
            // Bytes do not fold past ASCII, so they rule nothing out.
            ("é", "plain", true),
            ("É", "plain", false),
            ("É", "CAFÉ", true),
            ("\u{FFFD}", "plain", true),
            ("kiki", "plain", true),
        ] {
            let mut scratch = Vec::new();
            assert_eq!(
                Matcher::new(pat).may_match(line.as_bytes(), &mut scratch, false),
                want,
                "{pat:?} on {line:?}"
            );
        }
        // Literal text drops C1 too, folding or not.
        for pat in ["foo", "Foo"] {
            let mut scratch = Vec::new();
            assert!(Matcher::new(pat).may_match("F\u{9b}oo".as_bytes(), &mut scratch, true));
        }
    }

    /// `may_match` counts on K and İ being the only chars past ASCII that
    /// lower-case into it.
    #[test]
    fn only_two_chars_fold_to_ascii() {
        for c in ('\u{80}'..=char::MAX).filter(|c| lower(*c).is_ascii()) {
            assert!(
                matches!(c, '\u{212A}' | '\u{0130}'),
                "{c:?} lowers to {:?}",
                lower(c)
            );
        }
        assert_eq!((lower('\u{212A}'), lower('\u{0130}')), ('k', 'i'));
    }

    /// What `next` must agree with: every line decoded and searched.
    fn ref_next(buf: &Buffer, m: &Matcher, from: Pos, after: bool) -> Option<Found> {
        let n = buf.len();
        if n == 0 || m.is_empty() {
            return None;
        }
        let mut col = from.col + usize::from(after);
        let mut text = Vec::new();
        for k in 0..=n {
            let li = (from.line + k) % n;
            if k > 0 {
                col = 0;
            }
            buf.text(li, &mut text);
            if let Some(i) = m.find(&text, col) {
                return Some(Found {
                    pos: at(li, i),
                    wrapped: from.line + k >= n,
                });
            }
        }
        None
    }

    fn ref_prev(buf: &Buffer, m: &Matcher, from: Pos) -> Option<Found> {
        let n = buf.len();
        if n == 0 || m.is_empty() {
            return None;
        }
        let mut text = Vec::new();
        for k in 0..=n {
            let li = (from.line + n - k % n) % n;
            let before = if k == 0 { from.col } else { usize::MAX };
            buf.text(li, &mut text);
            if let Some(i) = m.find_last(&text, before) {
                return Some(Found {
                    pos: at(li, i),
                    wrapped: k > from.line,
                });
            }
        }
        None
    }

    /// From every position, `next` and `prev` find what decoding every
    /// line finds, over text that bytes alone misjudge and across block
    /// edges.
    #[test]
    fn walk_matches_every_line_decoded() {
        let odd = [
            "foo bar foo",
            "fo\x1b[31mo\x1b[0m",
            "f\x01oo",
            "FOO Foo",
            "\u{212A}elvin \u{0130}stanbul",
            "caf\u{e9} CAF\u{c9}",
            "",
            "zoo\r",
            "ab \x1b]0;foo",
            "\x1b]0;foo\x07bar",
            "",
        ];
        let mut lines: Vec<Vec<u8>> = (0..2 * BLOCK_LINES + 9)
            .map(|i| {
                if i % 13 == 5 {
                    match i / 13 % odd.len() {
                        6 => b"a\xffb".to_vec(),
                        o => odd[o].as_bytes().to_vec(),
                    }
                } else {
                    b"x".to_vec()
                }
            })
            .collect();
        lines[BLOCK_LINES - 1] = b"foo".to_vec();
        lines[BLOCK_LINES] = b"bar".to_vec();
        lines.push(b"end foo".to_vec());
        let mut crlf = lines[..3].join(&b"\r\n"[..]);
        crlf.extend_from_slice(b"\r\n");
        let inputs = [lines.join(&b'\n'), crlf, b"foo".to_vec(), b"x".to_vec()];
        let pats = [
            "foo",
            "Foo",
            "FOO",
            "oo",
            "bar",
            "k",
            "i",
            "ki",
            "kelvin",
            "istanbul",
            "é",
            "É",
            "café",
            "\u{FFFD}",
            "a\u{FFFD}b",
            "x",
            "zzz",
            "end",
        ];
        let mut text = Vec::new();
        for input in inputs {
            let buf = buffer(&input, false);
            for pat in pats {
                let m = Matcher::new(pat);
                for li in 0..buf.len() {
                    buf.text(li, &mut text);
                    for col in 0..=text.len() {
                        let from = at(li, col);
                        for after in [false, true] {
                            assert_eq!(
                                next(&buf, &m, from, after),
                                ref_next(&buf, &m, from, after),
                                "next({pat:?}, {from:?}, {after})"
                            );
                        }
                        assert_eq!(
                            prev(&buf, &m, from),
                            ref_prev(&buf, &m, from),
                            "prev({pat:?}, {from:?})"
                        );
                    }
                }
            }
        }
    }

    /// In a literal buffer a pattern finds an escape's text, which the
    /// byte check must not strip, and does not match across one.
    #[test]
    fn next_literal() {
        let mut buf = buffer(b"plain\nre\x1b[31md\nTAIL 31M\n", false);
        let start = Pos::default();
        assert_eq!(
            next(&buf, &Matcher::new("red"), start, false),
            found(1, 0, false)
        );
        buf.set_literal(true);
        assert_eq!(
            next(&buf, &Matcher::new("31m"), start, false),
            found(1, 4, false)
        );
        // Folded, and so found by its bytes lowered.
        assert_eq!(prev(&buf, &Matcher::new("31m"), start), found(2, 5, true));
        assert_eq!(next(&buf, &Matcher::new("red"), start, false), None);
        assert_eq!(
            next(&buf, &Matcher::new("RE\x1b"), start, false),
            None,
            "the pattern has capitals"
        );
        assert_eq!(next(&buf, &Matcher::new("E\x1b[31MD"), start, false), None);
        assert_eq!(
            next(&buf, &Matcher::new("re\x1b[31md"), at(1, 1), false),
            found(1, 0, true)
        );
    }
}
