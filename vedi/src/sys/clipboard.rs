//! Sends text to the system clipboard.

use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::proc::{Group, Killer, Killers};
use crate::app::{Copier, CopyResult, Done};

/// The copier for the options: `cmd` when given, its runs added to
/// `killers`; else OSC 52 on `raw`, the terminal, but on Terminal.app,
/// which ignores it, `pbcopy`.
pub fn pick(
    cmd: Option<String>,
    term_program: &str,
    term: &str,
    raw: impl Write + Send + 'static,
    killers: Killers,
) -> Box<dyn Copier> {
    match command(cmd, term_program) {
        Some(cmd) => Box::new(CommandCopier::new(cmd, killers)),
        None => Box::new(Osc52::new(raw, term)),
    }
}

fn command(cmd: Option<String>, term_program: &str) -> Option<String> {
    cmd.or_else(|| (term_program == "Apple_Terminal").then(|| "pbcopy".to_string()))
}

/// Asks the terminal to set the clipboard, with no acknowledgement.
pub struct Osc52<W: Write> {
    /// `None` on a legacy terminal, which has no clipboard to set and
    /// may show the sequence instead.
    out: Option<W>,
}

impl<W: Write> Osc52<W> {
    pub fn new(out: W, term: &str) -> Osc52<W> {
        let legacy = term.starts_with("vt")
            || term.contains("ansi")
            || matches!(term, "linux" | "sun" | "sun-color");
        Osc52 {
            out: (!legacy).then_some(out),
        }
    }
}

impl<W: Write> Osc52<W> {
    fn send(&mut self, text: &str) -> CopyResult {
        let Some(out) = &mut self.out else {
            return Ok(());
        };
        let seq = format!("\x1b]52;c;{}\x1b\\", STANDARD.encode(text));
        out.write_all(seq.as_bytes())
            .and_then(|()| out.flush())
            .map_err(|e| e.to_string())
    }
}

impl<W: Write> Copier for Osc52<W> {
    fn copy(&mut self, text: &str, done: Done) -> Option<Killer> {
        done(self.send(text));
        None
    }
}

/// Pipes text to a shell command's stdin, on a thread: a command that
/// hangs, as one prompting on the tty under the alternate screen does,
/// must not hold the keys. One still running at exit is killed through
/// `killers`, with what it started; a tool that forks a daemon to
/// serve the selection (xclip) has exited by then, so the daemon
/// stays. A failure's error carries the exit status and what the
/// command wrote to stderr.
pub struct CommandCopier {
    cmd: String,
    killers: Killers,
}

impl CommandCopier {
    pub fn new(cmd: impl Into<String>, killers: Killers) -> CommandCopier {
        CommandCopier {
            cmd: cmd.into(),
            killers,
        }
    }
}

impl Copier for CommandCopier {
    fn copy(&mut self, text: &str, done: Done) -> Option<Killer> {
        let mut sh = Command::new("sh");
        sh.args(["-c", &self.cmd])
            .stdin(Stdio::piped())
            .stdout(Stdio::null());
        let mut run = match Group::spawn(&mut sh) {
            Ok(run) => run,
            Err(e) => {
                done(Err(e.to_string()));
                return None;
            }
        };
        let killer = run.killer();
        self.killers.add(killer.clone());
        let text = text.to_string();
        thread::spawn(move || done(finish(&mut run, &text)));
        Some(killer)
    }
}

