//! The terminal's input, parsed: keys, the mouse, pastes and the
//! replies to `term`'s queries.

use crate::app::{Mouse, MouseKind};
use crate::input::key::{Key, KeyCode, Mods};

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Input {
    Key(Key),
    Mouse(Mouse),
    Paste(String),
    /// The reply to `CSI ? u`: the kitty keyboard flags in effect.
    KeyboardFlags(u32),
    /// The reply to DA1, which every terminal sends.
    DeviceAttributes,
}

const ESC: u8 = 0x1b;
const PASTE_END: &[u8] = b"\x1b[201~";

/// Bytes in, [`Input`]s out. A sequence may be split across feeds.
#[derive(Default)]
pub struct Parser {
    /// What no input has been made of yet.
    buf: Vec<u8>,
    /// Inside a paste, how much of `buf` holds no end of it.
    paste: Option<usize>,
}

enum Step {
    /// That many bytes are done with.
    Done(usize),
    /// The same, and a paste follows.
    Paste(usize),
    /// The sequence is not all here.
    More,
}

impl Parser {
    pub fn feed(&mut self, bytes: &[u8], out: &mut Vec<Input>) {
        self.buf.extend_from_slice(bytes);
        loop {
            if let Some(scanned) = self.paste {
                // The end may straddle the last feed and this one.
                let from = scanned.saturating_sub(PASTE_END.len() - 1);
                let found = self.buf[from..]
                    .windows(PASTE_END.len())
                    .position(|w| w == PASTE_END);
                let Some(i) = found else {
                    self.paste = Some(self.buf.len());
                    return;
                };
                let text = String::from_utf8_lossy(&self.buf[..from + i]).into_owned();
                out.push(Input::Paste(text));
                self.buf.drain(..from + i + PASTE_END.len());
                self.paste = None;
            }
            let mut at = 0;
            while at < self.buf.len() {
                match parse(&self.buf[at..], false, out) {
                    Step::Done(n) => at += n,
                    Step::Paste(n) => {
                        at += n;
                        self.paste = Some(0);
                        break;
                    }
                    Step::More => break,
                }
            }
            self.buf.drain(..at);
            if self.paste.is_none() {
                return;
            }
        }
    }

    /// Whether what is held could be a key or the start of a
    /// sequence, so that [`Parser::flush`] settles it if no more comes.
    /// Anything else unfinished, a character's first bytes or a
    /// sequence past `ESC [`, waits for its rest however late.
    pub fn pending(&self) -> bool {
        self.paste.is_none()
            && matches!(
                *self.buf.as_slice(),
                [ESC] | [ESC, ESC] | [ESC, b'[' | b'O']
            )
    }

    /// No more is coming: an Esc alone was the key, not the start of a
    /// sequence, and `ESC [` and `ESC O` were Alt+[ and Alt+O.
    pub fn flush(&mut self, out: &mut Vec<Input>) {
        if !self.pending() {
            return;
        }
        match *self.buf.as_slice() {
            [ESC, c @ (b'[' | b'O')] => out.push(key(KeyCode::Char(c.into()), Mods::ALT)),
            _ => out.push(key(KeyCode::Esc, Mods::NONE)),
        }
        self.buf.clear();
    }
}

fn key(code: KeyCode, mods: Mods) -> Input {
    Input::Key(Key::new(code, mods).normalized())
}

/// One input from the front of `b`, which is not empty. `alt` is set
/// after an ESC prefix, where another only ends the sequence.
fn parse(b: &[u8], alt: bool, out: &mut Vec<Input>) -> Step {
    match b[0] {
        ESC => escape(b, alt, out),
        // Ctrl+4 to Ctrl+7, as a legacy terminal sends them.
        c @ 0x1c..=0x1f => {
            out.push(key(KeyCode::Char((c - 0x1c + b'4').into()), Mods::CTRL));
            Step::Done(1)
        }
        c @ ..0x80 => {
            out.push(key(KeyCode::Char(c.into()), Mods::NONE));
            Step::Done(1)
        }
        c => {
            let len = match c {
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => return Step::Done(1),
            };
            let Some(bytes) = b.get(..len) else {
                return Step::More;
            };
            let Some(c) = str::from_utf8(bytes).ok().and_then(|s| s.chars().next()) else {
                return Step::Done(1);
            };
            out.push(key(KeyCode::Char(c), Mods::NONE));
            Step::Done(len)
        }
    }
}

