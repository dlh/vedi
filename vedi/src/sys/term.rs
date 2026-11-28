//! The terminal: frames out, keys, the mouse and resizes in.

use std::fs::File;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::input::{Input, Parser};
use super::tty;
use crate::app::Event;
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

const ALT_SCREEN_ON: &[u8] = b"\x1b[?1049h";
const ALT_SCREEN_OFF: &[u8] = b"\x1b[2J\x1b[?25h\x1b[?1049l";
// Presses, releases and motion with a button down, as SGR reports.
// Not every motion (1003), a report per move that nothing here reads.
const MOUSE_ON: &[u8] = b"\x1b[?1000h\x1b[?1002h\x1b[?1006h";
const MOUSE_OFF: &[u8] = b"\x1b[?1006l\x1b[?1002l\x1b[?1000l";
// XTSHIFTESCAPE: Shift+click to the pager, which the terminal
// otherwise keeps for its own selection.
const SHIFT_ESCAPE_ON: &[u8] = b"\x1b[>1s";
const SHIFT_ESCAPE_OFF: &[u8] = b"\x1b[>0s";

const PASTE_ON: &[u8] = b"\x1b[?2004h";
const PASTE_OFF: &[u8] = b"\x1b[?2004l";
/// The kitty keyboard flags: disambiguate, so Esc and the modified
/// keys are told apart, every key as an escape code, its text, and
/// the key Shift makes of it on the terminal's layout.
const KEYS: u32 = 1 | 4 | 8 | 16;
const KEYS_QUERY: &[u8] = b"\x1b[?u\x1b[c";
const KEYS_POP: &[u8] = b"\x1b[<1u";

/// How long a terminal has to answer a query. One that answers none
/// costs this at startup.
const REPLY_WAIT: Duration = Duration::from_secs(2);
/// How long after an Esc the rest of a sequence may take to come,
/// over a slow line; past it the Esc was the key.
const ESC_WAIT: Duration = Duration::from_millis(50);

/// /dev/tty, open for `Term::enter`: stdout may be a pipe.
pub struct Device {
    out: File,
    input: File,
    sized: File,
}

/// Opens the terminal. Called before the input files are opened: the
/// wait for input can only watch a low descriptor.
pub fn open() -> io::Result<Device> {
    let out = File::options()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|e| io::Error::new(e.kind(), format!("open /dev/tty: {e}")))?;
    Ok(Device {
        input: out.try_clone()?,
        sized: out.try_clone()?,
        out,
    })
}

/// The terminal on /dev/tty.
/// Raw, on the alternate screen, with the mouse; Drop puts it back.
pub struct Term {
    tty: File,
    out: Vec<u8>,
    renderer: Renderer,
    raw_tx: Sender<Vec<u8>>,
    raw: Receiver<Vec<u8>>,
}

