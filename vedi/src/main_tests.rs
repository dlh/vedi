use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use vedi::text::index::Indexer;
use vedi::text::source::Chunk;

use std::time::Duration;

use vedi::app::Copier;
use vedi::sys::clipboard::CommandCopier;

use super::*;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> TempDir {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("vedi-main-unit-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn file(&self, name: &str, data: &str) -> OsString {
        let p = self.0.join(name);
        fs::write(&p, data).unwrap();
        p.into_os_string()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn always(argv: &[&str]) -> OpenCmdFor {
    let argv: Vec<String> = argv.iter().map(|a| a.to_string()).collect();
    Box::new(move |_| Some(argv.clone()))
}

fn read_all(i: &mut Input) -> String {
    let (mut out, mut buf) = (Vec::new(), [0; 64]);
    loop {
        match i.src.read(&mut buf).unwrap() {
            Chunk::Data(n) => out.extend_from_slice(&buf[..n]),
            Chunk::Part => {}
            Chunk::Eof => return String::from_utf8(out).unwrap(),
        }
    }
}

fn storage(i: &Input) -> &'static str {
    match i.storage {
        Storage::Paged(_) => "paged",
        Storage::Memory(_) => "memory",
    }
}

#[test]
fn open_input_file_is_paged() {
    let d = TempDir::new();
    let f = d.file("f", "hello\n");
    let i = open_input(&[f], None).unwrap();
    assert_eq!((storage(&i), i.on_disk), ("paged", true));
}

// A pipe cannot be read at random, so it is read once into memory.
#[test]
fn open_input_fifo_is_in_memory() {
    let d = TempDir::new();
    let p = d.0.join("pipe");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&p)
            .status()
            .unwrap()
            .success()
    );
    let w = p.clone();
    let writer = thread::spawn(move || fs::write(w, "hello\n"));
    let mut i = open_input(&[p.into_os_string()], None).unwrap();
    assert_eq!((storage(&i), i.on_disk), ("memory", false));
    assert_eq!(read_all(&mut i), "hello\n");
    writer.join().unwrap().unwrap();
}

// Read through a command, a file's text is kept in memory, but it is
// still on disk to watch and read again.
#[test]
fn open_input_cmd_is_in_memory_on_disk() {
    let d = TempDir::new();
    let f = d.file("f", "hello\n");
    let mut i = open_input(&[f], Some(&always(&["sed", "s/l/L/g", "%s"]))).unwrap();
    assert_eq!(
        (storage(&i), i.on_disk, i.killers.len()),
        ("memory", true, 1)
    );
    assert_eq!(read_all(&mut i), "heLLo\n");
}

// Paged only when every file is read as it is.
#[test]
fn open_input_cmd_by_name() {
    let d = TempDir::new();
    let a = d.file("a.md", "hello\n");
    let b = d.file("b.txt", "hello\n");
    let by_name: OpenCmdFor = Box::new(|name| {
        name.ends_with(".md")
            .then(|| vec!["sed".to_string(), "s/l/L/g".to_string()])
    });
    let mut i = open_input(&[a, b], Some(&by_name)).unwrap();
    assert_eq!(
        (storage(&i), i.on_disk, i.killers.len()),
        ("memory", true, 1)
    );
    assert_eq!(read_all(&mut i), "heLLo\nhello\n");
}

/// A pipe for the reader to read as `Source`.
struct Pipe(io::PipeReader);

impl Source for Pipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        io::Read::read(&mut self.0, buf)
    }
}

/// A buffer whose reader reads what the returned writer writes,
/// reporting on the returned receiver as main's does.
fn one_page_input() -> (Buffer, io::PipeWriter, Receiver<Event>) {
    let (r, w) = io::pipe().unwrap();
    let (tx, rx) = mpsc::channel();
    let input = Input {
        src: Concat::new(vec![Box::new(Pipe(r))]),
        storage: Storage::Memory(Vec::new()),
        on_disk: false,
        killers: Vec::new(),
    };
    let buf = Readers::default().start(input, ViewStyle::Color, 0, &tx);
    (buf, w, rx)
}

// -F waits for the input to end, and text that fits is then printed.
#[test]
fn wait_one_page_prints_at_eof() {
    let (mut buf, mut w, rx) = one_page_input();
    w.write_all(b"hello\n").unwrap();
    drop(w);
    assert!(wait_one_page(&mut buf, &rx, (40, 3), 1, false));
}

// Text that outgrows the screen is paged as soon as it does, with the
// input still open.
#[test]
fn wait_one_page_pages_before_eof() {
    let (mut buf, mut w, rx) = one_page_input();
    w.write_all(b"1\n2\n3\n").unwrap();
    assert!(!wait_one_page(&mut buf, &rx, (40, 3), 1, false));
}

