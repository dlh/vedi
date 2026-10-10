//! Reads an input by running a command on it.
#![allow(unsafe_code)]

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, Read};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use super::{failure, scratch};
use crate::text::buffer::ReadError;
use crate::text::source::Source;

/// How long Drop waits for a killed command to be reaped, as Go's
/// `WaitDelay`.
const WAIT_DELAY: Duration = Duration::from_secs(1);

/// A running command's output. Its failure, with its stderr, is the
/// read error once the output ends.
pub struct Cmd {
    child: Child,
    out: ChildStdout,
    stderr: Option<File>,
    state: State,
    killer: Killer,
}

/// Kills a command whose reader may be blocked on it, which then sees
/// the output end.
#[derive(Clone)]
pub struct Killer {
    pid: libc::pid_t,
    reaped: Arc<AtomicBool>,
}

impl Killer {
    /// Kills the command and what it started. Once it is reaped its id
    /// may be reused, so nothing is killed.
    pub fn kill(&self) {
        if self.reaped() {
            return;
        }
        // SAFETY: kill takes no pointers. The group is the child's,
        // unreaped when checked; one reaped since, its id reused within
        // that window, would be killed, as Go's Process.Kill risks too.
        unsafe { libc::kill(-self.pid, libc::SIGKILL) };
    }

    pub fn reaped(&self) -> bool {
        self.reaped.load(Ordering::SeqCst)
    }
}

enum State {
    Running,
    /// Reaped: its group may be gone and its id reused, so it is not
    /// killed. The error is `read_error`'s, until taken.
    Reaped(Option<ReadError>),
}

/// `arg` with each %s replaced by `name`, which need not be UTF-8.
fn subst(arg: &str, name: &OsStr) -> OsString {
    let mut out = Vec::new();
    for (i, part) in arg.split("%s").enumerate() {
        if i > 0 {
            out.extend_from_slice(name.as_bytes());
        }
        out.extend_from_slice(part.as_bytes());
    }
    OsString::from_vec(out)
}

/// Runs `argv` with each `%s` replaced by `name`, or `name` appended
/// when there is none. The command reads nothing. The error is one of
/// starting it; a failure after that is the read error.
pub fn open(argv: &[String], name: &OsStr) -> io::Result<Cmd> {
    let mut args: Vec<OsString> = argv.iter().map(|a| subst(a, name)).collect();
    if !argv.iter().any(|a| a.contains("%s")) {
        args.push(name.to_os_string());
    }
    let Some((prog, rest)) = args.split_first() else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "no command"));
    };
    let stderr = scratch();
    let mut cmd = Command::new(prog);
    cmd.args(rest)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        // Its own group, so Drop kills what it started too.
        .process_group(0);
    match &stderr {
        Some(f) => cmd.stderr(f.try_clone()?),
        None => cmd.stderr(Stdio::null()),
    };
    let mut child = cmd.spawn()?;
    let out = child.stdout.take().expect("stdout piped");
    let pid = libc::pid_t::try_from(child.id()).map_err(io::Error::other)?;
    Ok(Cmd {
        child,
        out,
        stderr,
        state: State::Running,
        killer: Killer {
            pid,
            reaped: Arc::new(AtomicBool::new(false)),
        },
    })
}

impl Cmd {
    pub fn killer(&self) -> Killer {
        self.killer.clone()
    }
}

impl Source for Cmd {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.out.read(buf)?;
        if n == 0 && !buf.is_empty() && matches!(self.state, State::Running) {
            let status = self.child.wait()?;
            self.killer.reaped.store(true, Ordering::SeqCst);
            let err = (!status.success())
                .then(|| ReadError::Command(failure(status, self.stderr.as_mut())));
            self.state = State::Reaped(err);
        }
        Ok(n)
    }

    fn read_error(&mut self) -> Option<ReadError> {
        match &mut self.state {
            State::Reaped(err) => err.take(),
            State::Running => None,
        }
    }
}

