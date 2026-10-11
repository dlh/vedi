#![allow(unsafe_code)]

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use crate::screen::{Queries, Screen};

/// Why a measurement failed; its text goes in the report.
#[derive(Debug)]
pub enum Error {
    Timeout,
    /// The pager exited before the text showed.
    Exited(String),
    DidNotQuit,
    /// The pager exited badly.
    Status(String),
    /// Not on PATH, or a path that is not there.
    NotFound(OsString),
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Timeout => f.write_str("timeout"),
            Error::Exited(text) => write!(f, "exited before {text:?}"),
            Error::DidNotQuit => f.write_str("did not quit"),
            Error::Status(s) => f.write_str(s),
            Error::NotFound(name) => write!(f, "{}: not found", name.to_string_lossy()),
            Error::Io(e) => e.fmt(f),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

/// A pager on an 80x24 pty. The reader thread answers its queries and
/// passes on what it writes, which `screen` takes as `wait` asks.
pub struct Session {
    pid: libc::pid_t,
    tty: File,
    out: Receiver<Vec<u8>>,
    screen: Screen,
    exited: bool,
    reaped: bool,
}

/// Held from openpty to spawn: a fork in between would inherit the
/// pty's fds, which are not close-on-exec until set, and keep the pty
/// open after its pager exits.
static SPAWN: Mutex<()> = Mutex::new(());

/// Runs argv on a pty in dir, reading stdin from it unless one is
/// given. The controlling terminal is named by stdout, since stdin may
/// be the pipe.
pub fn start(
    dir: Option<&Path>,
    argv: &[OsString],
    stdin: Option<Stdio>,
) -> Result<Session, Error> {
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .env_clear()
        .envs(env(std::env::vars_os(), dir));
    if let Some(dir) = dir {
        cmd.current_dir(dir);
    }
    // SAFETY: setsid and ioctl are async-signal-safe, and the closure
    // allocates nothing.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() < 0 || libc::ioctl(1, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let (master, child) = {
        let _held = SPAWN.lock().unwrap_or_else(|e| e.into_inner());
        let (master, slave) = openpty()?;
        cmd.stdin(match stdin {
            Some(s) => s,
            None => slave.try_clone()?.into(),
        })
        .stdout(slave.try_clone()?)
        .stderr(slave);
        let child = cmd.spawn().map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => Error::NotFound(argv[0].clone()),
            _ => Error::Io(e),
        })?;
        (master, child)
    };
    // The slave's and stdin's fds in cmd close here, so the pty ends
    // when the pager does.
    drop(cmd);
    let pid = child.id() as libc::pid_t;
    let mut reader = master.try_clone()?;
    let mut answer = master.try_clone()?;
    let (tx, out) = mpsc::channel();
    thread::spawn(move || {
        let mut queries = Queries::new();
        let mut buf = vec![0; 64 << 10];
        loop {
            let n = match reader.read(&mut buf) {
                Ok(0) => return,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return,
            };
            let asked = queries.scan(&buf[..n]);
            if tx.send(buf[..n].to_vec()).is_err() {
                return;
            }
            for q in asked {
                let _ = answer.write_all(q.reply());
            }
        }
    });
    Ok(Session {
        pid,
        tty: master,
        out,
        screen: Screen::new(24, 80),
        exited: false,
        reaped: false,
    })
}

fn openpty() -> io::Result<(File, File)> {
    let mut master = -1;
    let mut slave = -1;
    let mut size = libc::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: the out-pointers are valid; a null name and termios are
    // allowed.
    let r = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut size,
        )
    };
    if r < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: openpty returned these fds, and nothing else owns them.
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    for fd in [&master, &slave] {
        // SAFETY: fd is open.
        if unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok((master.into(), slave.into()))
}

/// The environment with LESS* and VEDI removed and TERM set, so each
/// pager runs with its defaults and less writes no history. The config
/// home is `dir`, the pager's directory, which holds only the input:
/// a vedi.conf of the user's, with an open_cmd say, is not found.
pub fn env(
    vars: impl IntoIterator<Item = (OsString, OsString)>,
    dir: Option<&Path>,
) -> Vec<(OsString, OsString)> {
    let mut e: Vec<_> = vars
        .into_iter()
        .filter(|(k, _)| {
            !k.as_bytes().starts_with(b"LESS")
                && k != "VEDI"
                && k != "TERM"
                && (dir.is_none() || k != "XDG_CONFIG_HOME")
        })
        .collect();
    e.push(("TERM".into(), "xterm-256color".into()));
    e.push(("LESSHISTFILE".into(), "-".into()));
    if let Some(dir) = dir {
        e.push(("XDG_CONFIG_HOME".into(), dir.into()));
    }
    e
}

