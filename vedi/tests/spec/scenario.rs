//! One scenario: its setup, the pager it opens, and the sections run
//! against it.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{self, ErrorKind, Write};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;
use std::time::Instant;

use vedi::app::{
    App, Copier, Done, Event, Flow, Mouse, MouseKind, Opener, Options, Runtime, Sink, Verdict,
    Wake, one_page,
};
use vedi::cli;
use vedi::input::config::{self, Config};
use vedi::input::keymap::Keymap;
use vedi::sys::clipboard::CommandCopier;
use vedi::sys::proc::{Killer, Killers};
use vedi::text::ansi::ViewStyle;
use vedi::text::buffer::{Buffer, Progress, Storage};
use vedi::text::index::Indexer;
use vedi::text::reader::Gen;
use vedi::view::screen::TestTerminal;

use crate::spec::archive::Archive;
use crate::spec::keys::parse_keys;
use crate::spec::mouse::{Act, Dir, MouseAction, parse_mouse};
use crate::spec::screen::{render, side_by_side};
use crate::spec::step::Stepper;

/// The sections that drive the app, so none may follow a quit.
pub const ACTIONS: &[&str] = &[
    "input",
    "next-file",
    "eof",
    "file",
    "reload",
    "keys",
    "paste",
    "mouse",
    "resize",
];

/// The actions that need the pager open first; the others, input and
/// eof, may be what opens it.
pub const DRIVES: &[&str] = &["keys", "paste", "mouse", "resize", "file", "reload"];

/// The startup reader's generation.
const GEN: Gen = 1;

/// How long a `tick` lets pass: the app's auto-scroll interval.
const TICK: Duration = Duration::from_millis(50);

/// A file on disk: its text, and whether it ended with a newline.
#[derive(Clone, Default)]
struct Disk {
    text: String,
    nl: bool,
}

/// What `start` opens the pager with.
struct Startup {
    opts: Options,
    files: Vec<String>,
    /// The args name files, so the app has an opener.
    file: bool,
    /// -F: the rows to leave below the text, when the text fits.
    rows_below: Option<usize>,
}

struct Scenario<'s, 'a> {
    w: usize,
    h: usize,
    args: Vec<String>,
    /// $VEDI.
    env: String,
    macos: bool,
    config: Config,
    /// The buffer, until the app takes it.
    buf: Option<Buffer>,
    ix: Indexer,
    /// The last input section ended with a newline.
    nl: bool,
    /// The args name files, so the app has an opener.
    file: bool,
    /// -F: the pager opens only once the text is known not to fit; the
    /// rows to leave below it.
    rows_below: Option<usize>,
    /// A row names each input, for -F's measure.
    seps: bool,
    /// What each file holds: the input, then each file or reload body.
    disks: Rc<RefCell<Vec<Disk>>>,
    /// What open_cmd prints for each file: the input sections, as the
    /// harness runs no command.
    rendered: Rc<RefCell<Vec<Disk>>>,
    /// The file has an eof section, so input stays open.
    has_eof: bool,
    /// The input in -- args -- the input now comes from.
    part: usize,
    /// An eof section has run.
    finished: bool,
    /// The setup is frozen: an action or assertion has run.
    started: bool,
    files: Vec<String>,
    keys: Keymap,
    app: Option<App<TestTerminal>>,
    /// The options `start` built, until `open` takes them.
    pending_opts: Option<Options>,
    /// What the recording copier was last given.
    clip: Rc<RefCell<Option<String>>>,
    /// What :write wrote, by file name.
    written: Rc<RefCell<BTreeMap<String, Vec<u8>>>>,
    /// Wakes the app for a copy's result, and where the wake lands.
    wake: Wake,
    copied: Receiver<()>,
    /// The app's clock.
    now: Rc<Cell<Instant>>,
    quit: bool,
    step: Option<&'s mut Stepper<'a>>,
}

