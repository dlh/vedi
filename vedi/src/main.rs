//! vedi is a pager: view ANSI-colored text, select, copy.

use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Write};
use std::panic;
use std::path::Path;
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use vedi::app::{App, Event, Verdict, one_page};
use vedi::cli::{self, OpenCmdFor, USAGE};
use vedi::input::config::{self, Config, LoadError};
use vedi::sys::opencmd::{self, Killer};
use vedi::sys::term::{self, Crossterm};
use vedi::sys::watch::Watch;
use vedi::sys::{clipboard, strerror, tty};
use vedi::text::ansi::{Printer, ViewStyle};
use vedi::text::buffer::{Buffer, PagedPart, Progress, Storage};
use vedi::text::reader::{self, Gen};
use vedi::text::source::{Concat, Source};

const MACOS: bool = cfg!(target_os = "macos");

/// How often -F, waiting on the reader, looks for a resize: mpsc
/// cannot wait on both.
const RESIZE_POLL: Duration = Duration::from_millis(50);

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
            eprintln!("vedi: {}", f.msg);
            if f.usage {
                eprint!("{USAGE}");
            }
            ExitCode::FAILURE
        }
    }
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
        Err(LoadError::NotFound) if opts.config.is_none() => Config::default(),
        Err(LoadError::NotFound) => {
            return Err(format!("open {}: no such file or directory", path.display()).into());
        }
        Err(LoadError::Io(e)) => {
            return Err(format!("open {}: {}", path.display(), strerror(&e)).into());
        }
        Err(e) => return Err(e.to_string().into()),
    };
    let open_cmd = opts.open_cmd(&cfg)?;
    let view = opts.view(&cfg);

    let (tx, rx) = mpsc::channel();
    let killers = Killers::default();
    let _kill = KillOnExit(killers.clone());
    let input = open_input(&files, open_cmd.as_ref()).map_err(|msg| Fail {
        msg,
        usage: files.is_empty(),
    })?;
    let on_disk = input.on_disk;
    let mut buf = killers.start(input, view, 0, &tx);
    if opts.quit_if_one_page {
        // With no terminal to size, -F still prints what fits the
        // environment's size or 80×25; only paging needs one.
        if let Ok(tty) = File::open("/dev/tty").or_else(|_| File::open("/dev/null")) {
            let resize = tty::sigwinch();
            let below = opts.rows_below(&cfg);
            let seps = opts.separators(&cfg);
            let size = || tty::size(&tty);
            if wait_one_page(&mut buf, &rx, &resize, size, below, seps) {
                let _ = print_text(&mut stdout, &buf, view);
                return Ok(());
            }
        }
    }
    drop(stdout);

    let default_hook = panic::take_hook();
    let hook_killers = killers.clone();
    panic::set_hook(Box::new(move |info| {
        term::restore_after_panic();
        default_hook(info);
        // Go's panic ends the process from any goroutine; a thread's
        // here would leave the pager running on a restored terminal.
        // exit runs no Drop, so `KillOnExit` cannot.
        if thread::current().name() != Some("main") {
            hook_killers.kill_all();
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
    );
    if on_disk {
        let (files, killers, tx) = (files.clone(), killers.clone(), tx.clone());
        app_opts.open = Some(Box::new(move |generation| {
            let input = open_input(&files, open_cmd.as_ref()).map_err(io::Error::other)?;
            Ok(killers.start(input, view, generation, &tx))
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

/// Stdin a character device, /dev/null as much as a terminal, has
/// nothing piped in: what Go's main checked.
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
                let md =
                    fs::metadata(name).map_err(|e| format!("stat {shown}: {}", strerror(&e)))?;
                on_disk &= md.is_file();
                let c = opencmd::open(&argv, name).map_err(|e| exec_error(&argv, &e))?;
                killers.push(c.killer());
                parts.push(Box::new(c));
                paged = None;
            }
            None => {
                let f = File::open(name).map_err(|e| format!("open {shown}: {}", strerror(&e)))?;
                on_disk &= f.metadata().is_ok_and(|m| m.is_file());
                if let Some(p) = &mut paged {
                    let file = f
                        .try_clone()
                        .map_err(|e| format!("open {shown}: {}", strerror(&e)))?;
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

/// A command that would not start, as Go's exec words it.
fn exec_error(argv: &[String], e: &io::Error) -> String {
    let prog = argv.first().map_or("", String::as_str);
    match e.kind() {
        io::ErrorKind::NotFound => format!("exec: {prog:?}: executable file not found in $PATH"),
        _ => format!("exec: {prog:?}: {}", strerror(e)),
    }
}

/// The commands reading the inputs, killed when vedi exits and when a
/// reload replaces them: a reader blocked on one would otherwise keep
/// it running. A `Mutex`, as the panic hook kills them from whichever
/// thread panicked.
#[derive(Clone, Default)]
struct Killers(Arc<Mutex<Vec<Killer>>>);

impl Killers {
    /// An empty buffer for `input`, its reader started on `generation`
    /// and sending to `tx`. The commands already running are killed
    /// once it ends, as the app then swaps it in: until then the old
    /// text is still read and shown, as Go closed it after the swap.
    fn start(&self, input: Input, view: ViewStyle, generation: Gen, tx: &Sender<Event>) -> Buffer {
        // Kept until reaped, so exit reaches a set whose reload never
        // ended.
        let mut old = {
            let mut all = self.lock();
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

    fn kill_all(&self) {
        self.lock().iter().for_each(Killer::kill);
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Killer>> {
        // A panic while held left the list whole.
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Kills the commands when vedi exits, however it does.
struct KillOnExit(Killers);

impl Drop for KillOnExit {
    fn drop(&mut self) {
        self.0.kill_all();
    }
}

/// -F before any screen exists: applies the reader's progress until the
/// text outgrows the screen or ends, and reports whether to print it
/// instead of paging. Deciding first keeps short text from flashing
/// through the alternate screen. The terminal may be resized meanwhile,
/// so it is sized at each decision and a resize makes one.
fn wait_one_page(
    buf: &mut Buffer,
    rx: &Receiver<Event>,
    resize: &Receiver<()>,
    mut size: impl FnMut() -> (usize, usize),
    below: usize,
    seps: bool,
) -> bool {
    loop {
        let (w, h) = size();
        match one_page(buf, w, h, below, seps) {
            Verdict::Print => return true,
            Verdict::Page => return false,
            Verdict::Undecided => {}
        }
        loop {
            match rx.recv_timeout(RESIZE_POLL) {
                Ok(Event::Progress(_, p)) => {
                    buf.apply(p);
                    while let Ok(Event::Progress(_, p)) = rx.try_recv() {
                        buf.apply(p);
                    }
                    break;
                }
                // Only the reader sends before the app exists.
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) if resize.try_recv().is_ok() => break,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return false,
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