// A terminal that shrinks while -F waits is measured again at its new
// size, without waiting for more input: eight lines fit ten rows and
// not three.
#[test]
fn wait_one_page_resizes() {
    let (tx, rx) = mpsc::channel();
    let mut buf = Buffer::new(Storage::Memory(Vec::new()), false);
    let text = "line\n".repeat(8);
    let indexed = Indexer::new().feed(text.as_bytes());
    let chunk = Some(Arc::from(text.as_bytes()));
    tx.send(Event::Progress(0, Progress::Data { chunk, indexed }))
        .unwrap();
    tx.send(Event::Resize(40, 3)).unwrap();
    // On a thread: ignoring the resize would wait forever, as `tx` is
    // held open.
    let (done, decided) = mpsc::channel();
    thread::spawn(move || done.send(wait_one_page(&mut buf, &rx, (40, 10), 1, false)));
    assert_eq!(
        decided.recv_timeout(Duration::from_secs(2)),
        Ok(false),
        "eight lines on three rows are printed"
    );
}

/// The next progress from generation `g`, skipping the others'.
fn next_from(rx: &Receiver<Event>, g: Gen) -> Option<Progress> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(left) {
            Ok(Event::Progress(pg, p)) if pg == g => return Some(p),
            Ok(_) => {}
            Err(_) => return None,
        }
    }
}

fn is_done(p: &Progress) -> bool {
    matches!(p, Progress::Done { .. })
}

// A reload drops the handles of commands already reaped, so a session
// of many reloads keeps a bounded list.
#[test]
fn reload_prunes_reaped_commands() {
    let d = TempDir::new();
    let f = d.file("f", "hello\n");
    let readers = Readers::default();
    let _kill = KillOnExit(vec![readers.0.clone()]);
    let (tx, rx) = mpsc::channel();
    let cat = always(&["cat", "%s"]);
    for g in 0..3 {
        let input = open_input(std::slice::from_ref(&f), Some(&cat)).unwrap();
        let _buf = readers.start(input, ViewStyle::Color, g, &tx);
        assert!(std::iter::from_fn(|| next_from(&rx, g)).any(|p| is_done(&p)));
    }
    let input = open_input(std::slice::from_ref(&f), Some(&cat)).unwrap();
    let _buf = readers.start(input, ViewStyle::Color, 3, &tx);
    assert_eq!(readers.0.lock().len(), 1);
}

// A reload kills the commands reading the old input, not a clipboard
// command still running.
#[test]
fn reload_spares_a_clipboard_command() {
    let d = TempDir::new();
    let f = d.file("f", "hello\n");
    let readers = Readers::default();
    let copies = Killers::default();
    let _kill = KillOnExit(vec![readers.0.clone(), copies.clone()]);
    let (ctx, crx) = mpsc::channel();
    CommandCopier::new("sleep 1", copies).copy("x", Box::new(move |r| ctx.send(r).unwrap()));
    let (tx, rx) = mpsc::channel();
    let cat = always(&["cat", "%s"]);
    for g in 0..2 {
        let input = open_input(std::slice::from_ref(&f), Some(&cat)).unwrap();
        let _buf = readers.start(input, ViewStyle::Color, g, &tx);
        assert!(std::iter::from_fn(|| next_from(&rx, g)).any(|p| is_done(&p)));
    }
    assert_eq!(crx.recv_timeout(Duration::from_secs(3)).unwrap(), Ok(()));
}

// The old input's command reads on until the reload's text ends, when
// the app swaps it in; killed sooner, the text on screen would end early.
#[test]
fn reload_kills_old_commands_at_its_end() {
    let d = TempDir::new();
    let f = d.file("f", "hello\n");
    let readers = Readers::default();
    let _kill = KillOnExit(vec![readers.0.clone()]);
    let (tx, rx) = mpsc::channel();
    let slow = always(&["sh", "-c", "cat \"$0\"; exec sleep 30", "%s"]);
    let input = open_input(&[f], Some(&slow)).unwrap();
    let _old = readers.start(input, ViewStyle::Color, 0, &tx);
    while !matches!(
        next_from(&rx, 0),
        Some(Progress::Data { chunk: Some(_), .. })
    ) {}

    let (r, mut w) = io::pipe().unwrap();
    let input = Input {
        src: Concat::new(vec![Box::new(Pipe(r))]),
        storage: Storage::Memory(Vec::new()),
        on_disk: false,
        killers: Vec::new(),
    };
    let _new = readers.start(input, ViewStyle::Color, 1, &tx);
    w.write_all(b"new\n").unwrap();
    let deadline = std::time::Instant::now() + Duration::from_millis(300);
    while let Ok(ev) =
        rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
    {
        assert!(
            !matches!(ev, Event::Progress(0, _)),
            "the old input ended early"
        );
    }

    // Generation 0's end can only follow 1's, which is not skipped past.
    drop(w);
    assert!(std::iter::from_fn(|| next_from(&rx, 1)).any(|p| is_done(&p)));
    let ended = std::iter::from_fn(|| next_from(&rx, 0)).find(is_done);
    assert!(
        matches!(ended, Some(Progress::Done { err: Some(_), .. })),
        "the old command was not killed"
    );
}
