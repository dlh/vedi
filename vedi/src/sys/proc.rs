//! Runs commands in process groups of their own, so a kill reaches
//! what they started.
#![allow(unsafe_code)]

use std::fs::{self, File};
use std::io::{self, Read, Seek};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

/// How long Drop waits for a killed command to be reaped: SIGKILL
/// takes effect at once unless the command is stuck in the kernel,
/// and then no wait would do.
const WAIT_DELAY: Duration = Duration::from_secs(1);

/// A command running in a process group of its own, its stderr kept
/// for its failure. Dropped unreaped, it is killed with what it
/// started.
pub struct Group {
    child: Child,
    killer: Killer,
    stderr: Option<File>,
}

impl Group {
    /// Spawns `cmd`, whose stdin and stdout are the caller's to set.
    pub fn spawn(cmd: &mut Command) -> io::Result<Group> {
        let stderr = scratch();
        cmd.process_group(0);
        match &stderr {
            Some(f) => cmd.stderr(f.try_clone()?),
            None => cmd.stderr(Stdio::null()),
        };
        let child = cmd.spawn()?;
        let killer = Killer::new(&child)?;
        Ok(Group {
            child,
            killer,
            stderr,
        })
    }

    /// Takes the command's stdin, when piped.
    pub fn stdin(&mut self) -> Option<ChildStdin> {
        self.child.stdin.take()
    }

    /// Takes the command's stdout, when piped.
    pub fn stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }

    pub fn killer(&self) -> Killer {
        self.killer.clone()
    }

    /// Waits for the command: `None` when it succeeded, else its exit
    /// status with what it wrote to stderr. Reaped, it is not killed.
    /// A wait that fails has no child either: SIGCHLD ignored at
    /// startup reaps it before wait can.
    pub fn wait(&mut self) -> io::Result<Option<String>> {
        let res = self.child.wait();
        self.killer.reaped.store(true, Ordering::SeqCst);
        let status = res?;
        Ok((!status.success()).then(|| failure(status, self.stderr.as_mut())))
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        if self.killer.reaped() {
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
                Ok(None) => return,
                // No child to wait for: reaped already, as under an
                // ignored SIGCHLD.
                Err(_) => {
                    self.killer.reaped.store(true, Ordering::SeqCst);
                    return;
                }
            }
        }
    }
}

/// An unnamed temp file for a command's stderr, or `None` when none
/// can be made. A file, not a pipe: a descendant that keeps it open,
/// as a daemon forked to serve the clipboard does, holds up no one.
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

/// Kills a command whose reader may be blocked on it, which then sees
/// the output end.
#[derive(Clone)]
pub struct Killer {
    pid: libc::pid_t,
    reaped: Arc<AtomicBool>,
}

