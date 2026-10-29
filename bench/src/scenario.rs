use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};

use crate::input::{FILLER, marker};
use crate::pager::{self, Error, Session};

/// A pager and how to run it on a file, or on stdin when there is none.
pub struct Pager {
    pub name: &'static str,
    /// `{vedi}` is the vedi binary, `{file}` the input, `{plugins}` the
    /// plugins.
    argv: &'static [&'static str],
    /// What stands in for `{file}` to read stdin; None drops it.
    stdin: Option<&'static str>,
    /// What follows a marker once its line is drawn.
    drawn: &'static str,
    /// What a failed search prints.
    not_found: &'static str,
    /// What shows once it is ready at the last of `{n}` lines.
    at_end: &'static str,
    /// The key for the last line.
    end: &'static str,
    /// The key for the first line.
    home: &'static str,
    /// The keys that exit.
    quit: &'static str,
}

/// moor searches as each character is typed, so its search-miss time
/// covers four searches, one per character. It prints nothing on a
/// miss; the status line's hint changes after any search. ov draws a
/// tilde row past the last line.
pub static PAGERS: [Pager; 5] = [
    Pager {
        name: "vedi",
        argv: &["{vedi}", "{file}"],
        stdin: None,
        drawn: "",
        not_found: "not found:",
        at_end: "{n}+1/{n}",
        end: "G",
        home: "g",
        quit: "q",
    },
    Pager {
        name: "less",
        argv: &["less", "-R", "{file}"],
        stdin: None,
        drawn: "",
        not_found: "Pattern not found",
        at_end: "(END)",
        end: "G",
        home: "g",
        quit: "q",
    },
    Pager {
        name: "moor",
        argv: &["moor", "{file}"],
        stdin: None,
        drawn: "",
        not_found: "n/p to search",
        at_end: "100%",
        end: "G",
        home: "<",
        quit: "q",
    },
    Pager {
        name: "ov",
        argv: &["ov", "{file}"],
        stdin: None,
        drawn: "",
        not_found: "not found:",
        at_end: "~",
        end: "\x1b[F",
        home: "\x1b[H",
        quit: "q",
    },
    Pager {
        name: "vim",
        argv: &[
            "vim",
            "-N",
            "-u",
            "NONE",
            "-i",
            "NONE",
            "-n",
            "-R",
            "--cmd",
            "set rtp^={plugins}/vim-plugin-AnsiEsc ruler",
            "-c",
            "runtime! plugin/cecutil.vim plugin/AnsiEscPlugin.vim",
            "-c",
            "AnsiEsc",
            "{file}",
        ],
        stdin: Some("-"),
        drawn: FILLER.split_at(10).0,
        not_found: "Pattern not found",
        at_end: "{n},1",
        end: "G",
        home: "gg",
        quit: ":q!\r",
    },
];

impl Pager {
    pub fn needs_plugins(&self) -> bool {
        self.argv.iter().any(|a| a.contains("{plugins}"))
    }

    /// The at-end text for n lines.
    fn ready(&self, n: usize) -> String {
        self.at_end.replace("{n}", &n.to_string())
    }

    /// The argv for file, or for stdin when there is none. The plugins
    /// directory is made absolute, since vim starts in the input's
    /// directory.
    pub fn command(
        &self,
        vedi: &Path,
        file: Option<&OsStr>,
        plugins: Option<&Path>,
    ) -> Vec<OsString> {
        let plugins = plugins.map(|d| std::path::absolute(d).unwrap_or_else(|_| d.to_path_buf()));
        let plugins = plugins
            .as_deref()
            .unwrap_or(Path::new(""))
            .to_string_lossy();
        let mut argv = Vec::new();
        for a in self.argv {
            match *a {
                "{vedi}" => argv.push(vedi.into()),
                "{file}" => {
                    if let Some(f) = file.or(self.stdin.map(OsStr::new)) {
                        argv.push(f.into());
                    }
                }
                _ => argv.push(a.replace("{plugins}", &plugins).into()),
            }
        }
        argv
    }
}

/// The report's rows, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Row {
    FirstScreen,
    End,
    Home,
    SearchMiss,
    Stdin,
    SearchMissStdin,
    RssFile,
    RssStdin,
}