/// Runs one scenario. A failure, a malformed section or a failed
/// assertion, is prefixed with its line. `step`, if given, is told of
/// each section once it has run.
pub fn run(a: &Archive, step: Option<&mut Stepper>) -> Result<(), String> {
    if a.comment.trim().is_empty() {
        return Err("a scenario starts with prose describing the behavior".into());
    }
    let mut buf = Buffer::new(Storage::Memory(Vec::new()), false);
    // The first part, as the reader announces it.
    buf.apply(Progress::Part { offset: 0 });
    let (wake_tx, wake_rx) = mpsc::channel();
    let mut s = Scenario {
        w: 40,
        h: 6,
        args: Vec::new(),
        env: String::new(),
        macos: false,
        config: Config::default(),
        buf: Some(buf),
        ix: Indexer::new(),
        nl: false,
        file: false,
        rows_below: None,
        seps: true,
        disks: Rc::new(RefCell::new(Vec::new())),
        rendered: Rc::new(RefCell::new(Vec::new())),
        has_eof: a.sections.iter().any(|sec| sec.name == "eof"),
        part: 0,
        finished: false,
        started: false,
        files: Vec::new(),
        keys: Keymap::default(),
        app: None,
        clip: Rc::new(RefCell::new(None)),
        written: Rc::new(RefCell::new(BTreeMap::new())),
        wake: Arc::new(move || {
            let _ = wake_tx.send(());
        }),
        copied: wake_rx,
        now: Rc::new(Cell::new(Instant::now())),
        pending_opts: None,
        quit: false,
        step,
    };
    let mut input_seen = false;
    for i in 0..a.sections.len() {
        let r = s.section(a, i, &mut input_seen);
        let err = r.as_ref().err().map(String::as_str);
        let Scenario {
            step,
            app,
            started,
            quit,
            ..
        } = &mut s;
        if let Some(st) = step {
            st.after(app.as_ref().map(App::terminal), *started, *quit, err);
        }
        r?;
    }
    Ok(())
}