impl Killer {
    fn new(child: &Child) -> io::Result<Killer> {
        Ok(Killer {
            pid: libc::pid_t::try_from(child.id()).map_err(io::Error::other)?,
            reaped: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Kills the command and what it started. Once it is reaped its id
    /// may be reused, so nothing is killed.
    pub fn kill(&self) {
        if self.reaped() {
            return;
        }
        // SAFETY: kill takes no pointers. The group is the child's,
        // unreaped when checked; one reaped since, its id reused within
        // that window, would be killed; the window is one syscall wide.
        unsafe { libc::kill(-self.pid, libc::SIGKILL) };
    }

    pub fn reaped(&self) -> bool {
        self.reaped.load(Ordering::SeqCst)
    }
}

/// Commands to kill when vedi exits. A `Mutex`, as the panic hook
/// kills them from whichever thread panicked.
#[derive(Clone, Default)]
pub struct Killers(Arc<Mutex<Vec<Killer>>>);

impl Killers {
    pub fn add(&self, k: Killer) {
        self.kept_mut().push(k);
    }

    /// Adds `new`, handing back the commands before them: a reload
    /// kills those once its text is read. They stay in the set until
    /// reaped, so exit reaches them when the reload never ends.
    pub fn supersede(&self, new: Vec<Killer>) -> Vec<Killer> {
        let mut all = self.kept_mut();
        let old = all.clone();
        all.extend(new);
        old
    }

    pub fn kill_all(&self) {
        self.lock().iter().for_each(Killer::kill);
    }

    /// How many are kept: those added, less the reaped let go since.
    pub fn kept(&self) -> usize {
        self.lock().len()
    }

    pub fn all_reaped(&self) -> bool {
        self.lock().iter().all(Killer::reaped)
    }

    /// The set, those reaped let go.
    fn kept_mut(&self) -> MutexGuard<'_, Vec<Killer>> {
        let mut all = self.lock();
        all.retain(|k| !k.reaped());
        all
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Killer>> {
        // A panic while held left the list whole.
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::sync::mpsc;

    use super::*;

    fn sh(script: &str) -> Group {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", script])
            .stdin(Stdio::null())
            .stdout(Stdio::piped());
        Group::spawn(&mut cmd).unwrap()
    }

    /// `g`'s output to its end.
    fn read_all(g: &mut Group) -> String {
        let mut out = String::new();
        g.stdout().unwrap().read_to_string(&mut out).unwrap();
        out
    }

    fn read_exact(g: &mut Group, n: usize) -> String {
        let mut out = vec![0; n];
        g.stdout().unwrap().read_exact(&mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    // A non-zero exit is the failure, with what the command wrote to
    // stderr; a success is none.
    #[test]
    fn wait_reports_failure() {
        let mut g = sh("echo partial; echo oops >&2; exit 3");
        assert_eq!(read_all(&mut g), "partial\n");
        assert_eq!(g.wait().unwrap(), Some("exit status: 3: oops".into()));
        let mut g = sh("exit 0");
        assert_eq!(g.wait().unwrap(), None);
        // A byte that is not UTF-8 loses nothing but itself.
        let mut g = sh("printf 'bad \\377 byte' >&2; exit 3");
        assert_eq!(
            g.wait().unwrap(),
            Some("exit status: 3: bad \u{fffd} byte".into())
        );
    }

    // Drop ends a command still running, without waiting for it to
    // finish on its own.
    #[test]
    fn drop_kills() {
        let g = sh("exec sleep 10");
        let start = Instant::now();
        drop(g);
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    // A child of the command holding its stderr does not hold up Drop
    // either.
    #[test]
    fn drop_kills_descendants() {
        let mut g = sh("sleep 10 & echo ready; wait");
        assert_eq!(read_exact(&mut g, 6), "ready\n");
        let start = Instant::now();
        drop(g);
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    // A command that closed its stdout and runs on is ended too.
    #[test]
    fn drop_after_stdout_closed() {
        let mut g = sh("echo ready; exec 1>&-; sleep 10");
        assert_eq!(read_exact(&mut g, 6), "ready\n");
        let start = Instant::now();
        drop(g);
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    // A reader blocked on a command that writes nothing is freed by its
    // killer, and the command is gone.
    #[test]
    fn killer_frees_a_blocked_reader() {
        let mut g = sh("exec sleep 3");
        let killer = g.killer();
        let (tx, rx) = mpsc::channel();
        let reader = thread::spawn(move || {
            let out = read_all(&mut g);
            let _ = tx.send((out, g.wait().unwrap()));
        });
        thread::sleep(Duration::from_millis(50));
        killer.kill();
        let got = rx
            .recv_timeout(Duration::from_secs(1))
            .expect("the read did not return");
        reader.join().unwrap();
        assert_eq!(got, (String::new(), Some("signal: 9 (SIGKILL)".into())));
        // SAFETY: signal 0 only checks that the process exists.
        let alive = unsafe { libc::kill(killer.pid, 0) } == 0;
        assert!(!alive, "the command is still there");
        killer.kill();
    }

    // A command reaped by Drop is not killed again: its id may be
    // reused.
    #[test]
    fn drop_marks_reaped() {
        let g = sh("exec sleep 3");
        let killer = g.killer();
        assert!(!killer.reaped());
        drop(g);
        assert!(killer.reaped());
    }

    /// Reaps `pid` behind std's back, as an ignored SIGCHLD does: the
    /// next wait on it fails.
    fn reap(pid: libc::pid_t) {
        let mut status = 0;
        // SAFETY: waitpid writes the status to the int given.
        assert_eq!(unsafe { libc::waitpid(pid, &mut status, 0) }, pid);
    }

    // A wait that fails has no child either, so it is not killed later:
    // its id may be reused.
    #[test]
    fn failed_wait_marks_reaped() {
        let mut g = Group::spawn(&mut Command::new("true")).unwrap();
        let killer = g.killer();
        reap(killer.pid);
        assert!(g.wait().is_err());
        assert!(killer.reaped());
    }

    // So does Drop's poll failing.
    #[test]
    fn drop_marks_reaped_when_the_poll_fails() {
        let g = sh("exec sleep 3");
        let killer = g.killer();
        killer.kill();
        reap(killer.pid);
        drop(g);
        assert!(killer.reaped());
    }

    // Dropping after the wait is fine.
    #[test]
    fn drop_after_wait() {
        let mut g = sh("echo hi");
        assert_eq!(read_all(&mut g), "hi\n");
        assert_eq!(g.wait().unwrap(), None);
        drop(g);
    }

    // The set lets the reaped go as commands are added, and hands back
    // those before the ones superseding them.
    #[test]
    fn killers_supersede() {
        let killers = Killers::default();
        let mut done = sh("exit 0");
        done.wait().unwrap();
        let running = sh("exec sleep 3");
        killers.add(done.killer());
        assert_eq!(killers.kept(), 1);
        killers.add(running.killer());
        assert_eq!(killers.kept(), 1, "the reaped one let go");
        let newer = sh("exec sleep 3");
        let old = killers.supersede(vec![newer.killer()]);
        assert_eq!(old.len(), 1);
        assert_eq!(killers.kept(), 2);
        assert!(!killers.all_reaped());
        killers.kill_all();
        drop(running);
        drop(newer);
        assert!(killers.all_reaped());
    }
}
