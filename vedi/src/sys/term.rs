//! The terminal, through crossterm: frames out, keys, the mouse and
//! resizes in.

use std::fs::File;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{
    self as ct, DisableBracketedPaste, EnableBracketedPaste, KeyEventKind, KeyModifiers,
    KeyboardEnhancementFlags, MouseButton, MouseEventKind, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::queue;
use crossterm::terminal::{
    self as ctt, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen,
};

use super::tty;
use crate::app::{Event, Mouse, MouseKind};
use crate::input::key::{Key, KeyCode, Mods};
use crate::view::render::Renderer;
use crate::view::screen::{Cell, Terminal};

/// The terminal is up, and `restore` has it to put back.
static ENTERED: AtomicBool = AtomicBool::new(false);
/// The keyboard flags were pushed, and are to be popped.
static PUSHED: AtomicBool = AtomicBool::new(false);
/// Columns and rows, high and low halves: the event thread sets it on a
/// resize before the app hears of it, so a frame need not ask the tty.
static SIZE: AtomicU64 = AtomicU64::new(0);

fn set_size((w, h): (usize, usize)) {
    SIZE.store((w as u64) << 32 | h as u64 & 0xffff_ffff, Ordering::SeqCst);
}

// Presses, releases and motion with a button down. crossterm's
// EnableMouseCapture also asks for every motion (1003), a report per
// move that nothing here reads.
const MOUSE_ON: &[u8] = b"\x1b[?1000h\x1b[?1002h\x1b[?1006h";
const MOUSE_OFF: &[u8] = b"\x1b[?1006l\x1b[?1002l\x1b[?1000l";
// XTSHIFTESCAPE: Shift+click to the pager, which the terminal
// otherwise keeps for its own selection.
const SHIFT_ESCAPE_ON: &[u8] = b"\x1b[>1s";
const SHIFT_ESCAPE_OFF: &[u8] = b"\x1b[>0s";

/// The terminal on /dev/tty: stdout may be a pipe.
/// Raw, on the alternate screen, with the mouse; Drop puts it back.
pub struct Crossterm {
    tty: File,
    out: Vec<u8>,
    renderer: Renderer,
    raw_tx: Sender<Vec<u8>>,
    raw: Receiver<Vec<u8>>,
}

impl Crossterm {
    /// Takes the terminal over. Call before `spawn_events`: the
    /// keyboard protocol query reads the answer as an event.
    pub fn enter() -> io::Result<Crossterm> {
        let tty = File::options()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .map_err(|e| io::Error::new(e.kind(), format!("open /dev/tty: {e}")))?;
        ctt::enable_raw_mode()?;
        set_size(tty::size(&tty));
        ENTERED.store(true, Ordering::SeqCst);
        let (raw_tx, raw) = mpsc::channel();
        let mut t = Crossterm {
            tty,
            out: Vec::new(),
            renderer: Renderer::new(),
            raw_tx,
            raw,
        };
        // crossterm writes its keyboard query to stdout, maybe a pipe,
        // when it cannot write /dev/tty, which it opens read-only.
        let kitty =
            tty::with_stdout_on(&t.tty, ctt::supports_keyboard_enhancement).unwrap_or(false);
        queue!(t.out, EnterAlternateScreen)?;
        t.out.extend_from_slice(MOUSE_ON);
        // A paste read as keys would run any : line its author put
        // in it.
        queue!(t.out, EnableBracketedPaste)?;
        if kitty {
            queue!(
                t.out,
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
            )?;
            PUSHED.store(true, Ordering::SeqCst);
        }
        t.out.extend_from_slice(SHIFT_ESCAPE_ON);
        t.flush()?;
        Ok(t)
    }

    /// A writer for the clipboard's OSC 52. Its bytes go out with the
    /// next flush, after the frame, so none lands inside one.
    pub fn raw_writer(&self) -> RawWriter {
        RawWriter(self.raw_tx.clone())
    }

    fn take_raw(&mut self) {
        while let Ok(b) = self.raw.try_recv() {
            self.out.extend_from_slice(&b);
        }
    }
}

impl Terminal for Crossterm {
    fn size(&self) -> (usize, usize) {
        let s = SIZE.load(Ordering::SeqCst);
        ((s >> 32) as usize, (s & 0xffff_ffff) as usize)
    }

    fn draw(&mut self, cells: &mut dyn Iterator<Item = (usize, usize, &Cell)>) {
        let (w, h) = self.size();
        for (x, y, c) in cells {
            if x < w && y < h {
                self.renderer.cell(&mut self.out, x, y, c);
            }
        }
    }

    fn set_cursor(&mut self, at: Option<(usize, usize)>) {
        // Past what the protocol can address, the cursor is hidden as
        // off screen.
        let at = at.and_then(|(x, y)| Some((u16::try_from(x).ok()?, u16::try_from(y).ok()?)));
        let _ = match at {
            Some((x, y)) => queue!(self.out, MoveTo(x, y), Show),
            None => queue!(self.out, Hide),
        };
        self.renderer.cursor_moved();
    }

    fn flush(&mut self) -> io::Result<()> {
        self.take_raw();
        let r = self.tty.write_all(&self.out);
        self.out.clear();
        r
    }

    fn write_raw(&mut self, bytes: &[u8]) {
        self.out.extend_from_slice(bytes);
    }
}

impl Drop for Crossterm {
    fn drop(&mut self) {
        self.take_raw();
        let mut out = std::mem::take(&mut self.out);
        self.renderer.reset(&mut out);
        restore(&mut self.tty, out);
    }
}

/// Puts the terminal back after a panic, before the panic is printed.
/// Does nothing unless the terminal is up, and Drop then does nothing.
pub fn restore_after_panic() {
    if let Ok(mut tty) = File::options().write(true).open("/dev/tty") {
        // The renderer's state is out of reach: close a link and reset
        // the style whatever it was.
        restore(&mut tty, b"\x1b]8;;\x1b\\\x1b[0m".to_vec());
    }
}

/// Writes `out`, then undoes `enter` in reverse, once.
fn restore(tty: &mut File, mut out: Vec<u8>) {
    if !ENTERED.swap(false, Ordering::SeqCst) {
        return;
    }
    out.extend_from_slice(SHIFT_ESCAPE_OFF);
    if PUSHED.swap(false, Ordering::SeqCst) {
        let _ = queue!(out, PopKeyboardEnhancementFlags);
    }
    let _ = queue!(out, DisableBracketedPaste);
    out.extend_from_slice(MOUSE_OFF);
    let _ = queue!(out, Clear(ClearType::All), Show, LeaveAlternateScreen);
    let _ = tty.write_all(&out);
    let _ = ctt::disable_raw_mode();
}

/// Bytes for the terminal, sent with its next flush.
pub struct RawWriter(Sender<Vec<u8>>);

impl Write for RawWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .send(buf.to_vec())
            .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Reads the terminal's input on a thread, sending keys, pastes, the
/// mouse and resizes until `tx` closes or the terminal fails.
pub fn spawn_events(tx: Sender<Event>) -> JoinHandle<()> {
    thread::Builder::new()
        .name("vedi-events".into())
        .spawn(move || {
            while let Ok(ev) = ct::read() {
                let ev = match ev {
                    ct::Event::Key(k) => key(k).map(Event::Key),
                    ct::Event::Mouse(m) => Some(Event::Mouse(mouse(m))),
                    ct::Event::Paste(s) => Some(Event::Paste(s)),
                    ct::Event::Resize(w, h) => {
                        let (w, h) = (w.into(), h.into());
                        set_size((w, h));
                        Some(Event::Resize(w, h))
                    }
                    _ => None,
                };
                if let Some(ev) = ev
                    && tx.send(ev).is_err()
                {
                    return;
                }
            }
        })
        .expect("spawn event thread")
}

/// The key pressed; `None` for a release or repeat, or a key the key
/// map has no name for.
fn key(k: ct::KeyEvent) -> Option<Key> {
    if k.kind != KeyEventKind::Press {
        return None;
    }
    let mut mods = Mods::NONE;
    for (m, to) in [
        (KeyModifiers::SHIFT, Mods::SHIFT),
        (KeyModifiers::CONTROL, Mods::CTRL),
        (KeyModifiers::ALT, Mods::ALT),
        (KeyModifiers::SUPER, Mods::CMD),
        (KeyModifiers::META, Mods::CMD),
    ] {
        if k.modifiers.contains(m) {
            mods.insert(to);
        }
    }
    let code = match k.code {
        ct::KeyCode::Char(c) => KeyCode::Char(c),
        ct::KeyCode::Null => KeyCode::Char('\0'),
        ct::KeyCode::Up => KeyCode::Up,
        ct::KeyCode::Down => KeyCode::Down,
        ct::KeyCode::Left => KeyCode::Left,
        ct::KeyCode::Right => KeyCode::Right,
        ct::KeyCode::Home => KeyCode::Home,
        ct::KeyCode::End => KeyCode::End,
        ct::KeyCode::PageUp => KeyCode::PageUp,
        ct::KeyCode::PageDown => KeyCode::PageDown,
        ct::KeyCode::Enter => KeyCode::Enter,
        ct::KeyCode::Esc => KeyCode::Esc,
        ct::KeyCode::Tab => KeyCode::Tab,
        ct::KeyCode::BackTab => {
            mods.insert(Mods::SHIFT);
            KeyCode::Tab
        }
        ct::KeyCode::Backspace => KeyCode::Backspace,
        _ => return None,
    };
    Some(Key::new(code, mods).normalized())
}

/// Button 1 and the wheel. Anything else, motion with no button or
/// another button, is button 1 not held, so no drag outlives its button.
fn mouse(m: ct::MouseEvent) -> Mouse {
    let kind = match m.kind {
        MouseEventKind::Down(MouseButton::Left) => MouseKind::Press,
        MouseEventKind::Drag(MouseButton::Left) => MouseKind::Drag,
        MouseEventKind::Up(_)
        | MouseEventKind::Down(_)
        | MouseEventKind::Drag(_)
        | MouseEventKind::Moved => MouseKind::Release,
        MouseEventKind::ScrollUp => MouseKind::Wheel { rows: -1, cols: 0 },
        MouseEventKind::ScrollDown => MouseKind::Wheel { rows: 1, cols: 0 },
        MouseEventKind::ScrollLeft => MouseKind::Wheel { rows: 0, cols: -1 },
        MouseEventKind::ScrollRight => MouseKind::Wheel { rows: 0, cols: 1 },
    };
    Mouse {
        kind,
        x: m.column.into(),
        y: m.row.into(),
        shift: m.modifiers.contains(KeyModifiers::SHIFT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: ct::KeyCode, mods: KeyModifiers) -> ct::KeyEvent {
        ct::KeyEvent::new(code, mods)
    }

    // Each byte sequence as crossterm's parser reports it, then as the
    // key map names it.
    #[test]
    fn terminal_keys() {
        use ct::KeyCode as C;
        let none = KeyModifiers::NONE;
        let ctrl = KeyModifiers::CONTROL;
        let shift = KeyModifiers::SHIFT;
        for (raw, ev, name) in [
            ("\\x00", press(C::Char(' '), ctrl), "Ctrl+Space"),
            ("\\x7f", press(C::Backspace, none), "Backspace"),
            ("\\x08", press(C::Char('h'), ctrl), "Backspace"),
            ("\\x06", press(C::Char('f'), ctrl), "Ctrl+f"),
            ("\\r", press(C::Enter, none), "Enter"),
            ("G", press(C::Char('G'), shift), "G"),
            ("<", press(C::Char('<'), none), "<"),
            (" ", press(C::Char(' '), none), "Space"),
            ("é", press(C::Char('é'), none), "é"),
            ("\\x1b[A", press(C::Up, none), "Up"),
            ("\\x1b[1;2B", press(C::Down, shift), "Shift+Down"),
            (
                "\\x1b[1;6D",
                press(C::Left, ctrl | shift),
                "Ctrl+Shift+Left",
            ),
            ("\\x1b[5~", press(C::PageUp, none), "PgUp"),
            ("\\x1b[Z", press(C::BackTab, shift), "Shift+Tab"),
            ("\\x1b[9u", press(C::Tab, none), "Tab"),
            (
                "kitty ⌘c",
                press(C::Char('c'), KeyModifiers::SUPER),
                "Cmd+c",
            ),
            (
                "kitty ⌘⇧G",
                press(C::Char('G'), KeyModifiers::SUPER | shift),
                "Cmd+Shift+g",
            ),
            ("meta", press(C::Char('c'), KeyModifiers::META), "Cmd+c"),
            (
                "kitty ⌥<",
                press(C::Char(','), KeyModifiers::ALT | shift),
                "Alt+<",
            ),
        ] {
            let want: Key = name.parse().unwrap();
            assert_eq!(key(ev), Some(want), "{raw}");
        }
    }

    #[test]
    fn only_presses_of_named_keys() {
        let mut ev = press(ct::KeyCode::Char('q'), KeyModifiers::NONE);
        ev.kind = KeyEventKind::Release;
        assert_eq!(key(ev), None);
        ev.kind = KeyEventKind::Repeat;
        assert_eq!(key(ev), None);
        assert_eq!(key(press(ct::KeyCode::F(1), KeyModifiers::NONE)), None);
        assert_eq!(key(press(ct::KeyCode::Delete, KeyModifiers::NONE)), None);
    }

    #[test]
    fn mouse_events() {
        let ev = |kind, mods| ct::MouseEvent {
            kind,
            column: 3,
            row: 4,
            modifiers: mods,
        };
        let at = |kind, shift| Mouse {
            kind,
            x: 3,
            y: 4,
            shift,
        };
        let none = KeyModifiers::NONE;
        let shift = KeyModifiers::SHIFT;
        let left = MouseButton::Left;
        for (got, want) in [
            (
                ev(MouseEventKind::Down(left), shift),
                at(MouseKind::Press, true),
            ),
            (
                ev(MouseEventKind::Up(left), none),
                at(MouseKind::Release, false),
            ),
            (
                ev(MouseEventKind::Drag(left), none),
                at(MouseKind::Drag, false),
            ),
            (
                ev(MouseEventKind::Moved, none),
                at(MouseKind::Release, false),
            ),
            (
                ev(MouseEventKind::ScrollUp, none),
                at(MouseKind::Wheel { rows: -1, cols: 0 }, false),
            ),
            (
                ev(MouseEventKind::ScrollDown, shift),
                at(MouseKind::Wheel { rows: 1, cols: 0 }, true),
            ),
            (
                ev(MouseEventKind::ScrollLeft, none),
                at(MouseKind::Wheel { rows: 0, cols: -1 }, false),
            ),
            (
                ev(MouseEventKind::ScrollRight, none),
                at(MouseKind::Wheel { rows: 0, cols: 1 }, false),
            ),
            (
                ev(MouseEventKind::Down(MouseButton::Right), none),
                at(MouseKind::Release, false),
            ),
            (
                ev(MouseEventKind::Drag(MouseButton::Middle), none),
                at(MouseKind::Release, false),
            ),
            (
                ev(MouseEventKind::Up(MouseButton::Right), none),
                at(MouseKind::Release, false),
            ),
        ] {
            assert_eq!(mouse(got), want, "{got:?}");
        }
    }
}