impl Scenario<'_, '_> {
    fn section(&mut self, a: &Archive, i: usize, input_seen: &mut bool) -> Result<(), String> {
        let sec = &a.sections[i];
        let fail = |msg: String| format!("line {}, -- {} --: {msg}", sec.line, sec.name);
        let setup = |started: bool| {
            if started {
                return Err(fail("must come before the app starts".into()));
            }
            Ok(())
        };
        let size =
            || parse_size(&sec.body).ok_or_else(|| fail(format!("want WxH, got {:?}", sec.body)));
        let verb = sec.verb();
        if self.quit && ACTIONS.contains(&verb) {
            return Err(fail(format!("{} after the app quit", sec.name)));
        }
        let started = self.started;
        match verb {
            "size" => {
                setup(self.started)?;
                (self.w, self.h) = size()?;
            }
            "args" => {
                setup(self.started)?;
                self.args = sec.body.split_whitespace().map(String::from).collect();
            }
            "env" => {
                setup(self.started)?;
                self.env = sec.body.trim().to_string();
            }
            "os" => {
                setup(self.started)?;
                self.macos = match sec.body.trim() {
                    "macos" => true,
                    "linux" => false,
                    os => return Err(fail(format!("want macos or linux, got {os:?}"))),
                };
            }
            "config" => {
                setup(self.started)?;
                self.config =
                    config::parse("config", &sec.body).map_err(|e| fail(e.to_string()))?;
            }
            "input" => {
                if self.finished {
                    return Err(fail("input after eof".into()));
                }
                if !self.started && !*input_seen {
                    *input_seen = true;
                    self.input(&sec.body);
                    return Ok(());
                }
                self.start()?;
                self.input(&sec.body);
                self.deliver()?;
            }
            "next-file" => {
                if self.finished {
                    return Err(fail("input after eof".into()));
                }
                self.part += 1;
                let offset = self.ix.offset();
                self.feed(Progress::Part { offset });
                for d in [&self.disks, &self.rendered] {
                    let mut d = d.borrow_mut();
                    if d.is_empty() {
                        d.push(Disk::default());
                    }
                    d.push(Disk::default());
                }
                if self.started && self.part >= self.files.len() {
                    return Err(fail("no more files in -- args --".into()));
                }
            }
            "eof" => {
                if self.finished {
                    return Err(fail("input already finished".into()));
                }
                self.finished = true;
                self.start()?;
                self.finish();
                self.deliver()?;
            }
            "file" | "reload" => {
                // A file before the first key is what the file holds
                // from the start, behind an open_cmd.
                if verb == "reload" || self.started {
                    self.ready().map_err(fail)?;
                }
                let files = match self.started {
                    true => self.files.clone(),
                    false => names(&cli::parse(&self.os_args(), &self.env).map_err(fail)?.1),
                };
                if files.is_empty() || files.iter().any(|f| f == "-") {
                    return Err(fail(format!("{} needs a file in -- args --", sec.name)));
                }
                // With several files the section names the one that changed.
                let mut i = 0;
                if files.len() > 1 || !sec.arg().is_empty() {
                    i = files.iter().position(|f| f == sec.arg()).ok_or_else(|| {
                        fail(format!(
                            "{} names one of the files in -- args --: {}",
                            verb,
                            files.join(" ")
                        ))
                    })?;
                }
                let (text, nl) = lines(&sec.body);
                let mut disks = self.disks.borrow_mut();
                while disks.len() <= i {
                    disks.push(Disk::default());
                }
                disks[i] = Disk { text, nl };
                drop(disks);
                if verb == "reload" {
                    let app = self.app.as_mut().expect("started");
                    self.quit = app.handle(Event::Changed) == Flow::Quit;
                    app.draw();
                }
            }
            "keys" => {
                self.ready().map_err(fail)?;
                let keys = parse_keys(&sec.body).map_err(fail)?;
                for k in keys {
                    let app = self.app.as_mut().expect("started");
                    self.quit = app.handle(Event::Key(k.key)) == Flow::Quit;
                    app.draw();
                }
            }
            "paste" => {
                self.ready().map_err(fail)?;
                let text = sec.body.strip_suffix('\n').unwrap_or(&sec.body);
                let app = self.app.as_mut().expect("started");
                self.quit = app.handle(Event::Paste(text.to_string())) == Flow::Quit;
                app.draw();
            }
            "mouse" => {
                self.ready().map_err(fail)?;
                for a in parse_mouse(&sec.body).map_err(fail)? {
                    self.mouse(a);
                }
            }
            "resize" => {
                self.ready().map_err(fail)?;
                let (w, h) = size()?;
                let app = self.app.as_mut().expect("started");
                app.terminal_mut().resize(w, h);
                self.quit = app.handle(Event::Resize(w, h)) == Flow::Quit;
                app.draw();
            }
            "screen" => {
                self.ready().map_err(fail)?;
                self.settle();
                let app = self.app.as_ref().expect("started");
                let got = render(app.terminal(), app.status_shown());
                if got != sec.body {
                    return Err(fail(format!(
                        "screen differs\n{}",
                        side_by_side(&sec.body, &got)
                    )));
                }
            }
            "cursor" => {
                self.ready().map_err(fail)?;
                self.settle();
                let want = sec.body.trim();
                let got = match self.app.as_ref().expect("started").terminal().cursor() {
                    Some((x, y)) => format!("{y} {x}"),
                    None => "hidden".to_string(),
                };
                if got != want {
                    return Err(fail(format!("cursor = {got}, want {want}")));
                }
            }
            "clipboard" => {
                self.ready().map_err(fail)?;
                self.settle();
                let want = sec.body.strip_suffix('\n').unwrap_or(&sec.body);
                let got = self.clip.borrow().clone().unwrap_or_default();
                if got != want {
                    return Err(fail(format!("clipboard = {got:?}, want {want:?}")));
                }
            }
            "written" => {
                self.ready().map_err(fail)?;
                let name = sec.arg();
                let want = sec.body.strip_suffix('\n').unwrap_or(&sec.body);
                let written = self.written.borrow();
                let Some(got) = written.get(name) else {
                    return Err(fail(format!("{name:?} was not written")));
                };
                let got = String::from_utf8_lossy(got);
                if got != want {
                    return Err(fail(format!("{name} = {got:?}, want {want:?}")));
                }
            }
            "quit" => {
                self.start()?;
                if !self.quit {
                    return Err(fail("the app did not quit".into()));
                }
            }
            _ => return Err(fail("unknown section".into())),
        }
        // Starting the app can quit it too: -F at EOF before the first
        // draw.
        if self.quit
            && sec.name != "quit"
            && (ACTIONS.contains(&verb) || !started)
            && a.sections.get(i + 1).is_none_or(|n| n.name != "quit")
        {
            return Err(fail(
                "the app quit; the next section must be -- quit --".into(),
            ));
        }
        Ok(())
    }

