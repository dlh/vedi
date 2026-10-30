//! Reads an input by running a command on it.

use std::ffi::{OsStr, OsString};
use std::io::{self, Read};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::process::{ChildStdout, Command, Stdio};

use super::proc::{Group, Killer};
use crate::text::buffer::ReadError;
use crate::text::source::Source;

/// A running command's output. Its failure, with its stderr, is the
/// read error once the output ends.
pub struct Cmd {
    group: Group,
    out: ChildStdout,
    state: State,
}

enum State {
    Running,
    /// Reaped. The error is `read_error`'s, until taken.
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
    // A name starting with - would be an option to the command.
    let dotted;
    let name = if name.as_bytes().starts_with(b"-") {
        dotted = subst("./%s", name);
        dotted.as_os_str()
    } else {
        name
    };
    let mut args: Vec<OsString> = argv.iter().map(|a| subst(a, name)).collect();
    if !argv.iter().any(|a| a.contains("%s")) {
        args.push(name.to_os_string());
    }
    let Some((prog, rest)) = args.split_first() else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "no command"));
    };
    let mut cmd = Command::new(prog);
    cmd.args(rest).stdin(Stdio::null()).stdout(Stdio::piped());
    let mut group = Group::spawn(&mut cmd)?;
    let out = group.stdout().expect("stdout piped");
    Ok(Cmd {
        group,
        out,
        state: State::Running,
    })
}

impl Cmd {
    pub fn killer(&self) -> Killer {
        self.group.killer()
    }
}

impl Source for Cmd {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.out.read(buf)?;
        if n == 0 && !buf.is_empty() && matches!(self.state, State::Running) {
            self.state = State::Reaped(self.group.wait()?.map(ReadError::Command));
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

    // A name that starts with - reaches the command as a path, not an
    // option, whether substituted or appended.
    #[test]
    fn open_dash_name() {
        let mut c = open(&args(&["echo", "%s"]), OsStr::new("-n")).unwrap();
        assert_eq!(read_all(&mut c), ("./-n\n".into(), None));
        let mut c = open(&args(&["echo"]), OsStr::new("--version")).unwrap();
        assert_eq!(read_all(&mut c), ("./--version\n".into(), None));
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
            ("partial\n".into(), Some("exit status: 3: oops".into()))
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
}
