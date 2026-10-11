//! The terminal's size, and word of its changes.
#![allow(unsafe_code)]

use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc::Sender;
use std::thread;

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
        (ws.ws_col as usize, ws.ws_row as usize)
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

/// The write end of the self-pipe, for the handler.
static PIPE: AtomicI32 = AtomicI32::new(-1);

extern "C" fn on_sigwinch(_: libc::c_int) {
    let fd = PIPE.load(Ordering::Relaxed);
    let b = 0u8;
    // The interrupted code may be about to read errno.
    let errno = errno();
    // SAFETY: errno is thread-local; the pointer is live for the
    // thread. write is async-signal-safe and reads one byte from a
    // live local; a full pipe already holds word of a resize.
    unsafe {
        let saved = *errno;
        libc::write(fd, (&raw const b).cast(), 1);
        *errno = saved;
    }
}

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
fn errno() -> *mut libc::c_int {
    // SAFETY: returns the calling thread's errno; no arguments.
    unsafe { libc::__error() }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn errno() -> *mut libc::c_int {
    // SAFETY: returns the calling thread's errno; no arguments.
    unsafe { libc::__errno_location() }
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "linux",
    target_os = "android"
)))]
compile_error!("errno access is not implemented for this target");

/// An `Event::Resize` on `tx` per SIGWINCH, sized from `tty`, until
/// the guard drops. Called once: a second call takes the signal from
/// the first.
pub fn sigwinch(tx: Sender<Event>, tty: File) -> Sigwinch {
    let mut fds = [0; 2];
    // SAFETY: pipe writes two descriptors into the array.
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        // No resize is ever reported.
        return Sigwinch(None);
    }
    for fd in fds {
        // SAFETY: fcntl on a descriptor just made; no pointers.
        unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
    }
    // SAFETY: as above.
    unsafe { libc::fcntl(fds[1], libc::F_SETFL, libc::O_NONBLOCK) };
    PIPE.store(fds[1], Ordering::Relaxed);
    // SAFETY: the handler only calls write, which is async-signal-safe.
    unsafe {
        libc::signal(
            libc::SIGWINCH,
            on_sigwinch as extern "C" fn(libc::c_int) as libc::sighandler_t,
        )
    };
    // SAFETY: both ends are ours alone from here.
    let (mut pipe, write_end) =
        unsafe { (File::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    thread::Builder::new()
        .name("vedi-sigwinch".into())
        .spawn(move || {
            let mut buf = [0; 64];
            loop {
                match pipe.read(&mut buf) {
                    Ok(0) => return,
                    Ok(_) => {}
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(_) => return,
                }
                let (w, h) = size(&tty);
                if tx.send(Event::Resize(w, h)).is_err() {
                    return;
                }
            }
        })
        .expect("spawn sigwinch thread");
    Sigwinch(Some(write_end))
}

/// Holds the pipe's write end; dropping it puts the signal back to its
/// default and ends the thread.
pub struct Sigwinch(Option<OwnedFd>);

impl Drop for Sigwinch {
    fn drop(&mut self) {
        if self.0.is_none() {
            return;
        }
        // SAFETY: restores the default disposition; no pointers.
        unsafe { libc::signal(libc::SIGWINCH, libc::SIG_DFL) };
        PIPE.store(-1, Ordering::Relaxed);
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