    /// Delivers one action as the events a terminal would send, or for
    /// tick the auto-scroll timer's. The clock moves on a second before
    /// every click or press, so two in a row are never a double-click;
    /// dblclick and tripleclick press two or three times without moving
    /// it. A tick is the tick interval passing.
    fn mouse(&mut self, a: MouseAction) {
        let advance = |now: &Cell<Instant>, by: Duration| now.set(now.get() + by);
        let presses = match a.kind {
            Act::Click => 1,
            Act::DblClick => 2,
            Act::TripleClick => 3,
            _ => 0,
        };
        match a.kind {
            Act::Click | Act::DblClick | Act::TripleClick => {
                advance(&self.now, Duration::from_secs(1));
                for _ in 0..presses {
                    self.send(MouseKind::Press, a.x, a.y, a.shift);
                    self.send(MouseKind::Release, a.x, a.y, a.shift);
                }
            }
            Act::Press => {
                advance(&self.now, Duration::from_secs(1));
                self.send(MouseKind::Press, a.x, a.y, a.shift);
            }
            Act::Release => self.send(MouseKind::Release, a.x, a.y, a.shift),
            Act::Move => self.send(MouseKind::Drag, a.x, a.y, a.shift),
            Act::Drag => {
                advance(&self.now, Duration::from_secs(1));
                self.send(MouseKind::Press, a.x, a.y, a.shift);
                self.send(MouseKind::Drag, a.x2, a.y2, a.shift);
                self.send(MouseKind::Release, a.x2, a.y2, a.shift);
            }
            Act::Wheel => {
                let (rows, cols) = match a.dir {
                    Some(Dir::Up) => (-1, 0),
                    Some(Dir::Down) => (1, 0),
                    Some(Dir::Left) => (0, -1),
                    Some(Dir::Right) | None => (0, 1),
                };
                for _ in 0..a.n {
                    self.send(MouseKind::Wheel { rows, cols }, 0, 0, a.shift);
                }
            }
            Act::Tick => {
                for _ in 0..a.n {
                    advance(&self.now, TICK);
                    let app = self.app.as_mut().expect("started");
                    self.quit = app.handle(Event::Tick) == Flow::Quit;
                    app.draw();
                }
            }
        }
    }

    fn send(&mut self, kind: MouseKind, x: usize, y: usize, shift: bool) {
        let app = self.app.as_mut().expect("started");
        self.quit = app.handle(Event::Mouse(Mouse { kind, x, y, shift })) == Flow::Quit;
        app.draw();
    }

    /// Hands progress to the app once it is open, else to the buffer.
    fn feed(&mut self, p: Progress) {
        if let Some(app) = &mut self.app {
            let quit = app.handle(Event::Progress(GEN, p)) == Flow::Quit;
            self.quit = quit;
        } else if let Some(buf) = &mut self.buf {
            buf.apply(p);
        }
    }

    /// Appends the body to the buffer and to the disk text.
    fn input(&mut self, body: &str) {
        let (text, nl) = lines(body);
        if text.is_empty() {
            return;
        }
        self.nl = nl;
        for disks in [&self.disks, &self.rendered] {
            let mut disks = disks.borrow_mut();
            if disks.is_empty() {
                disks.push(Disk::default());
            }
            let d = disks.last_mut().expect("a disk");
            d.text.push_str(&text);
            d.nl = nl;
        }
        let bytes = text.as_bytes();
        let indexed = self.ix.feed(bytes);
        self.feed(Progress::Data {
            chunk: Some(Arc::from(bytes)),
            indexed,
        });
    }