fn escape(b: &[u8], alt: bool, out: &mut Vec<Input>) -> Step {
    match b.get(1) {
        None => Step::More,
        Some(b'[') => csi(b, out),
        Some(b'O') => {
            // Konsole and GNOME Terminal of old put a modifier
            // parameter here, as CSI has after `1;`.
            let params = b[2..]
                .iter()
                .take_while(|c| c.is_ascii_digit() || **c == b';')
                .count();
            let Some(&c) = b.get(2 + params) else {
                return Step::More;
            };
            let mods = str::from_utf8(&b[2..2 + params])
                .ok()
                .and_then(|p| pressed(p.rsplit(';').next().unwrap_or("")));
            let code = match c {
                // urxvt's Ctrl+arrows.
                b'a'..=b'd' if params == 0 => {
                    cursor_key(c.to_ascii_uppercase()).map(|code| (code, Mods::CTRL))
                }
                _ => cursor_key(c).zip(mods),
            };
            if let Some((code, mods)) = code {
                out.push(key(code, mods));
            }
            Step::Done(2 + params + 1)
        }
        Some(&ESC) if alt => Step::Done(1),
        // Alt is an ESC before the key's own bytes.
        Some(_) => {
            let from = out.len();
            let step = parse(&b[1..], true, out);
            for input in &mut out[from..] {
                if let Input::Key(k) = input {
                    *k = Key::new(k.code, k.mods | Mods::ALT).normalized();
                }
            }
            match step {
                Step::Done(n) => Step::Done(n + 1),
                Step::Paste(n) => Step::Paste(n + 1),
                Step::More => Step::More,
            }
        }
    }
}

fn cursor_key(c: u8) -> Option<KeyCode> {
    Some(match c {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        b'H' => KeyCode::Home,
        b'F' => KeyCode::End,
        _ => return None,
    })
}