impl Session {
    /// Types keys.
    pub fn send(&mut self, keys: &str) -> io::Result<()> {
        self.tty.write_all(keys.as_bytes())
    }

    /// Blocks until text shows on the screen, the pager exits, or
    /// timeout passes.
    pub fn wait(&mut self, text: &str, timeout: Duration) -> Result<(), Error> {
        let deadline = Instant::now() + timeout;
        loop {
            if self.screen.contains(text) {
                return Ok(());
            }
            if self.exited {
                return Err(Error::Exited(text.to_string()));
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Error::Timeout);
            }
            match self.out.recv_timeout(left) {
                Ok(bytes) => {
                    self.screen.write(&bytes);
                    // One look at the screen for all that has come.
                    while let Ok(bytes) = self.out.try_recv() {
                        self.screen.write(&bytes);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => self.exited = true,
            }
        }
    }

    /// Waits for the pager to exit and returns its peak RSS in bytes;
    /// after timeout it is killed.
    pub fn finish(mut self, timeout: Duration) -> Result<u64, Error> {
        let deadline = Instant::now() + timeout;
        let (status, usage) = loop {
            if let Some(done) = wait4(self.pid, libc::WNOHANG)? {
                break done;
            }
            if Instant::now() >= deadline {
                self.kill_group();
                wait4(self.pid, 0)?;
                self.reaped = true;
                return Err(Error::DidNotQuit);
            }
            thread::sleep(Duration::from_millis(10));
        };
        self.reaped = true;
        if libc::WIFSIGNALED(status) {
            return Err(Error::Status(format!("signal {}", libc::WTERMSIG(status))));
        }
        if libc::WEXITSTATUS(status) != 0 {
            return Err(Error::Status(format!(
                "exit status {}",
                libc::WEXITSTATUS(status)
            )));
        }
        // Linux counts kilobytes, macOS bytes.
        let unit = if cfg!(target_os = "linux") { 1024 } else { 1 };
        Ok(usage.ru_maxrss as u64 * unit)
    }

    /// Kills the pager and what it started: a child in a session of its
    /// own would go on without its terminal. The pager leads its own
    /// process group, since start made it a session.
    fn kill_group(&self) {
        if let Ok(out) = Command::new("pgrep")
            .arg("-P")
            .arg(self.pid.to_string())
            .output()
        {
            for child in String::from_utf8_lossy(&out.stdout).split_whitespace() {
                if let Ok(child) = child.parse::<libc::pid_t>() {
                    // SAFETY: kill takes any pid.
                    unsafe { libc::kill(child, libc::SIGKILL) };
                }
            }
        }
        // SAFETY: kill takes any pid; the group is the pager's, which is
        // not yet reaped, so its pid is not reused.
        unsafe { libc::kill(-self.pid, libc::SIGKILL) };
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if !self.reaped {
            self.kill_group();
            let _ = wait4(self.pid, 0);
        }
    }
}

/// The exit status and resource usage of pid, or None if WNOHANG and it
/// is running. wait4 gives the usage of this child alone, where
/// getrusage(RUSAGE_CHILDREN) gives the peak over all of them.
fn wait4(pid: libc::pid_t, flags: libc::c_int) -> io::Result<Option<(libc::c_int, libc::rusage)>> {
    let mut status = 0;
    // SAFETY: rusage is plain data, for which zero is valid.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    loop {
        // SAFETY: the out-pointers are valid.
        let r = unsafe { libc::wait4(pid, &mut status, flags, &mut usage) };
        match r {
            0 => return Ok(None),
            r if r > 0 => return Ok(Some((status, usage))),
            _ => {
                let e = io::Error::last_os_error();
                if e.kind() != io::ErrorKind::Interrupted {
                    return Err(e);
                }
            }
        }
    }
}