/// Sends `text` to `run` and waits for it.
fn finish(run: &mut Group, text: &str) -> CopyResult {
    // A command that exits without reading closes the pipe; that is
    // its exit status's to report.
    if let Some(mut stdin) = run.stdin() {
        let _ = stdin.write_all(text.as_bytes());
    }
    run.wait().map_err(|e| e.to_string())?.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use super::*;

    #[derive(Clone, Default)]
    struct Shared(Arc<Mutex<Vec<u8>>>);

    impl Write for Shared {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Shared {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    fn cmd(c: impl Into<String>) -> CommandCopier {
        CommandCopier::new(c, Killers::default())
    }

    /// `c`'s result for `text`, waited for.
    fn copy(c: &mut dyn Copier, text: &str) -> CopyResult {
        let (tx, rx) = mpsc::channel();
        c.copy(text, Box::new(move |r| tx.send(r).unwrap()));
        rx.recv().unwrap()
    }

    #[test]
    fn osc52_sends_base64() {
        let mut c = Osc52::new(Vec::new(), "xterm-256color");
        assert_eq!(copy(&mut c, "héllo\nworld"), Ok(()));
        assert_eq!(c.out.unwrap(), b"\x1b]52;c;aMOpbGxvCndvcmxk\x1b\\");
    }

    // Nothing to a legacy terminal, which has no clipboard to set.
    #[test]
    fn osc52_legacy_terminal_is_a_no_op() {
        for term in [
            "vt100",
            "vt220",
            "ansi",
            "pcansi",
            "linux",
            "sun",
            "sun-color",
        ] {
            let out = Shared::default();
            assert_eq!(copy(&mut Osc52::new(out.clone(), term), "x"), Ok(()));
            assert_eq!(out.text(), "", "{term}");
        }
        for term in ["xterm", "linux-16color", "sun-cmd", "screen"] {
            let out = Shared::default();
            assert_eq!(copy(&mut Osc52::new(out.clone(), term), "x"), Ok(()));
            assert_ne!(out.text(), "", "{term}");
        }
    }

    #[test]
    fn command_pipes_text() {
        let dir = std::env::temp_dir().join(format!("vedi-clip-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let out = dir.join("clip");
        let res = copy(&mut cmd(format!("cat > '{}'", out.display())), "a b\nc");
        let data = fs::read_to_string(&out);
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(res, Ok(()));
        assert_eq!(data.unwrap(), "a b\nc");
    }

    #[test]
    fn command_reports_status_and_stderr() {
        assert_eq!(copy(&mut cmd("cat >/dev/null"), "hi"), Ok(()));
        assert_eq!(
            copy(&mut cmd("false"), "hi"),
            Err("exit status: 1".to_string())
        );
        assert_eq!(
            copy(&mut cmd("echo nope >&2; exit 3"), "hi"),
            Err("exit status: 3: nope".to_string())
        );
        // A byte that is not UTF-8 loses nothing but itself.
        assert_eq!(
            copy(&mut cmd("printf 'bad \\377 byte' >&2; exit 3"), "hi"),
            Err("exit status: 3: bad \u{fffd} byte".to_string())
        );
    }

    // A tool that forks a daemon to serve the selection (xclip) leaves
    // a child holding stdout and stderr; copy must return when the
    // command exits, not when the child does.
    #[test]
    fn command_returns_while_child_holds_stderr() {
        let start = Instant::now();
        assert_eq!(copy(&mut cmd("sleep 5 & exit 0"), "x"), Ok(()));
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    // A command still running at exit is killed, and its copy fails.
    // One that has exited is let go, so a daemon it left stays.
    #[test]
    fn command_running_at_exit_is_killed() {
        let killers = Killers::default();
        let (tx, rx) = mpsc::channel();
        CommandCopier::new("sleep 5", killers.clone())
            .copy("x", Box::new(move |r| tx.send(r).unwrap()));
        killers.kill_all();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            Err("signal: 9 (SIGKILL)".to_string())
        );
        assert_eq!(
            copy(&mut CommandCopier::new("exit 0", killers.clone()), "x"),
            Ok(())
        );
        killers.kill_all();
        assert!(killers.all_reaped());
    }

    // The command runs on a thread: copy returns while it does, and the
    // result follows.
    #[test]
    fn command_does_not_hold_the_caller() {
        let start = Instant::now();
        let (tx, rx) = mpsc::channel();
        cmd("sleep 1").copy("x", Box::new(move |r| tx.send(r).unwrap()));
        assert!(start.elapsed() < Duration::from_millis(500));
        assert_eq!(rx.recv().unwrap(), Ok(()));
        assert!(start.elapsed() >= Duration::from_secs(1));
    }

    // The killer handed back ends the command, for a copy superseded.
    #[test]
    fn copy_hands_back_the_commands_killer() {
        let (tx, rx) = mpsc::channel();
        let killer = cmd("sleep 5")
            .copy("x", Box::new(move |r| tx.send(r).unwrap()))
            .expect("a command has a killer");
        killer.kill();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            Err("signal: 9 (SIGKILL)".to_string())
        );
    }

    // Terminal.app ignores OSC 52, so with no command it gets pbcopy.
    #[test]
    fn pick_command() {
        assert_eq!(command(None, "Apple_Terminal"), Some("pbcopy".to_string()));
        assert_eq!(command(None, "iTerm.app"), None);
        assert_eq!(command(None, ""), None);
        assert_eq!(
            command(Some("wl-copy".into()), "Apple_Terminal"),
            Some("wl-copy".to_string())
        );
    }

    #[test]
    fn pick_defaults_to_osc52() {
        let out = Shared::default();
        let mut c = pick(None, "", "xterm", out.clone(), Killers::default());
        assert_eq!(copy(c.as_mut(), "hi"), Ok(()));
        assert_eq!(out.text(), "\x1b]52;c;aGk=\x1b\\");
    }

    #[test]
    fn pick_runs_a_command_given() {
        let out = Shared::default();
        let mut c = pick(
            Some("exit 4".into()),
            "Apple_Terminal",
            "xterm",
            out.clone(),
            Killers::default(),
        );
        assert_eq!(copy(c.as_mut(), "hi"), Err("exit status: 4".to_string()));
        assert_eq!(out.text(), "");
    }
}