/// `b` starts with `ESC [`.
fn csi(b: &[u8], out: &mut Vec<Input>) -> Step {
    let body = &b[2..];
    // The mouse as a terminal without SGR reports sends it: three
    // bytes that are not parameters.
    if body.first() == Some(&b'M') {
        let Some(&[cb, x, y]) = body.get(1..4).and_then(|s| s.first_chunk()) else {
            return Step::More;
        };
        let at = |c: u8| u32::from(c).saturating_sub(32);
        out.push(Input::Mouse(mouse(at(cb), at(x), at(y), false)));
        return Step::Done(6);
    }
    // The Linux console's F1 to F5: `[` and a letter, which is no key.
    if body.first() == Some(&b'[') {
        return if body.len() < 2 {
            Step::More
        } else {
            Step::Done(4)
        };
    }
    // urxvt ends a shifted key with `$`, elsewhere an intermediate
    // byte, and then only after a `;`.
    let urxvt = |i: usize, c: u8| c == b'$' && i > 0 && body[..i].iter().all(u8::is_ascii_digit);
    let end = body
        .iter()
        .enumerate()
        .position(|(i, &c)| !(0x20..=0x3f).contains(&c) || urxvt(i, c));
    // No bound: a report's text may run to many codepoints and many
    // reads.
    let Some(end) = end else {
        return Step::More;
    };
    let last = body[end];
    if last != b'$' && !(0x40..=0x7e).contains(&last) {
        // Cut short: whatever follows starts over.
        return Step::Done(2 + end);
    }
    let done = Step::Done(2 + end + 1);
    let Ok(params) = str::from_utf8(&body[..end]) else {
        return done;
    };
    let mut fields = params.split(';');
    let mut field = || fields.next().unwrap_or("");
    match last {
        b'M' | b'm' => {
            let sgr = params.strip_prefix('<');
            let mut n = sgr.unwrap_or(params).split(';').map(|n| n.parse().ok());
            if let (Some(Some(cb)), Some(Some(x)), Some(Some(y))) = (n.next(), n.next(), n.next()) {
                let cb: u32 = if sgr.is_some() {
                    cb
                } else {
                    cb.saturating_sub(32)
                };
                out.push(Input::Mouse(mouse(cb, x, y, last == b'm')));
            }
        }
        b'u' if params.starts_with('?') => {
            if let Ok(flags) = params[1..].parse() {
                out.push(Input::KeyboardFlags(flags));
            }
        }
        b'c' if params.starts_with('?') => out.push(Input::DeviceAttributes),
        b'u' => kitty(field(), field(), out),
        b'~' | b'$' | b'^' | b'@' => {
            let (code, mods) = (sub(field(), 0), pressed(field()));
            // urxvt's modifiers are the last byte.
            let held = match last {
                b'$' => Mods::SHIFT,
                b'^' => Mods::CTRL,
                b'@' => Mods::CTRL | Mods::SHIFT,
                _ => Mods::NONE,
            };
            let mods = mods.map(|m| m | held);
            let code = match code {
                Some(200) if last == b'~' => return Step::Paste(2 + end + 1),
                // modifyOtherKeys: the character is the third field.
                Some(27) if last == b'~' => {
                    sub(field(), 0).and_then(char::from_u32).map(KeyCode::Char)
                }
                Some(1 | 7) => Some(KeyCode::Home),
                Some(4 | 8) => Some(KeyCode::End),
                Some(5) => Some(KeyCode::PageUp),
                Some(6) => Some(KeyCode::PageDown),
                _ => None,
            };
            if let (Some(code), Some(mods)) = (code, mods) {
                out.push(key(code, mods));
            }
        }
        b'Z' => out.push(key(KeyCode::Tab, Mods::SHIFT)),
        // urxvt's Shift+arrows.
        b'a'..=b'd' if params.is_empty() => {
            if let Some(code) = cursor_key(last.to_ascii_uppercase()) {
                out.push(key(code, Mods::SHIFT));
            }
        }
        c => {
            // xterm's older modifyCursorKeys sends the modifiers alone:
            // `ESC [ 2 A` is Shift+Up.
            let (first, second) = (field(), field());
            let mods = if second.is_empty() { first } else { second };
            if let (Some(code), Some(mods)) = (cursor_key(c), pressed(mods)) {
                out.push(key(code, mods));
            }
        }
    }
    done
}

/// The `i`th of a field's colon-separated numbers.
fn sub(field: &str, i: usize) -> Option<u32> {
    field.split(':').nth(i)?.parse().ok()
}

/// The modifiers of a `modifiers:event-type` field; `None` for a
/// repeat or a release.
fn pressed(field: &str) -> Option<Mods> {
    if sub(field, 1).unwrap_or(1) != 1 {
        return None;
    }
    let bits = sub(field, 0).unwrap_or(1).saturating_sub(1);
    let mut mods = Mods::NONE;
    // Bit 32 is Meta. Hyper and the locks name no key here.
    for (bit, m) in [
        (1, Mods::SHIFT),
        (2, Mods::ALT),
        (4, Mods::CTRL),
        (8, Mods::CMD),
        (32, Mods::CMD),
    ] {
        if bits & bit != 0 {
            mods.insert(m);
        }
    }
    Some(mods)
}