impl Row {
    pub const ALL: [Row; 8] = [
        Row::FirstScreen,
        Row::End,
        Row::Home,
        Row::SearchMiss,
        Row::Stdin,
        Row::SearchMissStdin,
        Row::RssFile,
        Row::RssStdin,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Row::FirstScreen => "first-screen ms",
            Row::End => "end ms",
            Row::Home => "home ms",
            Row::SearchMiss => "search-miss ms",
            Row::Stdin => "stdin ms",
            Row::SearchMissStdin => "search-miss stdin ms",
            Row::RssFile => "rss file MB",
            Row::RssStdin => "rss stdin MB",
        }
    }
}

/// The line whose marker means the first screen is up: the last of 23
/// text rows, above vedi's status line or less's prompt.
const FIRST_SCREEN: usize = 23;

pub type Cell = Result<f64, Error>;

/// One run's cells by row. Rows after a failure are absent.
pub type Sample = HashMap<Row, Cell>;

/// How the pagers are run.
pub struct Bench {
    pub vedi: PathBuf,
    pub plugins: Option<PathBuf>,
    pub timeout: Duration,
}

impl Bench {
    /// Measures p once on file, which has n lines: a session on the file
    /// for the first four rows and "rss file", and one on stdin for the
    /// rest. Pagers run in the file's directory and see its base name,
    /// so a long path does not push their status off the screen.
    pub fn run(&self, p: &Pager, file: &Path, n: usize) -> Sample {
        let mut out = Sample::new();
        // A bare name comes from PATH; a relative path would resolve in
        // the input's directory.
        let vedi = pager::look_path(self.vedi.as_os_str())
            .and_then(|v| std::path::absolute(v).ok())
            .unwrap_or_else(|| self.vedi.clone());
        // A pager that took the machine's memory leaves the file out of
        // the page cache, so the next one would read it from disk.
        if let Err(e) = warm(file) {
            out.insert(Row::FirstScreen, Err(e.into()));
            return out;
        }
        let mut run = Run {
            bench: self,
            p,
            vedi: &vedi,
            file,
            n,
            out: &mut out,
        };
        run.file();
        run.stdin();
        out
    }
}

/// Reads file through, so it is in the page cache.
fn warm(file: &Path) -> io::Result<()> {
    io::copy(&mut File::open(file)?, &mut io::sink()).map(drop)
}

/// A core idle since the last pager's exit starts the next one slowly:
/// vedi after vim measured 6.5 ms to its first screen, 5.2 after ov.
/// The spin wakes it before the clock starts.
fn wake() {
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(100) {}
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

fn mb(bytes: u64) -> f64 {
    bytes as f64 / (1 << 20) as f64
}

struct Run<'a> {
    bench: &'a Bench,
    p: &'a Pager,
    vedi: &'a Path,
    file: &'a Path,
    n: usize,
    out: &'a mut Sample,
}

