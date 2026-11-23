//! The terminal's size, and word of its changes.
#![allow(unsafe_code)]

use std::fs::File;
use std::io::{self, Write};
use std::os::fd::{AsFd, AsRawFd};
use std::sync::mpsc::Sender;
use std::thread;

use signal_hook::consts::SIGWINCH;
use signal_hook::iterator::{Handle, Signals};

use crate::app::Event;

/// The size, columns by rows, of the terminal open on `tty`, falling
/// back to `COLUMNS` and `LINES`, then the classic 80×25. A pager's
/// stdin is the pipe and its stdout may be too, so neither is asked.
pub fn size(tty: &File) -> (usize, usize) {
    // SAFETY: winsize is plain data, zeroed is valid.
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    // SAFETY: TIOCGWINSZ writes a winsize through the pointer, which is
    // to a live one.
    let ok = unsafe { libc::ioctl(tty.as_raw_fd(), libc::TIOCGWINSZ, &mut ws) } == 0;
    let got = if ok {
        (usize::from(ws.ws_col), usize::from(ws.ws_row))
    } else {
        (0, 0)
    };
    fallback(
        got,
        std::env::var("COLUMNS").ok(),
        std::env::var("LINES").ok(),
    )
}

/// Runs `f` with stdout on `tty`, then puts stdout back.
pub fn with_stdout_on<R>(tty: &File, f: impl FnOnce() -> io::Result<R>) -> io::Result<R> {
    io::stdout().flush()?;
    let saved = io::stdout().as_fd().try_clone_to_owned()?;
    // SAFETY: dup2 takes two open descriptors and touches no memory.
    if unsafe { libc::dup2(tty.as_raw_fd(), libc::STDOUT_FILENO) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let r = f();
    let _ = io::stdout().flush();
    // SAFETY: as above; `saved` is open until it drops after this.
    unsafe { libc::dup2(saved.as_raw_fd(), libc::STDOUT_FILENO) };
    r
}

fn fallback(got: (usize, usize), cols: Option<String>, lines: Option<String>) -> (usize, usize) {
    let or = |n: usize, env: Option<String>, dflt: usize| match n {
        0 => env
            .and_then(|s| s.parse().ok())
            .filter(|&n| n > 0)
            .unwrap_or(dflt),
        n => n,
    };
    (or(got.0, cols, 80), or(got.1, lines, 25))
}

/// An `Event::Resize` on `tx` per SIGWINCH, sized from `tty`, until
/// the guard drops.
pub fn sigwinch(tx: Sender<Event>, tty: File) -> Sigwinch {
    let Ok(mut signals) = Signals::new([SIGWINCH]) else {
        // No resize is ever reported.
        return Sigwinch(None);
    };
    let handle = signals.handle();
    thread::Builder::new()
        .name("vedi-sigwinch".into())
        .spawn(move || {
            for _ in signals.forever() {
                let (w, h) = size(&tty);
                if tx.send(Event::Resize(w, h)).is_err() {
                    return;
                }
            }
        })
        .expect("spawn sigwinch thread");
    Sigwinch(Some(handle))
}

/// Dropping it ends the thread.
pub struct Sigwinch(Option<Handle>);

impl Drop for Sigwinch {
    fn drop(&mut self) {
        if let Some(handle) = &self.0 {
            handle.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    #[test]
    fn size_falls_back() {
        let s = |v: &str| Some(v.to_string());
        assert_eq!(fallback((100, 40), s("90"), s("30")), (100, 40));
        assert_eq!(fallback((0, 0), s("90"), s("30")), (90, 30));
        assert_eq!(fallback((0, 40), s("90"), s("x")), (90, 40));
        assert_eq!(fallback((0, 0), s("x"), None), (80, 25));
        assert_eq!(fallback((0, 0), s("0"), s("-3")), (80, 25));
    }

    // Not a terminal: no ioctl answer, so whatever the environment
    // gives, never zero.
    #[test]
    fn size_of_a_non_terminal() {
        let (w, h) = size(&File::open("/dev/null").unwrap());
        assert!(w > 0 && h > 0);
    }

    #[test]
    fn sigwinch_forwards() {
        let (tx, rx) = mpsc::channel();
        let _guard = sigwinch(tx, File::open("/dev/null").unwrap());
        // SAFETY: raise sends a signal to this thread; the handler is
        // installed.
        assert_eq!(unsafe { libc::raise(libc::SIGWINCH) }, 0);
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(2)),
            Ok(Event::Resize(w, h)) if w > 0 && h > 0
        ));
    }
}
