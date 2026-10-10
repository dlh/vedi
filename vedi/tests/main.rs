//! The binary, run on temp files. Each run is in a session of its own,
//! with no controlling terminal, so it can never page on the one
//! running the tests, and runs the same without one.
#![allow(unsafe_code)]

use std::fs;
use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use vedi::cli::{USAGE, completion};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> TempDir {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("vedi-main-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn file(&self, name: &str, data: &str) -> String {
        let p = self.0.join(name);
        fs::write(&p, data).unwrap();
        p.to_str().unwrap().to_string()
    }

    fn path(&self, name: &str) -> String {
        self.0.join(name).to_str().unwrap().to_string()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// vedi in a session of its own, so /dev/tty cannot open: a test never
/// pages on the terminal running it. The config is looked for in
/// `home`, where there is none.
fn vedi(home: &Path, args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_vedi"));
    c.args(args)
        .env_remove("VEDI")
        .env_remove("COLUMNS")
        .env_remove("LINES")
        .env("XDG_CONFIG_HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // SAFETY: setsid is async-signal-safe and touches no memory.
    unsafe {
        c.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    c
}

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

/// Waits for `child`, killing it and failing after 10 s; `stdin` is
/// held open until then.
fn wait(mut child: Child, stdin: Option<ChildStdin>) -> Out {
    let read = |r: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut s = Vec::new();
            if let Some(mut r) = r {
                r.read_to_end(&mut s).unwrap();
            }
            String::from_utf8_lossy(&s).into_owned()
        })
    };
    let out = read(child.stdout.take().map(|r| Box::new(r) as _));
    let err = read(child.stderr.take().map(|r| Box::new(r) as _));
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("vedi hung");
        }
        thread::sleep(Duration::from_millis(10));
    };
    drop(stdin);
    Out {
        code: status.code().unwrap_or(-1),
        stdout: out.join().unwrap(),
        stderr: err.join().unwrap(),
    }
}

fn run(mut c: Command, input: Option<&str>) -> Out {
    if input.is_some() {
        c.stdin(Stdio::piped());
    }
    let mut child = c.spawn().unwrap();
    if let Some(input) = input {
        let mut w = child.stdin.take().unwrap();
        let input = input.to_string();
        thread::spawn(move || {
            let _ = w.write_all(input.as_bytes());
        });
    }
    wait(child, None)
}

fn printed(out: &Out) -> &str {
    assert_eq!((out.code, out.stderr.as_str()), (0, ""));
    &out.stdout
}

#[test]
fn version() {
    let d = TempDir::new();
    let out = run(vedi(&d.0, &["-v"]), None);
    assert_eq!(printed(&out), format!("vedi {}\n", vedi::VERSION));
}

#[test]
fn help() {
    let d = TempDir::new();
    let out = run(vedi(&d.0, &["--help"]), None);
    assert_eq!(printed(&out), USAGE);
}

#[test]
fn completion_bash() {
    let d = TempDir::new();
    let out = run(vedi(&d.0, &["--completion", "bash"]), None);
    assert_eq!(printed(&out), completion("bash").unwrap());
}

#[test]
fn bad_flag_errors_with_usage() {
    let d = TempDir::new();
    let out = run(vedi(&d.0, &["--no-such-flag"]), None);
    assert_eq!(out.code, 1);
    assert!(out.stderr.starts_with("vedi: "), "{}", out.stderr);
    assert!(
        out.stderr.ends_with(&format!("\n{USAGE}")),
        "{}",
        out.stderr
    );
}

// Usage only when no file was named, as Go's main did.
#[test]
fn missing_file_errors() {
    let d = TempDir::new();
    let p = d.path("missing");
    let out = run(vedi(&d.0, &[&p]), None);
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stderr,
        format!("vedi: open {p}: no such file or directory\n")
    );
}

#[test]
fn no_input_on_a_terminal_errors_with_usage() {
    let d = TempDir::new();
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
    let (parent, child) = unsafe {
        use std::os::fd::FromRawFd;
        (fs::File::from_raw_fd(parent), fs::File::from_raw_fd(child))
    };
    let mut c = vedi(&d.0, &[]);
    c.stdin(child);
    let out = wait(c.spawn().unwrap(), None);
    drop(parent);
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stderr,
        format!("vedi: no input: give a file or pipe something in\n{USAGE}")
    );
}

