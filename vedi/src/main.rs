//! vedi is a pager: view ANSI-colored text, select, copy.

use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Write};
use std::panic;
use std::path::Path;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use vedi::app::{App, Event, Verdict, one_page};
use vedi::cli::{self, OpenCmdFor, USAGE};
use vedi::input::config::{self, Config, LoadError};
use vedi::sys::opencmd::{self, Killer, Killers};
use vedi::sys::term::{self, Crossterm};
use vedi::sys::watch::Watch;
use vedi::sys::{clipboard, tty};
use vedi::text::ansi::{Printer, ViewStyle};
use vedi::text::buffer::{Buffer, PagedPart, Progress, Storage};
use vedi::text::reader::{self, Gen};
use vedi::text::source::{Concat, Source};

const MACOS: bool = cfg!(target_os = "macos");

/// Why vedi stops early; printed once the terminal is put back.
struct Fail {
    msg: String,
    usage: bool,
}

impl From<String> for Fail {
    fn from(msg: String) -> Fail {
        Fail { msg, usage: false }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(f) => {
            eprintln!("vedi: {}", spelled(&f.msg));
            if f.usage {
                eprint!("{USAGE}");
            }
            ExitCode::FAILURE
        }
    }
}

/// `s` with each control spelled `\xNN`: a message echoes a file name
/// or flag from anywhere, and stderr may be the terminal.
fn spelled(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_control() {
            out.push_str(&format!("\\x{:02x}", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

fn run() -> Result<(), Fail> {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let (opts, files) = cli::parse(&args, &env::var("VEDI").unwrap_or_default())
        .map_err(|msg| Fail { msg, usage: true })?;
    let mut stdout = io::stdout().lock();
    if opts.help {
        let _ = stdout.write_all(USAGE.as_bytes());
        return Ok(());
    }
    if let Some(shell) = &opts.completion {
        let _ = stdout.write_all(cli::completion(shell).unwrap_or_default().as_bytes());
        return Ok(());
    }
    if opts.version {
        let _ = writeln!(stdout, "vedi {}", vedi::VERSION);
        return Ok(());
    }
    // The default config may be missing; one named with --config may not.
    let path = opts.config.clone().unwrap_or_else(config::path);
    let cfg = match config::load(&path) {
        Ok(cfg) => cfg,
        Err(LoadError::NotFound(_)) if opts.config.is_none() => Config::default(),
        Err(LoadError::NotFound(e) | LoadError::Io(e)) => {
            return Err(format!("open {}: {e}", path.display()).into());
        }
        Err(e) => return Err(e.to_string().into()),
    };
    let open_cmd = opts.open_cmd(&cfg)?.map(Rc::new);
    let settings = opts.settings(&cfg);
    let view = settings.view_style;

    let (tx, rx) = mpsc::channel();
    let readers = Readers::default();
    let copies = Killers::default();
    let _kill = KillOnExit(vec![readers.0.clone(), copies.clone()]);
    let input = open_input(&files, open_cmd.as_deref()).map_err(|msg| Fail {
        msg,
        usage: files.is_empty(),
    })?;
    let on_disk = input.on_disk;
    let mut buf = readers.start(input, view, 0, &tx);
    if opts.quit_if_one_page {
        // With no terminal to size, -F still prints what fits the
        // environment's size or 80×25; only paging needs one.
        if let Ok(tty) = File::open("/dev/tty").or_else(|_| File::open("/dev/null")) {
            let size = tty::size(&tty);
            let _resize = tty::sigwinch(tx.clone(), tty);
            let below = settings.one_page_rows_below;
            let seps = settings.file_separators;
            if wait_one_page(&mut buf, &rx, size, below, seps) {
                let _ = print_text(&mut stdout, &buf, view);
                return Ok(());
            }
        }
    }
    drop(stdout);

    let default_hook = panic::take_hook();
    let hook_killers = [readers.0.clone(), copies.clone()];
    panic::set_hook(Box::new(move |info| {
        term::restore_after_panic();
        default_hook(info);
        // A thread's panic would leave the pager running on a restored
        // terminal, so it ends the process. exit runs no Drop, so
        // `KillOnExit` cannot.
        if thread::current().name() != Some("main") {
            hook_killers.iter().for_each(Killers::kill_all);
            std::process::exit(2);
        }
    }));
    let term = Crossterm::enter().map_err(|e| e.to_string())?;
    let mut app_opts = opts.app(&files, &cfg, MACOS);
    app_opts.copier = clipboard::pick(
        opts.clipboard_cmd(&cfg),
        &env::var("TERM_PROGRAM").unwrap_or_default(),
        &env::var("TERM").unwrap_or_default(),
        term.raw_writer(),
        copies.clone(),
    );
    app_opts.killers = copies;
    let wake_tx = tx.clone();
    app_opts.wake = Arc::new(move || {
        let _ = wake_tx.send(Event::Copied);
    });
    if let Some(f) = &open_cmd {
        let f = f.clone();
        app_opts.open_cmd = Some(Box::new(move |name| f(name)));
    }
    if on_disk {
        let (files, readers, tx) = (files.clone(), readers.clone(), tx.clone());
        app_opts.open = Some(Box::new(move |generation, through| {
            let cmd = open_cmd.as_deref().filter(|_| through);
            let input = open_input(&files, cmd).map_err(io::Error::other)?;
            Ok(readers.start(input, view, generation, &tx))
        }));
    }
    let mut app = App::new(term, buf, 0, app_opts);
    term::spawn_events(tx.clone());
    // Declared after the app, so dropped before the terminal is put back.
    let _watch = on_disk.then(|| {
        let tx = tx.clone();
        Watch::files(&files, move || {
            let _ = tx.send(Event::Changed);
        })
    });
    drop(tx);
    app.run(rx);
    Ok(())
}

/// The inputs, opened: their sources in order, and where the buffer
/// finds their bytes again.
struct Input {
    src: Concat,
    storage: Storage,
    /// Every input is a regular file, to watch and read again.
    on_disk: bool,
    killers: Vec<Killer>,
}

/// Stdin a character device has nothing piped in. /dev/null is one
/// too, so it is refused like a terminal, not paged as empty text.
fn stdin_is_char_device() -> bool {
    use std::os::fd::AsFd;
    use std::os::unix::fs::FileTypeExt;
    io::stdin()
        .as_fd()
        .try_clone_to_owned()
        .and_then(|fd| File::from(fd).metadata())
        .is_ok_and(|md| md.file_type().is_char_device())
}

/// Opens the files, each a part, or stdin when there are none; "-"
/// names stdin. A file `open_cmd` names a command for is read through
/// it and left for the command to open: a named pipe opened twice
/// waits for a second writer. The text is paged from disk only when
/// every input is a regular file read as it is; a command's output
/// and a pipe can be read only once.
fn open_input(files: &[OsString], open_cmd: Option<&OpenCmdFor>) -> Result<Input, String> {
    let stdin = [OsString::from("-")];
    let names = if files.is_empty() {
        if stdin_is_char_device() {
            return Err("no input: give a file or pipe something in".into());
        }
        &stdin[..]
    } else {
        files
    };
    let mut parts: Vec<Box<dyn Source>> = Vec::new();
    let mut paged = Some(Vec::new());
    let mut on_disk = true;
    let mut killers = Vec::new();
    for name in names {
        if name == "-" {
            on_disk = false;
            parts.push(Box::new(io::stdin()));
            continue;
        }
        let shown = Path::new(name).display();
        match open_cmd.and_then(|f| f(&name.to_string_lossy())) {
            Some(argv) => {
                let md = fs::metadata(name).map_err(|e| format!("stat {shown}: {e}"))?;
                on_disk &= md.is_file();
                let c = opencmd::open(&argv, name).map_err(|e| format!("exec {}: {e}", argv[0]))?;
                killers.push(c.killer());
                parts.push(Box::new(c));
                paged = None;
            }
            None => {
                let f = File::open(name).map_err(|e| format!("open {shown}: {e}"))?;
                on_disk &= f.metadata().is_ok_and(|m| m.is_file());
                if let Some(p) = &mut paged {
                    let file = f.try_clone().map_err(|e| format!("open {shown}: {e}"))?;
                    p.push(PagedPart { file, start: 0 });
                }
                parts.push(Box::new(f));
            }
        }
    }
    let storage = match paged {
        Some(p) if on_disk => Storage::Paged(p),
        _ => Storage::Memory(Vec::new()),
    };
    Ok(Input {
        src: Concat::new(parts),
        storage,
        on_disk,
        killers,
    })
}

/// The commands reading the inputs, killed when vedi exits and when a
/// reload replaces them: a reader blocked on one would otherwise keep
/// it running. A list of their own: a reload must not kill a copy
/// under way.
#[derive(Clone, Default)]
struct Readers(Killers);

impl Readers {
    /// An empty buffer for `input`, its reader started on `generation`
    /// and sending to `tx`. The commands already running are killed
    /// once it ends, as the app then swaps it in: until then the old
    /// text is still read and shown.
    fn start(&self, input: Input, view: ViewStyle, generation: Gen, tx: &Sender<Event>) -> Buffer {
        // Kept until reaped, so exit reaches a set whose reload never
        // ended.
        let mut old = {
            let mut all = self.0.lock();
            all.retain(|k| !k.reaped());
            let old = all.clone();
            all.extend(input.killers);
            old
        };
        let paged = matches!(input.storage, Storage::Paged(_));
        let buf = Buffer::new(input.storage, view == ViewStyle::Raw);
        let tx = tx.clone();
        // Never joined: a tty stdin blocks.
        reader::spawn(input.src, paged, generation, move |g, p| {
            let done = matches!(p, Progress::Done { .. });
            let ok = tx.send(Event::Progress(g, p)).is_ok();
            // After the send, so the swap is queued ahead of the old
            // reader's end.
            if done {
                std::mem::take(&mut old).iter().for_each(Killer::kill);
            }
            ok
        });
        buf
    }
}

/// Kills the commands when vedi exits, however it does.
struct KillOnExit(Vec<Killers>);

impl Drop for KillOnExit {
    fn drop(&mut self) {
        self.0.iter().for_each(Killers::kill_all);
    }
}

/// -F before any screen exists: applies the reader's progress until the
/// text outgrows the screen or ends, and reports whether to print it
/// instead of paging. Deciding first keeps short text from flashing
/// through the alternate screen. A resize makes a decision at the new
/// size.
fn wait_one_page(
    buf: &mut Buffer,
    rx: &Receiver<Event>,
    (mut w, mut h): (usize, usize),
    below: usize,
    seps: bool,
) -> bool {
    loop {
        match one_page(buf, w, h, below, seps) {
            Verdict::Print => return true,
            Verdict::Page => return false,
            Verdict::Undecided => {}
        }
        // One event, then whatever is queued, so a burst is measured once.
        let mut ev = rx.recv();
        loop {
            match ev {
                Ok(Event::Progress(_, p)) => buf.apply(p),
                Ok(Event::Resize(nw, nh)) => (w, h) = (nw, nh),
                // Only the reader and the resize thread send before the
                // app exists.
                Ok(_) => {}
                Err(_) => return false,
            }
            match rx.try_recv() {
                Ok(e) => ev = Ok(e),
                Err(_) => break,
            }
        }
    }
}

/// -F printing the text: its chars, colors and links, and no other
/// escape or control, so text from anywhere cannot act on the terminal;
/// the colors only as `view` shows them.
fn print_text(w: &mut dyn Write, buf: &Buffer, view: ViewStyle) -> io::Result<()> {
    let mut text = Vec::new();
    buf.write_to(&mut text)?;
    let mut p = Printer::new(view);
    let (mut out, mut line_out) = (Vec::with_capacity(text.len()), Vec::new());
    for line in text.split_inclusive(|&b| b == b'\n') {
        // A \r is an ending only before \n; alone at the text's end it
        // is filtered like any other.
        let body = match line.strip_suffix(b"\n") {
            Some(b) => b.strip_suffix(b"\r").unwrap_or(b),
            None => line,
        };
        p.line(body, &mut line_out);
        out.extend_from_slice(&line_out);
        out.extend_from_slice(&line[body.len()..]);
    }
    p.end(&mut line_out);
    out.extend_from_slice(&line_out);
    w.write_all(&out)?;
    w.flush()
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
