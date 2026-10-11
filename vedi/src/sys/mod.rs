pub mod clipboard;
pub mod opencmd;
pub mod term;
pub mod tty;
pub mod watch;
pub mod words;

use std::fs::{self, File};
use std::io::{self, Read, Seek};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;
use std::sync::atomic::{AtomicUsize, Ordering};

/// An unnamed temp file for a command's stderr, or `None` when none
/// can be made. A file, not a pipe: a descendant that keeps it open
/// holds up no one.
fn scratch() -> Option<File> {
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("vedi-stderr-{}-{n}", std::process::id()));
    let f = File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .ok()?;
    let _ = fs::remove_file(&path);
    Some(f)
}

/// A failed command's error, `exit status N` or `signal: N`, with
/// what it wrote to `stderr`.
fn failure(status: ExitStatus, stderr: Option<&mut File>) -> String {
    let err = match status.code() {
        Some(c) => format!("exit status {c}"),
        None => format!("signal: {}", status.signal().unwrap_or_default()),
    };
    let mut msg = Vec::new();
    if let Some(f) = stderr {
        let _ = f.rewind();
        let _ = f.read_to_end(&mut msg);
    }
    // A bad byte costs itself, not the message.
    match String::from_utf8_lossy(&msg).trim() {
        "" => err,
        msg => format!("{err}: {msg}"),
    }
}

/// An error as the status line shows it: an OS error is its lowercase
/// strerror, without Rust's "(os error N)", a number that tells the
/// reader nothing.
pub fn strerror(e: &io::Error) -> String {
    let s = e.to_string();
    let s = match e.raw_os_error() {
        Some(n) => s
            .strip_suffix(&format!(" (os error {n})"))
            .unwrap_or(&s)
            .to_string(),
        None => return s,
    };
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_lowercase().chain(c).collect())
        .unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strerror_as_go() {
        let e = io::Error::from_raw_os_error(libc::ENOENT);
        assert_eq!(strerror(&e), "no such file or directory");
        assert_eq!(strerror(&io::Error::other("Mine")), "Mine");
    }
}