#[test]
fn no_input_from_dev_null_errors_with_usage() {
    let d = TempDir::new();
    let out = run(vedi(&d.0, &[]), None);
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stderr,
        format!("vedi: no input: give a file or pipe something in\n{USAGE}")
    );
}

// The keyboard query goes to the terminal, never to stdout.
#[test]
fn terminal_queries_skip_stdout() {
    let d = TempDir::new();
    let f = d.file("f", "hello\n");
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
    let (parent, child) = unsafe {
        use std::os::fd::FromRawFd;
        (fs::File::from_raw_fd(parent), fs::File::from_raw_fd(child))
    };
    let mut c = vedi(&d.0, &[&f]);
    let tty = {
        use std::os::fd::AsRawFd;
        child.as_raw_fd()
    };
    // SAFETY: ioctl is async-signal-safe and touches no memory; this
    // runs after vedi's setsid, so the pty becomes the controlling tty.
    unsafe {
        c.pre_exec(move || {
            libc::ioctl(tty, libc::TIOCSCTTY as _, 0);
            Ok(())
        });
    }
    let mut w = parent.try_clone().unwrap();
    let mut r = parent;
    thread::spawn(move || {
        let mut seen = Vec::new();
        let mut buf = [0; 4096];
        let (mut answered, mut quit) = (false, false);
        while let Ok(n @ 1..) = r.read(&mut buf) {
            seen.extend_from_slice(&buf[..n]);
            if !answered && seen.windows(3).any(|b| b == b"\x1b[c") {
                answered = true;
                let _ = w.write_all(b"\x1b[?62c");
            }
            if !quit && seen.windows(8).any(|b| b == b"\x1b[?1049h") {
                quit = true;
                let _ = w.write_all(b"q");
            }
        }
    });
    let out = wait(c.spawn().unwrap(), None);
    drop(child);
    assert_eq!((out.code, out.stdout.as_str()), (0, ""), "{}", out.stderr);
}

#[test]
fn missing_config_errors() {
    let d = TempDir::new();
    let f = d.file("f", "hello\n");
    let p = d.path("missing.conf");
    let out = run(vedi(&d.0, &["--config", &p, &f]), None);
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stderr,
        format!("vedi: open {p}: no such file or directory\n")
    );
}

#[test]
fn one_page_prints_colors() {
    let d = TempDir::new();
    let text = "\x1b[31mred\x1b[0m\nplain\n";
    let out = run(vedi(&d.0, &["-F"]), Some(text));
    assert_eq!(printed(&out), text);
}

// A paged file, its bytes read back from disk to print.
#[test]
fn one_page_prints_files() {
    let d = TempDir::new();
    let a = d.file("a", "a\n");
    let b = d.file("b", "\x1b[1mb\nc\x1b[m\n");
    let out = run(vedi(&d.0, &["-F", &a, &b]), None);
    assert_eq!(printed(&out), "a\n\x1b[1mb\nc\x1b[m\n");
}

// A name that is not UTF-8 opens the file it names. APFS refuses
// such a name, so only Linux can try.
#[cfg(target_os = "linux")]
#[test]
fn file_name_not_utf8() {
    use std::os::unix::ffi::OsStringExt;
    let d = TempDir::new();
    let name = d.0.join(std::ffi::OsString::from_vec(vec![b'f', 0xff]));
    fs::write(&name, "hello\n").unwrap();
    let mut c = vedi(&d.0, &["-F"]);
    c.arg(&name);
    let out = run(c, None);
    assert_eq!(printed(&out), "hello\n");
}

// No other escape or control reaches the terminal, a carriage return
// inside a line is spelled out, and a style or link left on is ended.
#[test]
fn one_page_keeps_only_colors_and_links() {
    let d = TempDir::new();
    let text = "\x1b]2;hunk\x07@@ \x1b[31mone\x1b[0m\r\nplain\n\x1b]0;t\x1b\\\n\x1b]8;;http://x\x07link\x1b]8;;\x07\n\
                \x1b]52;c;aGk=\x07ok\x1b[2J\rfake\n\x1b[1m\x1b]8;;http://y\x07end\x1b]2;t\x07";
    let want = "@@ \x1b[31mone\x1b[0m\r\nplain\n\n\x1b]8;;http://x\x07link\x1b]8;;\x07\n\
                ok^Mfake\n\x1b[1m\x1b]8;;http://y\x07end\x1b[m\x1b]8;;\x1b\\";
    let out = run(vedi(&d.0, &["-F"]), Some(text));
    assert_eq!(printed(&out), want);
}