    /// Ends the input, as the reader does.
    fn finish(&mut self) {
        let indexed = self.ix.finish();
        self.feed(Progress::Data {
            chunk: None,
            indexed,
        });
        self.feed(Progress::Done {
            err: None,
            trailing_newline: self.nl,
        });
    }

    /// The reader's notification, as main handles it: a draw once the
    /// pager is open, else -F's decision, which opens the pager when the
    /// text outgrows the screen and quits to print it when the input
    /// ends first.
    fn deliver(&mut self) -> Result<(), String> {
        if let Some(app) = &mut self.app {
            app.draw();
            return Ok(());
        }
        if let Some(below) = self.rows_below {
            let buf = self.buf.as_ref().expect("the buffer until the app opens");
            match one_page(buf, self.w, self.h, below, self.seps) {
                Verdict::Undecided => return Ok(()),
                Verdict::Print => {
                    self.quit = true;
                    return Ok(());
                }
                Verdict::Page => {}
            }
        }
        self.open();
        Ok(())
    }

    /// Starts the scenario and reports why there is no pager to act on
    /// or assert about.
    /// Hands the app the result of a copy under way: a clipboard
    /// command's, which runs while the keys after it are handled.
    fn settle(&mut self) {
        let app = self.app.as_mut().expect("started");
        while !self.quit && app.copying() {
            self.copied.recv().expect("copier");
            self.quit = app.handle(Event::Copied) == Flow::Quit;
            app.draw();
        }
    }

    fn ready(&mut self) -> Result<(), String> {
        self.start()?;
        match (&self.app, self.quit) {
            (Some(_), _) => Ok(()),
            (None, true) => Err("-F printed the text; the pager never opened".into()),
            (None, false) => Err("-F is waiting for EOF; the pager has not opened".into()),
        }
    }

    /// Freezes the setup the first time an action or assertion needs
    /// the app, and opens the pager. Without an eof section the input
    /// is complete. With -F the pager opens only once the text is known
    /// not to fit, so it may not open yet: see `deliver`.
    fn start(&mut self) -> Result<(), String> {
        if self.started {
            return Ok(());
        }
        self.started = true;
        if !self.has_eof {
            self.finish();
        }
        let Startup {
            opts,
            files,
            file,
            rows_below,
        } = self.options()?;
        if self.part > 0 && self.part >= files.len() {
            return Err("-- next-file --: no more files in -- args --".into());
        }
        self.file = file;
        self.files = files;
        self.rows_below = rows_below;
        self.seps = opts.settings.file_separators;
        self.keys = opts.keys.clone();
        if let Some(buf) = &mut self.buf {
            buf.set_literal(opts.settings.view_style == ViewStyle::Raw);
        }
        self.pending_opts = Some(opts);
        self.deliver()
    }

    /// Builds the app and draws the first frame. A pager that opens has
    /// not quit, whatever -F decided over the text before.
    fn open(&mut self) {
        self.quit = false;
        let opts = self.pending_opts.take().expect("options from start");
        let buf = self.buf.take().expect("the buffer is taken once");
        let mut app = App::new(TestTerminal::new(self.w, self.h), buf, GEN, opts);
        app.draw();
        if let Some(step) = &mut self.step {
            step.opened(app.terminal());
        }
        self.app = Some(app);
    }

    /// `VEDI` and then the command line, over the config, as the app's
    /// options, with the files named beside. A clipboard command, from
    /// The args as the command line carries them.
    fn os_args(&self) -> Vec<OsString> {
        self.args.iter().map(OsString::from).collect()
    }