impl Term {
    /// Takes the terminal over.
    pub fn enter(device: Device) -> io::Result<(Term, Events)> {
        let tty = device.out;
        let mut events = Events {
            tty: device.input,
            sized: device.sized,
            parser: Parser::default(),
            early: Vec::new(),
        };
        tty::raw(&tty)?;
        set_size(tty::size(&tty));
        ENTERED.store(true, Ordering::SeqCst);
        let (raw_tx, raw) = mpsc::channel();
        let mut t = Term {
            tty,
            out: Vec::new(),
            renderer: Renderer::new(),
            raw_tx,
            raw,
        };
        t.out.extend_from_slice(ALT_SCREEN_ON);
        t.out.extend_from_slice(MOUSE_ON);
        // A paste read as keys would run any : line its author put
        // in it.
        t.out.extend_from_slice(PASTE_ON);
        // Asked and pushed on the alternate screen, which has flags of
        // its own.
        t.out.extend_from_slice(KEYS_QUERY);
        t.flush()?;
        if events.keyboard_flags()?.is_some() {
            write!(t.out, "\x1b[>{KEYS}u")?;
            PUSHED.store(true, Ordering::SeqCst);
            t.out.extend_from_slice(KEYS_QUERY);
            t.flush()?;
            // A terminal that sends every key as an escape code but
            // not its text leaves a shifted or composed character
            // unknown, and one without the shifted key leaves it a
            // guess. With the one flag text comes bare, which every
            // such terminal gets right.
            if events.keyboard_flags()?.is_none_or(|f| f & KEYS != KEYS) {
                t.out.extend_from_slice(b"\x1b[=1u");
            }
        }
        t.out.extend_from_slice(SHIFT_ESCAPE_ON);
        t.flush()?;
        Ok((t, events))
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

impl Terminal for Term {
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
            Some((x, y)) => write!(self.out, "\x1b[{};{}H\x1b[?25h", y + 1, x + 1),
            None => self.out.write_all(b"\x1b[?25l"),
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

impl Drop for Term {
    fn drop(&mut self) {
        self.take_raw();
        let mut out = std::mem::take(&mut self.out);
        self.renderer.reset(&mut out);
        restore(&mut self.tty, out);
    }
}

/// Puts the terminal back after a panic, before the panic is printed,
/// or on a signal that ends vedi. Does nothing unless the terminal is up, and Drop then does nothing.
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
        out.extend_from_slice(KEYS_POP);
    }
    out.extend_from_slice(PASTE_OFF);
    out.extend_from_slice(MOUSE_OFF);
    out.extend_from_slice(ALT_SCREEN_OFF);
    let _ = tty.write_all(&out);
    tty::cooked(tty);
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

/// The terminal's input, to read once the app is there to hear it.
pub struct Events {
    tty: File,
    /// For the resize thread to size.
    sized: File,
    parser: Parser,
    /// What was typed ahead of a query's reply.
    early: Vec<Input>,
}

impl Events {
    /// Reads until DA1's reply, which every terminal sends: the
    /// keyboard flags if their reply came before it, the sign of the
    /// kitty keyboard protocol.
    fn keyboard_flags(&mut self) -> io::Result<Option<u32>> {
        let deadline = Instant::now() + REPLY_WAIT;
        let mut flags = None;
        let mut inputs = Vec::new();
        let mut buf = [0; 1024];
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if !tty::readable_within(&self.tty, left) {
                return Ok(flags);
            }
            let n = match self.tty.read(&mut buf) {
                Ok(0) => return Ok(flags),
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
            self.parser.feed(&buf[..n], &mut inputs);
            let mut done = false;
            for input in inputs.drain(..) {
                match input {
                    Input::KeyboardFlags(f) => flags = Some(f),
                    Input::DeviceAttributes => done = true,
                    input => self.early.push(input),
                }
            }
            if done {
                return Ok(flags);
            }
        }
    }

    /// Reads the terminal's input on a thread, sending keys, pastes,
    /// the mouse and resizes until `tx` closes or the terminal fails.
    pub fn spawn(mut self, tx: Sender<Event>) -> JoinHandle<()> {
        thread::Builder::new()
            .name("vedi-events".into())
            .spawn(move || {
                let resize_tx = tx.clone();
                let resized = move |w, h| {
                    set_size((w, h));
                    resize_tx.send(Event::Resize(w, h)).is_ok()
                };
                let was = SIZE.load(Ordering::SeqCst);
                // A resize since `enter` sized the terminal came before
                // anything listened for one.
                let _resize = tty::sigwinch(self.sized, resized.clone());
                let now = tty::size(&self.tty);
                set_size(now);
                if SIZE.load(Ordering::SeqCst) != was && !resized(now.0, now.1) {
                    return;
                }
                let mut inputs = std::mem::take(&mut self.early);
                let mut buf = [0; 1024];
                loop {
                    for input in inputs.drain(..) {
                        let ev = match input {
                            Input::Key(k) => Event::Key(k),
                            Input::KeyText(k, text) => Event::KeyText(k, text),
                            Input::Mouse(m) => Event::Mouse(m),
                            Input::Paste(s) => Event::Paste(s),
                            Input::KeyboardFlags(_) | Input::DeviceAttributes => continue,
                        };
                        if tx.send(ev).is_err() {
                            return;
                        }
                    }
                    if self.parser.pending() && !tty::readable_within(&self.tty, ESC_WAIT) {
                        self.parser.flush(&mut inputs);
                        continue;
                    }
                    match self.tty.read(&mut buf) {
                        Ok(0) => return,
                        Ok(n) => self.parser.feed(&buf[..n], &mut inputs),
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                        Err(_) => return,
                    }
                }
            })
            .expect("spawn event thread")
    }
}
