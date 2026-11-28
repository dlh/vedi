//! The terminal's size, word of its changes, and its mode.
#![allow(unsafe_code)]

use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use signal_hook::consts::SIGWINCH;
use signal_hook::iterator::{Handle, Signals};

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

/// The settings `raw` replaced.
static COOKED: Mutex<Option<libc::termios>> = Mutex::new(None);

/// Puts the terminal on `tty` in raw mode, until `cooked`.
pub fn raw(tty: &File) -> io::Result<()> {
    let fd = tty.as_raw_fd();
    // SAFETY: termios is plain data, zeroed is valid.
    let mut t: libc::termios = unsafe { std::mem::zeroed() };
    // SAFETY: tcgetattr writes a termios through the pointer, which is
    // to a live one.
    if unsafe { libc::tcgetattr(fd, &mut t) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let was = t;
    // SAFETY: cfmakeraw edits the termios in place, and tcsetattr only
    // reads it.
    let ok = unsafe {
        libc::cfmakeraw(&mut t);
        libc::tcsetattr(fd, libc::TCSANOW, &t) == 0
    };
    if !ok {
        return Err(io::Error::last_os_error());
    }
    *COOKED.lock().unwrap_or_else(PoisonError::into_inner) = Some(was);
    Ok(())
}

/// Undoes `raw`, once.
pub fn cooked(tty: &File) {
    let was = COOKED.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(t) = was {
        // SAFETY: tcsetattr only reads the termios.
        unsafe { libc::tcsetattr(tty.as_raw_fd(), libc::TCSANOW, &t) };
    }
}

/// Waits for input on `tty`; false when none came within `wait`.
/// With select: on macOS poll says /dev/tty is invalid.
pub fn readable_within(tty: &File, wait: Duration) -> bool {
    let fd = tty.as_raw_fd();
    // An fd_set holds no descriptor past its size. Such a wait is over
    // at once: saying input came would leave the caller in a read with
    // no end, where this way it only gives up early.
    if usize::try_from(fd).is_ok_and(|fd| fd >= libc::FD_SETSIZE) {
        return false;
    }
    loop {
        let mut tv = libc::timeval {
            tv_sec: wait.as_secs().try_into().unwrap_or(0),
            // Below a million, so it fits; the type is i32 on macOS
            // and i64 on Linux, where a checked conversion is a lint.
            tv_usec: wait.subsec_micros() as libc::suseconds_t,
        };
        // SAFETY: fd_set is plain data, zeroed is valid; fd is open and
        // below FD_SETSIZE, and the set and timeout outlive the call.
        let n = unsafe {
            let mut set: libc::fd_set = std::mem::zeroed();
            libc::FD_SET(fd, &mut set);
            libc::select(
                fd + 1,
                &mut set,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut tv,
            )
        };
        // A signal, SIGWINCH for one, starts the wait over. Any other
        // failure is left for the read to report.
        if n < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
            continue;
        }
        return n != 0;
    }
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

/// Calls `resized` with the size of `tty` per SIGWINCH, until it
/// returns false or the guard drops.
pub fn sigwinch(
    tty: File,
    mut resized: impl FnMut(usize, usize) -> bool + Send + 'static,
) -> Sigwinch {
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
                if !resized(w, h) {
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

    use super::*;

    /// A descriptor select cannot watch is not waited on, readable or
    /// not.
    #[test]
    fn no_wait_past_the_fd_set() {
        let min = libc::FD_SETSIZE as libc::c_int;
        // SAFETY: rlimit is plain data, zeroed is valid; both calls
        // take a pointer to a live one. F_DUPFD touches no memory, and
        // the descriptor it returns is owned here.
        let high = unsafe {
            use std::os::fd::FromRawFd;
            let mut lim: libc::rlimit = std::mem::zeroed();
            assert_eq!(libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim), 0);
            // Room for one past the set; never lowered.
            let want = (min as libc::rlim_t + 64).min(lim.rlim_max);
            if lim.rlim_cur < want {
                lim.rlim_cur = want;
                assert_eq!(libc::setrlimit(libc::RLIMIT_NOFILE, &lim), 0);
            }
            let null = File::open("/dev/null").unwrap();
            let fd = libc::fcntl(null.as_raw_fd(), libc::F_DUPFD_CLOEXEC, min);
            assert!(fd >= min, "{}", io::Error::last_os_error());
            File::from_raw_fd(fd)
        };
        let start = std::time::Instant::now();
        assert!(!readable_within(&high, Duration::from_secs(5)));
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn waits_for_input() {
        let (mut parent, mut child) = (0, 0);
        // SAFETY: openpty writes two descriptors; the name and settings
        // pointers may be null.
        let ok = unsafe {
            libc::openpty(
                &mut parent,
                &mut child,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert_eq!(ok, 0);
        // SAFETY: both descriptors were just opened and are owned here.
        let (mut parent, child) = unsafe {
            use std::os::fd::FromRawFd;
            (File::from_raw_fd(parent), File::from_raw_fd(child))
        };
        assert!(!readable_within(&child, Duration::from_millis(10)));
        // The pty is cooked: a line is what makes it readable.
        io::Write::write_all(&mut parent, b"q\n").unwrap();
        assert!(readable_within(&child, Duration::from_secs(2)));
    }

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
        let _guard = sigwinch(File::open("/dev/null").unwrap(), move |w, h| {
            tx.send((w, h)).is_ok()
        });
        // SAFETY: raise sends a signal to this thread; the handler is
        // installed.
        assert_eq!(unsafe { libc::raise(libc::SIGWINCH) }, 0);
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(2)),
            Ok((w, h)) if w > 0 && h > 0
        ));
    }
}