    /// a flag or the config, is run; without one the copy is recorded.
    /// An open_cmd is not: read through it, a file is its input
    /// sections again. A :write is recorded, ~ being /home/u; the files
    /// in the args and the names written before exist.
    fn options(&self) -> Result<Startup, String> {
        let cfg = &self.config;
        let (flags, files) = cli::parse(&self.os_args(), &self.env)
            .map_err(|e| format!("args {:?}: {e}", self.args))?;
        // The disk text, each file a part, finished: a reload lands at
        // once.
        let (disks, rendered) = (Rc::clone(&self.disks), Rc::clone(&self.rendered));
        let open_cmd = flags.open_cmd(cfg)?;
        let has_cmd = open_cmd.is_some();
        let file = !files.is_empty() && !files.iter().any(|f| f == "-");
        let runtime = Runtime {
            copier: match flags.clipboard_cmd(cfg) {
                Some(cmd) => Box::new(CommandCopier::new(cmd, Killers::default())),
                None => Box::new(Recorder(Rc::clone(&self.clip))),
            },
            wake: Arc::clone(&self.wake),
            open: file.then(|| {
                Box::new(move |_, through| {
                    let from = if through && has_cmd {
                        &rendered
                    } else {
                        &disks
                    };
                    Ok(disk_buffer(&from.borrow()))
                }) as Opener
            }),
            open_cmd,
        };
        let mut opts = flags.app(&files, cfg, self.macos, runtime);
        let files = names(&files);
        let now = Rc::clone(&self.now);
        opts.clock = Box::new(move || now.get());
        let (written, existing) = (Rc::clone(&self.written), files.clone());
        opts.write = Box::new(move |name, overwrite| {
            let name = name.to_string_lossy();
            let name = name.as_ref();
            let exists = existing.iter().any(|f| f == name) || written.borrow().contains_key(name);
            if exists && !overwrite {
                return Err(io::Error::from(ErrorKind::AlreadyExists));
            }
            written.borrow_mut().insert(name.to_string(), Vec::new());
            Ok(Box::new(Written {
                name: name.to_string(),
                files: Rc::clone(&written),
            }))
        });
        opts.home = Some("/home/u".into());
        let rows_below = flags
            .quit_if_one_page
            .then_some(opts.settings.one_page_rows_below);
        Ok(Startup {
            opts,
            files,
            file,
            rows_below,
        })
    }
}

/// The files as the sections name them: the args are text.
fn names(files: &[OsString]) -> Vec<String> {
    files
        .iter()
        .map(|f| f.to_string_lossy().into_owned())
        .collect()
}

/// The files as a buffer read in full, each a part.
fn disk_buffer(disks: &[Disk]) -> Buffer {
    let mut ix = Indexer::new();
    let mut buf = Buffer::new(Storage::Memory(Vec::new()), false);
    let mut nl = false;
    for d in disks {
        buf.apply(Progress::Part {
            offset: ix.offset(),
        });
        if !d.text.is_empty() {
            buf.apply(Progress::Data {
                chunk: Some(Arc::from(d.text.as_bytes())),
                indexed: ix.feed(d.text.as_bytes()),
            });
        }
        nl = d.nl;
    }
    buf.apply(Progress::Data {
        chunk: None,
        indexed: ix.finish(),
    });
    buf.apply(Progress::Done {
        err: None,
        trailing_newline: nl,
    });
    buf
}

