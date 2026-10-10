//! Sends text to the system clipboard.

use std::io::Write;
use std::process::{Command, Stdio};

use super::{failure, scratch};
use crate::app::Copier;

/// The copier for the options: `cmd` when given; else OSC 52 on `raw`,
/// the terminal, but on Terminal.app, which ignores it, `pbcopy`.
pub fn pick(
    cmd: Option<String>,
    term_program: &str,
    term: &str,
    raw: impl Write + Send + 'static,
) -> Box<dyn Copier> {
    match command(cmd, term_program) {
        Some(cmd) => Box::new(CommandCopier::new(cmd)),
        None => Box::new(Osc52::new(raw, term)),
    }
}

fn command(cmd: Option<String>, term_program: &str) -> Option<String> {
    cmd.or_else(|| (term_program == "Apple_Terminal").then(|| "pbcopy".to_string()))
}

/// Asks the terminal to set the clipboard, with no acknowledgement.
pub struct Osc52<W: Write> {
    /// `None` on a legacy terminal, which is sent nothing, as tcell
    /// sends it nothing.
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

impl<W: Write> Copier for Osc52<W> {
    fn copy(&mut self, text: &str) -> Result<(), String> {
        let Some(out) = &mut self.out else {
            return Ok(());
        };
        let seq = format!("\x1b]52;c;{}\x1b\\", base64(text.as_bytes()));
        out.write_all(seq.as_bytes())
            .and_then(|()| out.flush())
            .map_err(|e| e.to_string())
    }
}

fn base64(data: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | (c.get(1).copied().unwrap_or(0) as u32) << 8
            | c.get(2).copied().unwrap_or(0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

/// Pipes text to a shell command's stdin. A failure's error carries the
/// exit status and what the command wrote to stderr.
pub struct CommandCopier {
    cmd: String,
}

impl CommandCopier {
    pub fn new(cmd: impl Into<String>) -> CommandCopier {
        CommandCopier { cmd: cmd.into() }
    }
}

impl Copier for CommandCopier {
    fn copy(&mut self, text: &str) -> Result<(), String> {
        // stderr goes to a file, not a pipe: a tool that forks a daemon
        // to serve the selection (xclip) leaves it holding a pipe, and
        // waiting would not return until the selection is lost.
        let mut stderr = scratch();
        let mut cmd = Command::new("sh");
        cmd.args(["-c", &self.cmd])
            .stdin(Stdio::piped())
            .stdout(Stdio::null());
        match &stderr {
            Some(f) => cmd.stderr(f.try_clone().map_err(|e| e.to_string())?),
            None => cmd.stderr(Stdio::null()),
        };
        let mut child = cmd.spawn().map_err(|e| e.to_string())?;
        // A command that exits without reading closes the pipe; that
        // is its exit status's to report.
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let status = child.wait().map_err(|e| e.to_string())?;
        if status.success() {
            return Ok(());
        }
        Err(failure(status, stderr.as_mut()))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;
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

    #[test]
    fn osc52_sends_base64() {
        let mut c = Osc52::new(Vec::new(), "xterm-256color");
        assert_eq!(c.copy("héllo\nworld"), Ok(()));
        assert_eq!(c.out.unwrap(), b"\x1b]52;c;aMOpbGxvCndvcmxk\x1b\\");
    }

    #[test]
    fn base64_pads() {
        let tests = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
        ];
        for (text, want) in tests {
            assert_eq!(base64(text.as_bytes()), want, "{text:?}");
        }
    }

    // tcell sends no OSC 52 to a legacy terminal.
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
            assert_eq!(Osc52::new(out.clone(), term).copy("x"), Ok(()));
            assert_eq!(out.text(), "", "{term}");
        }
        for term in ["xterm", "linux-16color", "sun-cmd", "screen"] {
            let out = Shared::default();
            assert_eq!(Osc52::new(out.clone(), term).copy("x"), Ok(()));
            assert_ne!(out.text(), "", "{term}");
        }
    }

    #[test]
    fn command_pipes_text() {
        let dir = std::env::temp_dir().join(format!("vedi-clip-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let out = dir.join("clip");
        let res = CommandCopier::new(format!("cat > '{}'", out.display())).copy("a b\nc");
        let data = fs::read_to_string(&out);
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(res, Ok(()));
        assert_eq!(data.unwrap(), "a b\nc");
    }

    #[test]
    fn command_reports_status_and_stderr() {
        assert_eq!(CommandCopier::new("cat >/dev/null").copy("hi"), Ok(()));
        assert_eq!(
            CommandCopier::new("false").copy("hi"),
            Err("exit status 1".to_string())
        );
        assert_eq!(
            CommandCopier::new("echo nope >&2; exit 3").copy("hi"),
            Err("exit status 3: nope".to_string())
        );
        // A byte that is not UTF-8 loses nothing but itself.
        assert_eq!(
            CommandCopier::new("printf 'bad \\377 byte' >&2; exit 3").copy("hi"),
            Err("exit status 3: bad \u{fffd} byte".to_string())
        );
    }

    // A tool that forks a daemon to serve the selection (xclip) leaves
    // a child holding stdout and stderr; copy must return when the
    // command exits, not when the child does.
    #[test]
    fn command_returns_while_child_holds_stderr() {
        let start = Instant::now();
        assert_eq!(CommandCopier::new("sleep 5 & exit 0").copy("x"), Ok(()));
        assert!(start.elapsed() < Duration::from_secs(2));
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
        assert_eq!(pick(None, "", "xterm", out.clone()).copy("hi"), Ok(()));
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
        );
        assert_eq!(c.copy("hi"), Err("exit status 4".to_string()));
        assert_eq!(out.text(), "");
    }
}