/// Where the shell would find name: itself when it has a slash.
pub fn look_path(name: &OsStr) -> Option<std::path::PathBuf> {
    let exec = |p: &Path| {
        use std::os::unix::fs::PermissionsExt;
        p.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    if name.as_bytes().contains(&b'/') {
        let p = Path::new(name);
        return exec(p).then(|| p.to_path_buf());
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|p| exec(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TempDir;
    use crate::input::{marker, write_input};
    use std::collections::HashMap;

    fn argv(a: &[&str]) -> Vec<OsString> {
        a.iter().map(OsString::from).collect()
    }

    /// Each pager runs with its defaults, so the variables that
    /// configure less and vedi are dropped, TERM is fixed, and the
    /// config home is the pager's directory, which holds no config.
    #[test]
    fn environment() {
        let given = [
            ("LESS", "-R"),
            ("LESSOPEN", "|cat %s"),
            ("VEDI", "-F -S"),
            ("TERM", "dumb"),
            ("XDG_CONFIG_HOME", "/home/me/.config"),
            ("KEEP", "1"),
        ];
        let vars = || given.map(|(k, v)| (k.into(), v.into()));
        let got: HashMap<OsString, OsString> =
            env(vars(), Some(Path::new("/work"))).into_iter().collect();
        for k in ["LESS", "LESSOPEN", "VEDI"] {
            assert!(!got.contains_key(OsStr::new(k)), "{k} kept");
        }
        let get = |k: &str| got.get(OsStr::new(k)).and_then(|v| v.to_str());
        assert_eq!(get("TERM"), Some("xterm-256color"));
        assert_eq!(get("LESSHISTFILE"), Some("-"));
        assert_eq!(get("XDG_CONFIG_HOME"), Some("/work"));
        assert_eq!(get("KEEP"), Some("1"));
        let got: HashMap<OsString, OsString> = env(vars(), None).into_iter().collect();
        assert_eq!(
            got.get(OsStr::new("XDG_CONFIG_HOME"))
                .and_then(|v| v.to_str()),
            Some("/home/me/.config")
        );
    }

    /// A shell stand-in for a pager prints the file, waits for a key,
    /// says bye and exits; RSS is positive whatever the platform.
    #[test]
    fn session() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("in");
        write_input(&file, 3).unwrap();
        let mut a = argv(&["sh", "-c", r#"cat "$1"; read k; echo bye"#, "sh"]);
        a.push(file.into());
        let mut s = start(None, &a, None).unwrap();
        s.wait(&marker(3), Duration::from_secs(5)).unwrap();
        s.send("\r").unwrap();
        s.wait("bye", Duration::from_secs(5)).unwrap();
        let rss = s.finish(Duration::from_secs(5)).unwrap();
        assert!(rss > 0, "rss = {rss}");
    }

    /// A pipe on stdin still leaves the pty as the controlling terminal.
    #[test]
    fn session_stdin() {
        let (r, mut w) = io::pipe().unwrap();
        w.write_all(b"hello\n").unwrap();
        drop(w);
        let mut s = start(None, &argv(&["cat"]), Some(r.into())).unwrap();
        s.wait("hello", Duration::from_secs(5)).unwrap();
        s.finish(Duration::from_secs(5)).unwrap();
    }

    #[test]
    fn wait_timeout() {
        let mut s = start(None, &argv(&["sleep", "5"]), None).unwrap();
        let err = s.wait("never", Duration::from_millis(50));
        assert!(matches!(err, Err(Error::Timeout)), "{err:?}");
        drop(s);
    }

    /// A pager that exits first fails the wait at once.
    #[test]
    fn wait_exit() {
        let mut s = start(None, &argv(&["true"]), None).unwrap();
        let t = Instant::now();
        let err = s.wait("never", Duration::from_secs(5));
        assert!(matches!(err, Err(Error::Exited(_))), "{err:?}");
        assert!(t.elapsed() < Duration::from_secs(4));
        s.finish(Duration::from_secs(1)).unwrap();
    }

    /// The pty answers a device attributes query, as a terminal would.
    #[test]
    fn answers_query() {
        let script =
            r#"stty raw -echo; printf '\033[c'; dd bs=1 count=9 2>/dev/null | od -c | head -1"#;
        let mut s = start(None, &argv(&["sh", "-c", script]), None).unwrap();
        s.wait("?   6   2   ;   2   2   c", Duration::from_secs(5))
            .unwrap();
        s.finish(Duration::from_secs(5)).unwrap();
    }

    #[test]
    fn not_found() {
        for name in ["vedi-bench-no-such-pager", "./vedi-bench-no-such-pager"] {
            let err = start(None, &argv(&[name]), None).err();
            assert_eq!(
                err.map(|e| e.to_string()),
                Some(format!("{name}: not found"))
            );
        }
    }

    #[test]
    fn killed() {
        let s = start(None, &argv(&["sh", "-c", "kill -9 $$"]), None).unwrap();
        let err = s.finish(Duration::from_secs(5)).err();
        assert_eq!(err.map(|e| e.to_string()).as_deref(), Some("signal 9"));
    }

    #[test]
    fn look_path_finds() {
        assert!(look_path(OsStr::new("sh")).is_some_and(|p| p.is_absolute()));
        assert_eq!(look_path(OsStr::new("vedi-bench-no-such-pager")), None);
        assert_eq!(
            look_path(OsStr::new("/bin/sh")).as_deref(),
            Some(Path::new("/bin/sh"))
        );
    }
}