/// A `CSI key ; modifiers:event u` report. The shifted key and the
/// text come only under flags vedi does not ask for.
fn kitty(code: &str, mods: &str, out: &mut Vec<Input>) {
    let (Some(code), Some(mods)) = (sub(code, 0), pressed(mods)) else {
        return;
    };
    let code = match code {
        9 => Some(KeyCode::Tab),
        13 | 57414 => Some(KeyCode::Enter),
        27 => Some(KeyCode::Esc),
        127 => Some(KeyCode::Backspace),
        // The keypad.
        57399..=57408 => char::from_digit(code - 57399, 10).map(KeyCode::Char),
        57409..=57413 => Some(KeyCode::Char(
            ['.', '/', '*', '-', '+'][(code - 57409) as usize],
        )),
        57415 => Some(KeyCode::Char('=')),
        57416 => Some(KeyCode::Char(',')),
        57417 => Some(KeyCode::Left),
        57418 => Some(KeyCode::Right),
        57419 => Some(KeyCode::Up),
        57420 => Some(KeyCode::Down),
        57421 => Some(KeyCode::PageUp),
        57422 => Some(KeyCode::PageDown),
        57423 => Some(KeyCode::Home),
        57424 => Some(KeyCode::End),
        // Keys with no name here, the modifiers themselves among
        // them, and 0, a report that is text alone.
        0 | 0xe000..=0xf8ff => None,
        c => char::from_u32(c).map(KeyCode::Char),
    };
    if let Some(code) = code {
        out.push(key(code, mods));
    }
}