#[test]
fn one_page_bare_carriage_return() {
    let d = TempDir::new();
    let out = run(vedi(&d.0, &["-F"]), Some("one\r\nvisible\r"));
    assert_eq!(printed(&out), "one\r\nvisible^M");
}

#[test]
fn one_page_plain_and_raw() {
    let d = TempDir::new();
    let text = "\x1b[31mone\x1b[0m\r\ntwo\n";
    let out = run(vedi(&d.0, &["-F", "--view-style", "plain"]), Some(text));
    assert_eq!(printed(&out), "one\r\ntwo\n");
    let out = run(vedi(&d.0, &["-F", "--view-style", "raw"]), Some(text));
    assert_eq!(printed(&out), "^[[31mone^[[0m\r\ntwo\n");
}

// Text that outgrows the screen is paged as soon as it does, the input
// still open; with no terminal, paging fails.
#[test]
fn one_page_pages_before_eof() {
    let d = TempDir::new();
    let mut c = vedi(&d.0, &["-F"]);
    c.env("LINES", "3")
        .env("COLUMNS", "40")
        .stdin(Stdio::piped());
    let mut child = c.spawn().unwrap();
    let mut w = child.stdin.take().unwrap();
    w.write_all(b"1\n2\n3\n").unwrap();
    let out = wait(child, Some(w));
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.starts_with("vedi: open /dev/tty: "),
        "{}",
        out.stderr
    );
}

#[test]
fn open_cmd_by_name() {
    let d = TempDir::new();
    let conf = d.file("vedi.conf", "open_cmd *.md sed s/l/L/g\n");
    let a = d.file("a.md", "hello\n");
    let b = d.file("b.txt", "hello\n");
    let out = run(vedi(&d.0, &["--config", &conf, "-F", &a, &b]), None);
    assert_eq!(printed(&out), "heLLo\nhello\n");
}

#[test]
fn open_cmd_leaves_stdin() {
    let d = TempDir::new();
    let out = run(
        vedi(&d.0, &["--open-cmd", "cat -n %s", "-F"]),
        Some("hello\n"),
    );
    assert_eq!(printed(&out), "hello\n");
}

/// A named pipe at `name` that "hello\n" is written to once opened.
fn fifo(d: &TempDir, name: &str) -> String {
    let p = d.path(name);
    assert!(Command::new("mkfifo").arg(&p).status().unwrap().success());
    let w = p.clone();
    thread::spawn(move || {
        let _ = fs::write(w, "hello\n");
    });
    p
}

#[test]
fn one_page_reads_a_fifo() {
    let d = TempDir::new();
    let p = fifo(&d, "pipe");
    let out = run(vedi(&d.0, &["-F", &p]), None);
    assert_eq!(printed(&out), "hello\n");
}

// Left for the command to open: opened twice, the second reader would
// wait for a writer that has been and gone.
#[test]
fn open_cmd_reads_a_fifo() {
    let d = TempDir::new();
    let p = fifo(&d, "pipe");
    let out = run(vedi(&d.0, &["--open-cmd", "cat %s", "-F", &p]), None);
    assert_eq!(printed(&out), "hello\n");
}

// The missing file is the error, before any command runs.
#[test]
fn open_cmd_missing_file() {
    let d = TempDir::new();
    let p = d.path("missing");
    let marker = d.path("ran");
    let cmd = format!("touch {marker}");
    let out = run(vedi(&d.0, &["--open-cmd", &cmd, &p]), None);
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stderr,
        format!("vedi: stat {p}: no such file or directory\n")
    );
    assert!(!Path::new(&marker).exists());
}

#[test]
fn open_cmd_not_found() {
    let d = TempDir::new();
    let f = d.file("f", "");
    let out = run(
        vedi(&d.0, &["--open-cmd", "vedi-no-such-command %s", &f]),
        None,
    );
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stderr,
        "vedi: exec: \"vedi-no-such-command\": executable file not found in $PATH\n"
    );
}