impl Drop for Cmd {
    fn drop(&mut self) {
        if !matches!(self.state, State::Running) {
            return;
        }
        self.killer.kill();
        // Polled, not waited: a command stuck in the kernel, which
        // SIGKILL does not end at once, is left behind rather than
        // holding up the caller.
        let until = Instant::now() + WAIT_DELAY;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => {
                    self.killer.reaped.store(true, Ordering::SeqCst);
                    return;
                }
                Ok(None) if Instant::now() < until => thread::sleep(Duration::from_millis(5)),
                _ => return,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{Duration, Instant};

    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    /// Reads `c` to its end: the output, and the error it ended with.
    fn read_all(c: &mut Cmd) -> (String, Option<String>) {
        let mut out = Vec::new();
        let mut buf = [0; 4];
        loop {
            let n = Source::read(c, &mut buf).unwrap();
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }
        let err = c.read_error().map(|e| e.to_string());
        (String::from_utf8(out).unwrap(), err)
    }

    fn read_exact(c: &mut Cmd, n: usize) -> String {
        let mut out = Vec::new();
        let mut buf = vec![0; n];
        while out.len() < n {
            let k = Source::read(c, &mut buf[..n - out.len()]).unwrap();
            assert!(k > 0, "early end");
            out.extend_from_slice(&buf[..k]);
        }
        String::from_utf8(out).unwrap()
    }

    // %s is the file name, one argument even with a space in it, and
    // the command's output is what is read.
    #[test]
    fn open_file() {
        let name = std::env::temp_dir().join(format!("vedi-open {}.txt", std::process::id()));
        fs::write(&name, "hello\n").unwrap();
        let got = open(&args(&["cat", "%s"]), name.as_os_str()).map(|mut c| read_all(&mut c));
        let _ = fs::remove_file(&name);
        assert_eq!(got.unwrap(), ("hello\n".into(), None));
    }

    // The name's bytes reach the command as they are.
    #[test]
    fn open_name_not_utf8() {
        let name = OsString::from_vec(vec![b'f', 0xff]);
        let mut c = open(&args(&["echo", "%s"]), &name).unwrap();
        let mut out = Vec::new();
        let mut buf = [0; 8];
        loop {
            let n = Source::read(&mut c, &mut buf).unwrap();
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }
        assert_eq!(out, [b'f', 0xff, b'\n']);
    }

    #[test]
    fn open_inside_word() {
        let mut c = open(&args(&["echo", "file=%s"]), OsStr::new("f.rs")).unwrap();
        assert_eq!(read_all(&mut c), ("file=f.rs\n".into(), None));
    }

    // A command without %s gets the name as its last argument, and its
    // stdin is empty.
    #[test]
    fn open_without_percent() {
        let mut c = open(&args(&["echo", "-n"]), OsStr::new("f.rs")).unwrap();
        assert_eq!(read_all(&mut c), ("f.rs".into(), None));
        let mut c = open(&args(&["sh", "-c", "cat; echo \"$0\""]), OsStr::new("f.rs")).unwrap();
        assert_eq!(read_all(&mut c), ("f.rs\n".into(), None));
    }

    // A non-zero exit is the read error once the output ends, with what
    // the command wrote to stderr.
    #[test]
    fn open_fails() {
        let argv = args(&[
            "sh",
            "-c",
            "echo partial; echo oops >&2; exit 3",
            "sh",
            "%s",
        ]);
        let mut c = open(&argv, OsStr::new("f")).unwrap();
        assert_eq!(
            read_all(&mut c),
            ("partial\n".into(), Some("exit status 3: oops".into()))
        );
    }

    #[test]
    fn open_not_found() {
        assert!(open(&args(&["vedi-no-such-command", "%s"]), OsStr::new("f")).is_err());
    }

    // Drop ends a command still running, without waiting for it to
    // finish on its own.
    #[test]
    fn drop_kills() {
        let c = open(&args(&["sleep", "10"]), OsStr::new("f")).unwrap();
        let start = Instant::now();
        drop(c);
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    // A child of the command holding its stderr does not hold up Drop
    // either.
    #[test]
    fn drop_kills_descendants() {
        let mut c = open(
            &args(&["sh", "-c", "sleep 10 & echo ready; wait"]),
            OsStr::new("f"),
        )
        .unwrap();
        assert_eq!(read_exact(&mut c, 6), "ready\n");
        let start = Instant::now();
        drop(c);
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    // A command that closed its stdout and runs on is ended too.
    #[test]
    fn drop_after_stdout_closed() {
        let mut c = open(
            &args(&["sh", "-c", "echo ready; exec 1>&-; sleep 10"]),
            OsStr::new("f"),
        )
        .unwrap();
        assert_eq!(read_exact(&mut c, 6), "ready\n");
        let start = Instant::now();
        drop(c);
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    // A reader blocked on a command that writes nothing is freed by its
    // killer, and the command is gone.
    #[test]
    fn killer_frees_a_blocked_reader() {
        let mut c = open(&args(&["sh", "-c", "exec sleep 3"]), OsStr::new("f")).unwrap();
        let killer = c.killer();
        let (tx, rx) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let got = read_all(&mut c);
            drop(c);
            let _ = tx.send(got);
        });
        std::thread::sleep(Duration::from_millis(50));
        killer.kill();
        let got = rx
            .recv_timeout(Duration::from_secs(1))
            .expect("the read did not return");
        reader.join().unwrap();
        assert_eq!(got, (String::new(), Some("signal: 9".into())));
        // SAFETY: signal 0 only checks that the process exists.
        let alive = unsafe { libc::kill(killer.pid, 0) } == 0;
        assert!(!alive, "the command is still there");
        killer.kill();
    }

    // A command reaped by Drop is not killed again: its id may be
    // reused.
    #[test]
    fn drop_marks_reaped() {
        let c = open(&args(&["sh", "-c", "exec sleep 3"]), OsStr::new("f")).unwrap();
        let killer = c.killer();
        assert!(!killer.reaped.load(Ordering::SeqCst));
        drop(c);
        assert!(killer.reaped.load(Ordering::SeqCst));
    }

    // Dropping after the output ended is fine.
    #[test]
    fn drop_after_eof() {
        let mut c = open(&args(&["echo", "hi"]), OsStr::new("f")).unwrap();
        assert_eq!(read_all(&mut c), ("hi f\n".into(), None));
        drop(c);
    }
}