/// Button 1 and the wheel, at a 1-based cell. Anything else, motion
/// with no button or another button, is button 1 not held, so no drag
/// outlives its button.
fn mouse(cb: u32, x: u32, y: u32, release: bool) -> Mouse {
    let kind = match (cb & 0xc3, cb & 0x20 != 0) {
        (0x40, _) => MouseKind::Wheel { rows: -1, cols: 0 },
        (0x41, _) => MouseKind::Wheel { rows: 1, cols: 0 },
        (0x42, _) => MouseKind::Wheel { rows: 0, cols: -1 },
        (0x43, _) => MouseKind::Wheel { rows: 0, cols: 1 },
        (0, false) if !release => MouseKind::Press,
        (0, true) if !release => MouseKind::Drag,
        _ => MouseKind::Release,
    };
    Mouse {
        kind,
        x: x.saturating_sub(1) as usize,
        y: y.saturating_sub(1) as usize,
        shift: cb & 4 != 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(bytes: &[u8]) -> Vec<Input> {
        let mut out = Vec::new();
        Parser::default().feed(bytes, &mut out);
        out
    }

    fn named(names: &[&str]) -> Vec<Input> {
        names
            .iter()
            .map(|n| Input::Key(n.parse().unwrap()))
            .collect()
    }

    /// Bytes, and the keys they are, by name.
    const KEYS: &[(&[u8], &[&str])] = &[
        // C0 controls and DEL.
        (b"\x00", &["Ctrl+Space"]),
        (b"\x7f", &["Backspace"]),
        (b"\x08", &["Backspace"]),
        (b"\x06", &["Ctrl+f"]),
        (b"\r", &["Enter"]),
        (b"\n", &["Ctrl+j"]),
        (b"\t", &["Tab"]),
        // Text.
        (b"G", &["G"]),
        (b"<", &["<"]),
        (b" ", &["Space"]),
        ("é".as_bytes(), &["é"]),
        ("こ".as_bytes(), &["こ"]),
        ("😀".as_bytes(), &["😀"]),
        (b"qj", &["q", "j"]),
        (b"\xffq", &["q"]),
        // Alt as an ESC prefix.
        (b"\x1bf", &["Alt+f"]),
        (b"\x1b<", &["Alt+<"]),
        (b"\x1b\x7f", &["Alt+Backspace"]),
        (b"\x1b\x1b[A", &["Alt+Up"]),
        (b"\x1b\x1bOA", &["Alt+Up"]),
        // SS3 and CSI cursor keys.
        (b"\x1b[A", &["Up"]),
        (b"\x1bOA", &["Up"]),
        (b"\x1bOB", &["Down"]),
        (b"\x1b[C", &["Right"]),
        (b"\x1b[D", &["Left"]),
        (b"\x1b[H", &["Home"]),
        (b"\x1bOF", &["End"]),
        (b"\x1b[1;2B", &["Shift+Down"]),
        (b"\x1b[1;3A", &["Alt+Up"]),
        (b"\x1b[1;6D", &["Ctrl+Shift+Left"]),
        (b"\x1b[2A", &["Shift+Up"]),
        (b"\x1b[5D", &["Ctrl+Left"]),
        (b"\x1b[1A", &["Up"]),
        (b"\x1b[1;9C", &["Cmd+Right"]),
        (b"\x1b[1;2:1B", &["Shift+Down"]),
        (b"\x1b[Z", &["Shift+Tab"]),
        // SS3 with a modifier parameter.
        (b"\x1bO5A", &["Ctrl+Up"]),
        (b"\x1bO2D", &["Shift+Left"]),
        (b"\x1bO3H", &["Alt+Home"]),
        (b"\x1bO6F", &["Ctrl+Shift+End"]),
        (b"\x1bO1;5C", &["Ctrl+Right"]),
        (b"\x1bO5Pq", &["q"]),
        (b"\x1bO2xq", &["q"]),
        // urxvt.
        (b"\x1b[a", &["Shift+Up"]),
        (b"\x1b[b", &["Shift+Down"]),
        (b"\x1b[c", &["Shift+Right"]),
        (b"\x1b[d", &["Shift+Left"]),
        (b"\x1bOa", &["Ctrl+Up"]),
        (b"\x1bOb", &["Ctrl+Down"]),
        (b"\x1bOc", &["Ctrl+Right"]),
        (b"\x1bOd", &["Ctrl+Left"]),
        (b"\x1b[5$", &["Shift+PgUp"]),
        (b"\x1b[6$q", &["Shift+PgDn", "q"]),
        (b"\x1b[7$", &["Shift+Home"]),
        (b"\x1b[8^", &["Ctrl+End"]),
        (b"\x1b[5^", &["Ctrl+PgUp"]),
        (b"\x1b[7@", &["Ctrl+Shift+Home"]),
        (b"\x1b[6@", &["Ctrl+Shift+PgDn"]),
        (b"\x1b[3$q", &["q"]),
        (b"\x1b[200$q", &["q"]),
        // `$` after a `;` is an intermediate byte, of a mode report.
        (b"\x1b[?2026;2$yq", &["q"]),
        (b"\x1b[4;1$yq", &["q"]),
        // The Linux console's F1 to F5.
        (b"\x1b[[A", &[]),
        (b"\x1b[[Eq", &["q"]),
        (b"\x1b[[Bj", &["j"]),
        // CSI ~ keys.
        (b"\x1b[1~", &["Home"]),
        (b"\x1b[7~", &["Home"]),
        (b"\x1b[4~", &["End"]),
        (b"\x1b[8~", &["End"]),
        (b"\x1b[5~", &["PgUp"]),
        (b"\x1b[6;2~", &["Shift+PgDn"]),
        (b"\x1b[27;5;102~", &["Ctrl+f"]),
        // CSI u.
        (b"\x1b[9u", &["Tab"]),
        (b"\x1b[9;2u", &["Shift+Tab"]),
        (b"\x1b[13u", &["Enter"]),
        (b"\x1b[27u", &["Esc"]),
        (b"\x1b[127u", &["Backspace"]),
        (b"\x1b[127;5u", &["Ctrl+Backspace"]),
        (b"\x1b[32;5u", &["Ctrl+Space"]),
        (b"\x1b[102;5u", &["Ctrl+f"]),
        (b"\x1b[102;6u", &["Ctrl+Shift+f"]),
        (b"\x1b[102;3u", &["Alt+f"]),
        (b"\x1b[99;9u", &["Cmd+c"]),
        (b"\x1b[103;10u", &["Cmd+Shift+g"]),
        (b"\x1b[99;33u", &["Cmd+c"]),
        (b"\x1b[44;4u", &["Alt+<"]),
        (b"\x1b[113;1:1u", &["q"]),
        (b"\x1b[57414u", &["Enter"]),
        (b"\x1b[57419;2u", &["Shift+Up"]),
        // Releases, repeats, modifier keys and keys without a name.
        (b"\x1b[113;1:3u", &[]),
        (b"\x1b[113;1:2u", &[]),
        (b"\x1b[1;1:3A", &[]),
        (b"\x1b[5;1:3~", &[]),
        (b"\x1b[57441u", &[]),
        (b"\x1b[57441;2u", &[]),
        (b"\x1b[57442;5u", &[]),
        (b"\x1b[57448;9u", &[]),
        (b"\x1b[57358u", &[]),
        (b"\x1bOP", &[]),
        (b"\x1b[P", &[]),
        (b"\x1b[11~", &[]),
        (b"\x1b[3~", &[]),
        (b"\x1b[I", &[]),
        (b"\x1b[201~", &[]),
        // A sequence cut short gives way to what follows.
        (b"\x1b[1;\x1b[A", &["Up"]),
        (b"\x1b[5\rq", &["Enter", "q"]),
    ];

    #[test]
    fn keys() {
        for &(bytes, names) in KEYS {
            assert_eq!(feed(bytes), named(names), "{}", bytes.escape_ascii());
        }
    }

    fn at(kind: MouseKind, shift: bool) -> Input {
        Input::Mouse(Mouse {
            kind,
            x: 3,
            y: 4,
            shift,
        })
    }

    fn other() -> Vec<(&'static [u8], Input)> {
        let wheel = |rows, cols| MouseKind::Wheel { rows, cols };
        vec![
            (b"\x1b[<4;4;5M", at(MouseKind::Press, true)),
            (b"\x1b[<0;4;5M", at(MouseKind::Press, false)),
            (b"\x1b[<0;4;5m", at(MouseKind::Release, false)),
            (b"\x1b[<32;4;5M", at(MouseKind::Drag, false)),
            (b"\x1b[<36;4;5M", at(MouseKind::Drag, true)),
            // Motion with no button.
            (b"\x1b[<35;4;5M", at(MouseKind::Release, false)),
            (b"\x1b[<64;4;5M", at(wheel(-1, 0), false)),
            (b"\x1b[<69;4;5M", at(wheel(1, 0), true)),
            (b"\x1b[<66;4;5M", at(wheel(0, -1), false)),
            (b"\x1b[<67;4;5M", at(wheel(0, 1), false)),
            // The right and middle buttons.
            (b"\x1b[<2;4;5M", at(MouseKind::Release, false)),
            (b"\x1b[<33;4;5M", at(MouseKind::Release, false)),
            (b"\x1b[<2;4;5m", at(MouseKind::Release, false)),
            // Without SGR reports.
            (b"\x1b[M $%", at(MouseKind::Press, false)),
            (b"\x1b[M#$%", at(MouseKind::Release, false)),
            (b"\x1b[32;4;5M", at(MouseKind::Press, false)),
            (
                b"\x1b[200~a\x1b[Ab\n\x1b[201~",
                Input::Paste("a\x1b[Ab\n".into()),
            ),
            (b"\x1b[200~\x1b[201~", Input::Paste(String::new())),
            (b"\x1b[?0u", Input::KeyboardFlags(0)),
            (b"\x1b[?25u", Input::KeyboardFlags(25)),
            (b"\x1b[?62;4c", Input::DeviceAttributes),
            (b"\x1b[?1;2c", Input::DeviceAttributes),
        ]
    }

    #[test]
    fn mouse_pastes_and_replies() {
        for (bytes, want) in other() {
            assert_eq!(feed(bytes), [want], "{}", bytes.escape_ascii());
        }
    }

    /// Every case, cut at every byte, is what it is whole.
    #[test]
    fn split_across_feeds() {
        let keys = KEYS.iter().map(|&(b, names)| (b, named(names)));
        let other = other().into_iter().map(|(b, want)| (b, vec![want]));
        for (bytes, want) in keys.chain(other) {
            for cut in 1..bytes.len() {
                let (mut p, mut out) = (Parser::default(), Vec::new());
                p.feed(&bytes[..cut], &mut out);
                p.feed(&bytes[cut..], &mut out);
                assert_eq!(out, want, "{} cut at {cut}", bytes.escape_ascii());
                assert!(!p.pending());
            }
            let (mut p, mut out) = (Parser::default(), Vec::new());
            for b in bytes {
                p.feed(&[*b], &mut out);
            }
            assert_eq!(out, want, "{} by the byte", bytes.escape_ascii());
        }
    }

    #[test]
    fn keys_around_a_paste() {
        assert_eq!(
            feed(b"q\x1b[200~x\x1b[201~\x1b[A"),
            [
                named(&["q"]),
                vec![Input::Paste("x".into())],
                named(&["Up"])
            ]
            .concat()
        );
    }

    /// What is unfinished when no more comes.
    #[test]
    fn flush() {
        let tests: [(&[u8], &[&str]); 6] = [
            (b"\x1b", &["Esc"]),
            (b"\x1b\x1b", &["Esc"]),
            (b"\x1b[", &["Alt+["]),
            (b"\x1bO", &["Alt+O"]),
            (b"q\x1b", &["q", "Esc"]),
            (b"", &[]),
        ];
        for (bytes, names) in tests {
            let (mut p, mut out) = (Parser::default(), Vec::new());
            p.feed(bytes, &mut out);
            assert_eq!(p.pending(), !bytes.is_empty());
            p.flush(&mut out);
            assert_eq!(out, named(names), "{}", bytes.escape_ascii());
            assert!(!p.pending());
            p.feed(b"j", &mut out);
            assert_eq!(out.last(), named(&["j"]).last());
        }
    }

    /// A character's first bytes and a sequence past `ESC [` are no
    /// key yet, and wait out a flush for their rest.
    #[test]
    fn late_rest_is_kept() {
        let tests: [(&[u8], &[u8], &str); 3] = [
            (b"\xc3", b"\xa9", "\u{e9}"),
            (b"\x1b[1;", b"2A", "Shift+Up"),
            (b"\x1b[2", b"7;5;106~", "Ctrl+j"),
        ];
        for (head, rest, name) in tests {
            let (mut p, mut out) = (Parser::default(), Vec::new());
            p.feed(head, &mut out);
            assert!(!p.pending(), "{}", head.escape_ascii());
            p.flush(&mut out);
            p.feed(rest, &mut out);
            assert_eq!(out, named(&[name]), "{}", head.escape_ascii());
        }
    }

    /// A paste is never given up on, and its end is found across feeds
    /// without the text being searched again.
    #[test]
    fn paste_waits_for_its_end() {
        let (mut p, mut out) = (Parser::default(), Vec::new());
        p.feed(b"\x1b[200~abc\x1b[20", &mut out);
        assert!(!p.pending());
        p.flush(&mut out);
        p.feed(b"1", &mut out);
        assert_eq!(out, []);
        p.feed(b"~q", &mut out);
        assert_eq!(
            out,
            [vec![Input::Paste("abc".into())], named(&["q"])].concat()
        );
    }

    /// A sequence the terminal never ends waits, and the next byte
    /// that can end one ends it; keys after it are keys.
    #[test]
    fn a_sequence_without_end_is_dropped() {
        let (mut p, mut out) = (Parser::default(), Vec::new());
        p.feed(b"\x1b[", &mut out);
        p.feed(&[b'1'; 4096], &mut out);
        assert!(!p.pending());
        p.feed(b"qj", &mut out);
        assert_eq!(out, named(&["j"]));
    }
}