impl Run<'_> {
    fn timeout(&self) -> Duration {
        self.bench.timeout
    }

    fn start(&self, file: Option<&OsStr>, stdin: Option<Stdio>) -> Result<Session, Error> {
        let argv = self
            .p
            .command(self.vedi, file, self.bench.plugins.as_deref());
        pager::start(self.file.parent(), &argv, stdin)
    }

    fn file(&mut self) {
        wake();
        let t0 = Instant::now();
        let s = self.start(self.file.file_name(), None);
        let Some(mut s) = self.check(Row::FirstScreen, s) else {
            return;
        };
        let first = s.wait(&(marker(FIRST_SCREEN) + self.p.drawn), self.timeout());
        if self.check(Row::FirstScreen, first).is_none() {
            return;
        }
        self.out.insert(Row::FirstScreen, Ok(ms(t0)));

        let t = Instant::now();
        if self
            .check(Row::End, to_end(&mut s, self.p, self.n, self.timeout()))
            .is_none()
        {
            return;
        }
        self.out.insert(Row::End, Ok(ms(t)));

        if !self.record(Row::Home, home(&mut s, self.p, self.timeout())) {
            return;
        }
        // The whole file is read now, so the search covers all of it.
        if !self.record(Row::SearchMiss, search_miss(&mut s, self.p, self.timeout())) {
            return;
        }
        let _ = s.send(self.p.quit);
        self.out
            .insert(Row::RssFile, s.finish(self.timeout()).map(mb));
    }

    /// Pipes the file into the pager and times the first screen; then
    /// reads to the end and searches all of it, from the top, so the
    /// search and the RSS are for the whole input.
    fn stdin(&mut self) {
        let piped = File::open(self.file).and_then(|f| Ok((f, io::pipe()?)));
        let Some((mut f, (r, mut w))) = self.check(Row::Stdin, piped.map_err(Error::from)) else {
            return;
        };
        thread::spawn(move || io::copy(&mut f, &mut w));
        wake();
        let t0 = Instant::now();
        let s = self.start(None, Some(r.into()));
        let Some(mut s) = self.check(Row::Stdin, s) else {
            return;
        };
        let first = s.wait(&(marker(FIRST_SCREEN) + self.p.drawn), self.timeout());
        if self.check(Row::Stdin, first).is_none() {
            return;
        }
        self.out.insert(Row::Stdin, Ok(ms(t0)));
        // The end and home are not timed again; a failure there lands on
        // the search's row.
        let c = to_end(&mut s, self.p, self.n, self.timeout())
            .and_then(|()| home(&mut s, self.p, self.timeout()))
            .and_then(|_| search_miss(&mut s, self.p, self.timeout()));
        if !self.record(Row::SearchMissStdin, c) {
            return;
        }
        let _ = s.send(self.p.quit);
        self.out
            .insert(Row::RssStdin, s.finish(self.timeout()).map(mb));
    }

    /// Puts c in row and says whether it is a measurement.
    fn record(&mut self, row: Row, c: Cell) -> bool {
        let ok = c.is_ok();
        self.out.insert(row, c);
        ok
    }

    /// The value, or None with the error in row.
    fn check<T>(&mut self, row: Row, r: Result<T, Error>) -> Option<T> {
        r.map_err(|e| self.out.insert(row, Err(e))).ok()
    }
}

/// Times the home key, from the end to the first line.
fn home(s: &mut Session, p: &Pager, timeout: Duration) -> Cell {
    let t = Instant::now();
    s.send(p.home)?;
    s.wait(&marker(1), timeout)?;
    Ok(ms(t))
}

/// Times a search for text that is not there.
fn search_miss(s: &mut Session, p: &Pager, timeout: Duration) -> Cell {
    let t = Instant::now();
    s.send("/zzzz\r")?;
    s.wait(p.not_found, timeout)?;
    Ok(ms(t))
}

/// How often to_end presses the end key again. It bounds how late the
/// last line is seen after the file is read, so it must be small next
/// to the time being measured.
pub const END_POLL: Duration = Duration::from_millis(5);

