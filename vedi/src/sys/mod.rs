pub mod clipboard;
pub mod opencmd;
pub mod term;
pub mod tty;
pub mod watch;
pub mod words;

use std::fs::{self, File};
use std::io::{Read, Seek};
use std::os::unix::fs::OpenOptionsExt;
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

/// A failed command's exit status with what it wrote to `stderr`.
fn failure(status: ExitStatus, stderr: Option<&mut File>) -> String {
    let err = status.to_string();
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