fn parse_size(s: &str) -> Option<(usize, usize)> {
    let (w, h) = s.trim().split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// A section body as input bytes. The archive's trailing newline is
/// stripped; a second one means the text itself ended with a newline.
/// Every line is ended, so the buffer sees each as a line at once.
fn lines(body: &str) -> (String, bool) {
    if body.is_empty() {
        return (String::new(), false);
    }
    let text = body.strip_suffix('\n').unwrap_or(body);
    let nl = text.ends_with('\n');
    (format!("{}\n", text.strip_suffix('\n').unwrap_or(text)), nl)
}

/// A file :write made, as the files map holds it.
struct Written {
    name: String,
    files: Rc<RefCell<BTreeMap<String, Vec<u8>>>>,
}

impl Write for Written {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let mut files = self.files.borrow_mut();
        files.entry(self.name.clone()).or_default().extend(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Sink for Written {
    fn done(self: Box<Self>) -> io::Result<()> {
        Ok(())
    }
}

/// The clipboard, as OSC 52 would set it.
struct Recorder(Rc<RefCell<Option<String>>>);

impl Copier for Recorder {
    fn copy(&mut self, text: &str, done: Done) -> Option<Killer> {
        *self.0.borrow_mut() = Some(text.to_string());
        done(Ok(()));
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::archive::parse;

    /// A failed assertion, a broken quit contract and a misspelled
    /// section each fail the scenario, so a green suite means the
    /// assertions ran.
    #[test]
    fn run_rejects() {
        let ok = "Text.\n-- input --\nhi\n-- screen --\nhi\n\n\n\n\n<stdin>  line 1/1  wrap           h help\n";
        assert!(run(&parse(ok), None).is_ok(), "control scenario failed");
        let cases = [
            (
                "wrong screen",
                "T\n-- input --\nhi\n-- screen --\nho\n",
                "line 4, -- screen --: screen differs",
            ),
            (
                "wrong cursor",
                "T\n-- input --\nhi\n-- cursor --\n0 1\n",
                "line 4, -- cursor --: cursor = 0 0, want 0 1",
            ),
            (
                "wrong clipboard",
                "T\n-- input --\nhi\n-- keys --\nCtrl+A Enter\n-- quit --\n-- clipboard --\nho\n",
                "line 7, -- clipboard --: clipboard = \"hi\", want \"ho\"",
            ),
            (
                "quit without section",
                "T\n-- input --\nhi\n-- keys --\nq\n",
                "line 4, -- keys --: the app quit; the next section must be -- quit --",
            ),
            (
                "quit section without quit",
                "T\n-- input --\nhi\n-- keys --\nDown\n-- quit --\n",
                "line 6, -- quit --: the app did not quit",
            ),
            (
                "keys after quit",
                "T\n-- input --\nhi\n-- keys --\nq\n-- quit --\n-- keys --\nDown\n",
                "line 7, -- keys --: keys after the app quit",
            ),
            (
                "unknown section",
                "T\n-- input --\nhi\n-- screeen --\nho\n",
                "line 4, -- screeen --: unknown section",
            ),
            (
                "bad os",
                "T\n-- os --\nwindows\n-- input --\nhi\n",
                "line 2, -- os --: want macos or linux, got \"windows\"",
            ),
            (
                "os after start",
                "T\n-- input --\nhi\n-- keys --\nDown\n-- os --\nmacos\n",
                "line 6, -- os --: must come before the app starts",
            ),
            (
                "config after start",
                "T\n-- input --\nhi\n-- keys --\nDown\n-- config --\nmap q none\n",
                "line 6, -- config --: must come before the app starts",
            ),
            (
                "bad config",
                "T\n-- config --\nmap q nope\n-- input --\nhi\n",
                "line 2, -- config --: config:1: unknown action \"nope\"",
            ),
            (
                "env after start",
                "T\n-- input --\nhi\n-- keys --\nDown\n-- env --\n-S\n",
                "line 6, -- env --: must come before the app starts",
            ),
            (
                "bad env",
                "T\n-- env --\na.txt\n-- input --\nhi\n-- screen --\nhi\n",
                "VEDI: \"a.txt\" is not a flag",
            ),
            (
                "reload without file",
                "T\n-- input --\nhi\n-- reload --\nho\n",
                "line 4, -- reload --: reload needs a file in -- args --",
            ),
            (
                "screen before -F opens",
                "T\n-- args --\n-F\n-- input --\nhi\n-- screen --\nhi\n-- eof --\n",
                "line 6, -- screen --: -F is waiting for EOF; the pager has not opened",
            ),
            (
                "keys before -F opens",
                "T\n-- args --\n-F\n-- input --\nhi\n-- keys --\nDown\n-- eof --\n",
                "line 6, -- keys --: -F is waiting for EOF; the pager has not opened",
            ),
            (
                "screen after -F prints",
                "T\n-- args --\n-F\n-- input --\nhi\n-- quit --\n-- screen --\nhi\n",
                "line 7, -- screen --: -F printed the text; the pager never opened",
            ),
        ];
        for (name, text, want) in cases {
            match run(&parse(text), None) {
                Err(msg) => assert!(
                    msg.contains(want),
                    "{name}: error {msg:?}\nwant it to contain {want:?}"
                ),
                Ok(()) => panic!("{name}: scenario passed, want failure"),
            }
        }
    }
}
