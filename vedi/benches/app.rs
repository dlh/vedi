//! The pager over a large colored input: taking the reader's data up,
//! then a key, a wheel tick, a search, a resize and help, each drawn.

use std::sync::Arc;
use std::time::Instant;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use vedi::app::{App, Copier, Done, Event, Mouse, MouseKind, Options, STDIN};
use vedi::input::config;
use vedi::input::key::Key;
use vedi::input::setting::Settings;
use vedi::sys::opencmd::Killer;
use vedi::text::buffer::{Buffer, Progress, Storage};
use vedi::text::index::Indexer;
use vedi::text::reader::Gen;
use vedi::view::screen::TestTerminal;

const LINES: usize = 100_000;
/// The reader's buffer: a burst of progress per chunk, as live.
const CHUNK: usize = 64 << 10;
const W: usize = 200;
const H: usize = 60;
const GEN: Gen = 1;

struct NoClip;

impl Copier for NoClip {
    fn copy(&mut self, _: &str, done: Done) -> Option<Killer> {
        done(Ok(()));
        None
    }
}

/// A line number, then colored filler, 54 columns.
fn input(n: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for i in 1..=n {
        out.extend_from_slice(
            format!(
                "{i:08}:\x1b[32msome plain text about\x1b[0m sixty \x1b[1mbold\x1b[0m columns wide\n"
            )
            .as_bytes(),
        );
    }
    out
}

fn options() -> Options {
    Options {
        names: vec![STDIN.to_string()],
        settings: Settings::default(),
        start_line: None,
        follow: false,
        screen: None,
        copier: Box::new(NoClip),
        wake: Arc::new(|| {}),
        clock: Box::new(Instant::now),
        macos: false,
        keys: config::default_keymap(false),
        open: None,
        open_cmd: None,
    }
}

fn empty() -> App<TestTerminal> {
    let buf = Buffer::new(Storage::Memory(Vec::new()), false);
    App::new(TestTerminal::new(W, H), buf, GEN, options())
}

/// Feeds `input` as the reader would, a burst per chunk, to EOF.
fn read(a: &mut App<TestTerminal>, input: &[u8]) {
    let mut ix = Indexer::new();
    for chunk in input.chunks(CHUNK) {
        let indexed = ix.feed(chunk);
        a.handle(Event::Progress(
            GEN,
            Progress::Data {
                chunk: Some(Arc::from(chunk)),
                indexed,
            },
        ));
    }
    a.handle(Event::Progress(
        GEN,
        Progress::Data {
            chunk: None,
            indexed: ix.finish(),
        },
    ));
    a.handle(Event::Progress(
        GEN,
        Progress::Done {
            err: None,
            trailing_newline: true,
        },
    ));
}

/// An app over `LINES` lines, read in full, with the first frame drawn.
fn app() -> App<TestTerminal> {
    let mut a = empty();
    read(&mut a, &input(LINES));
    draw(&mut a);
    a
}

/// Draws and discards the cells sent: unread, they would grow for the
/// whole run.
fn draw(a: &mut App<TestTerminal>) {
    a.draw();
    a.terminal_mut().take_draws();
    a.terminal_mut().take_raw();
}

fn press(a: &mut App<TestTerminal>, key: &str) {
    let key: Key = key.parse().unwrap();
    a.handle(Event::Key(key));
    draw(a);
}

fn wheel(a: &mut App<TestTerminal>, rows: i32) {
    a.handle(Event::Mouse(Mouse {
        kind: MouseKind::Wheel { rows, cols: 0 },
        x: 0,
        y: 0,
        shift: false,
    }));
    draw(a);
}

fn progress(c: &mut Criterion) {
    let input = input(LINES);
    let mut g = c.benchmark_group("app");
    g.throughput(Throughput::Bytes(input.len() as u64));
    g.bench_function("progress", |b| {
        b.iter(|| {
            let mut a = empty();
            read(&mut a, &input);
            draw(&mut a);
        })
    });
    g.finish();
}

fn keys(c: &mut Criterion) {
    let mut g = c.benchmark_group("app/key");
    // Each pair goes and comes back, so the text never runs out.
    for (name, there, back) in [
        ("down", "j", "k"),
        ("page", "PgDn", "PgUp"),
        ("ends", "G", "g"),
    ] {
        let mut a = app();
        let mut flip = false;
        g.bench_function(name, |b| {
            b.iter(|| {
                flip = !flip;
                press(&mut a, if flip { there } else { back });
            })
        });
    }
    g.finish();
}

fn search(c: &mut Criterion) {
    let mut a = app();
    // Every thousandth line matches: n scans a thousand, then scrolls.
    for k in ["/", "0", "0", "0", ":", "Enter"] {
        press(&mut a, k);
    }
    c.bench_function("app/search/next", |b| b.iter(|| press(&mut a, "n")));
}

fn mouse(c: &mut Criterion) {
    let mut a = app();
    let mut flip = false;
    c.bench_function("app/wheel", |b| {
        b.iter(|| {
            flip = !flip;
            wheel(&mut a, if flip { 1 } else { -1 });
        })
    });
}

fn resize(c: &mut Criterion) {
    let mut a = app();
    let mut flip = false;
    c.bench_function("app/resize", |b| {
        b.iter(|| {
            flip = !flip;
            let (w, h) = if flip { (W - 1, H - 1) } else { (W, H) };
            a.terminal_mut().resize(w, h);
            a.handle(Event::Resize(w, h));
            draw(&mut a);
        })
    });
}

fn help(c: &mut Criterion) {
    let mut a = app();
    c.bench_function("app/help/toggle", |b| {
        b.iter(|| {
            press(&mut a, "h");
            press(&mut a, "q");
        })
    });
}

criterion_group!(benches, progress, keys, search, mouse, resize, help);
criterion_main!(benches);