/// Presses the end key until the last of n lines shows, then waits for
/// the pager's at-end text: a pager can show the line and go on working
/// before it takes the next key. The end key jumps to the end of what a
/// pager has read so far, so it is pressed again until the file is
/// read.
pub fn to_end(s: &mut Session, p: &Pager, n: usize, timeout: Duration) -> Result<(), Error> {
    let deadline = Instant::now() + timeout;
    loop {
        s.send(p.end)?;
        match s.wait(&marker(n), END_POLL) {
            Ok(()) => break,
            Err(Error::Timeout) if Instant::now() < deadline => {}
            Err(e) => return Err(e),
        }
    }
    s.wait(
        &p.ready(n),
        deadline.saturating_duration_since(Instant::now()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TempDir;
    use crate::input::write_input;

    /// vedi built in the test's profile, by a path relative to the
    /// test's directory, the package's, which is neither the binary's
    /// nor the input's. Not on PATH: changing the environment would
    /// race the other tests.
    fn build_vedi() -> PathBuf {
        let exe = std::env::current_exe().unwrap();
        let profile = exe.parent().and_then(Path::parent).unwrap();
        let mut build = std::process::Command::new(env!("CARGO"));
        build.args(["build", "-q", "-p", "vedi"]);
        if profile.file_name() == Some(OsStr::new("release")) {
            build.arg("--release");
        }
        let status = build.status().unwrap();
        assert!(status.success(), "build: {status}");
        let vedi = profile.join("vedi");
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let root = root.canonicalize().unwrap();
        match vedi.canonicalize().unwrap().strip_prefix(&root) {
            Ok(rel) => Path::new("..").join(rel),
            Err(_) => vedi,
        }
    }

    /// Every row is measured on vedi built from this tree, on the other
    /// pagers that are installed, and on vim when PLUGINS names the
    /// directory of its plugin's checkout.
    #[test]
    fn run() {
        let plugins = std::env::var_os("PLUGINS").map(PathBuf::from);
        let vedi = build_vedi();
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("in");
        const N: usize = 200;
        write_input(&file, N).unwrap();
        for p in &PAGERS {
            if p.needs_plugins() && plugins.is_none() {
                eprintln!("skipping {}: PLUGINS is not set", p.name);
                continue;
            }
            let bench = Bench {
                vedi: vedi.clone(),
                plugins: plugins.clone(),
                timeout: Duration::from_secs(20),
            };
            let argv0 = p
                .command(&bench.vedi, Some(OsStr::new("in")), None)
                .remove(0);
            if pager::look_path(&argv0).is_none() {
                eprintln!("skipping {}: not on PATH", p.name);
                continue;
            }
            let got = bench.run(p, &file, N);
            for row in Row::ALL {
                match got.get(&row) {
                    Some(Ok(v)) if *v > 0.0 => {}
                    c => panic!("{} {} = {c:?}", p.name, row.label()),
                }
            }
        }
    }

    /// Re-pressing the end key adds at most END_POLL to "end ms", so it
    /// must be small next to the hundreds of ms a pager takes to read
    /// the file.
    #[test]
    fn end_poll() {
        assert!(END_POLL <= Duration::from_millis(10), "{END_POLL:?}");
    }

    /// to_end waits for the pager's at-end text, not just the last line,
    /// since a pager can show the line and go on working.
    #[test]
    fn to_end_ready() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("in");
        write_input(&file, 3).unwrap();
        let argv: Vec<OsString> = ["sh", "-c", r#"cat "$1"; sleep 0.1; echo READY"#, "sh"]
            .into_iter()
            .map(OsString::from)
            .chain([file.into()])
            .collect();
        let mut s = pager::start(None, &argv, None).unwrap();
        let p = Pager {
            at_end: "READY",
            end: "G",
            ..PAGERS[0]
        };
        let t0 = Instant::now();
        to_end(&mut s, &p, 3, Duration::from_secs(5)).unwrap();
        let d = t0.elapsed();
        assert!(
            d >= Duration::from_millis(100),
            "to_end returned after {d:?}, before READY"
        );
        drop(s);
    }

    /// A relative plugins directory is made absolute, since vim starts
    /// in the input's directory.
    #[test]
    fn command_plugins() {
        let want = std::env::current_dir().unwrap().join("rel");
        let want = want.to_str().unwrap();
        for p in PAGERS.iter().filter(|p| p.needs_plugins()) {
            let argv = p.command(
                Path::new("vedi"),
                Some(OsStr::new("in")),
                Some(Path::new("rel")),
            );
            let argv: Vec<&str> = argv.iter().map(|a| a.to_str().unwrap()).collect();
            for a in &argv {
                assert!(
                    !a.contains("{plugins}") && !a.contains("rtp^=rel"),
                    "{}: {a:?} not resolved",
                    p.name
                );
            }
            assert!(
                argv.iter().any(|a| a.contains(want)),
                "{}: {argv:?} lacks {want:?}",
                p.name
            );
        }
    }

    /// Without a file, a pager reads stdin by its own name for it, or by
    /// none.
    #[test]
    fn command_stdin() {
        let vedi = PAGERS[0].command(Path::new("/v"), None, None);
        assert_eq!(vedi, [OsStr::new("/v")]);
        let vim = PAGERS[4].command(Path::new("/v"), None, None);
        assert_eq!(vim.last().map(OsString::as_os_str), Some(OsStr::new("-")));
    }
}
