use std::cell::{Cell, RefCell};
use std::fs::File;
use std::io;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use super::*;
use crate::app::event::{Mouse, MouseKind};
use crate::app::text_view::{Geometry, Motion, Row, Start, TextView};
use crate::input::config;
use crate::input::key::Key;
use crate::sys::opencmd::{Killer, Killers};
use crate::text::buffer::tests::{Fill, temp};
use crate::text::buffer::{Progress, ReadError, Storage};
use crate::text::index::Indexer;
use crate::view::screen::TestTerminal;
use crate::view::style::{CUR_MATCH, Color, MATCH, SELECTION, SEP_NAME, SEP_RULE, Style};

const GEN: Gen = 1;

struct Clip(Rc<RefCell<Option<String>>>);

impl Copier for Clip {
    fn copy(&mut self, text: &str, done: Done) -> Option<Killer> {
        *self.0.borrow_mut() = Some(text.to_string());
        done(Ok(()));
        None
    }
}

/// A copier that holds each result for the test to give.
struct Slow(Rc<RefCell<Vec<Done>>>);

impl Copier for Slow {
    fn copy(&mut self, _: &str, done: Done) -> Option<Killer> {
        self.0.borrow_mut().push(done);
        None
    }
}

fn options() -> Options {
    Options {
        names: vec![STDIN.to_string()],
        settings: Settings::default(),
        start_line: None,
        follow: false,
        screen: None,
        copier: Box::new(Clip(Rc::default())),
        wake: Arc::new(|| {}),
        killers: Killers::default(),
        clock: Box::new(Instant::now),
        macos: false,
        keys: config::default_keymap(false),
        open: None,
        open_cmd: None,
    }
}

/// An app on a w×h terminal over `input`, read in full, with the first
/// frame drawn.
fn app(w: usize, h: usize, input: &str, opts: Options) -> App<TestTerminal> {
    let mut f = Fill::memory(false);
    f.part();
    f.write(input.as_bytes());
    f.finish(None);
    let mut a = App::new(TestTerminal::new(w, h), f.buf, GEN, opts);
    a.draw();
    a
}

/// An app whose input is still being read: `feed` and `done` are its
/// reader.
fn reading(w: usize, h: usize, opts: Options) -> (App<TestTerminal>, Indexer) {
    let mut f = Fill::memory(false);
    f.part();
    let mut a = App::new(TestTerminal::new(w, h), f.buf, GEN, opts);
    a.draw();
    (a, Indexer::new())
}

/// `reading`, with a first line drawn: the hold runs from that draw.
fn reading_drawn(w: usize, h: usize, opts: Options) -> (App<TestTerminal>, Indexer) {
    let (mut a, mut ix) = reading(w, h, opts);
    feed(&mut a, &mut ix, "w\n");
    a.draw();
    (a, ix)
}

fn feed(a: &mut App<TestTerminal>, ix: &mut Indexer, s: &str) {
    let indexed = ix.feed(s.as_bytes());
    a.handle(Event::Progress(
        GEN,
        Progress::Data {
            chunk: Some(Arc::from(s.as_bytes())),
            indexed,
        },
    ));
}

fn done(a: &mut App<TestTerminal>, ix: &mut Indexer, err: Option<&str>) {
    let indexed = ix.finish();
    a.handle(Event::Progress(
        GEN,
        Progress::Data {
            chunk: None,
            indexed,
        },
    ));
    a.handle(Event::Progress(GEN, finished(err)));
    a.draw();
}

fn finished(err: Option<&str>) -> Progress {
    Progress::Done {
        err: err.map(|e| ReadError::Io(Arc::new(io::Error::other(e.to_string())))),
        trailing_newline: false,
    }
}

/// Screen row `y`'s text, trailing spaces trimmed.
fn row(a: &App<TestTerminal>, y: usize) -> String {
    let (w, _) = a.screen.size();
    let s: String = (0..w)
        .map(|x| a.terminal().cell(x, y))
        .filter(|c| c.width > 0)
        .map(|c| c.text.as_str())
        .collect();
    s.trim_end_matches(' ').to_string()
}

fn style(a: &App<TestTerminal>, x: usize, y: usize) -> Style {
    a.terminal().cell(x, y).style
}

/// Feeds keys by name, redrawing after each, and reports whether the
/// last quit.
fn press(a: &mut App<TestTerminal>, keys: &str) -> Flow {
    let mut flow = Flow::Continue;
    for k in keys.split_whitespace() {
        let k: Key = k.parse().unwrap();
        flow = a.handle(Event::Key(k));
        a.draw();
    }
    flow
}

#[test]
fn draw_styles() {
    let a = app(20, 5, "\x1b[31mred", options());
    assert_eq!(style(&a, 0, 0).fg, Color::Palette(1));
}

/// view_style plain draws the text in the default style, a control
/// still in reverse video and a link still a link; color brings the
/// styles back.
#[test]
fn view_style_plain() {
    let opts = Options {
        settings: Settings {
            view_style: ViewStyle::Plain,
            ..Settings::default()
        },
        ..options()
    };
    let mut a = app(20, 5, "\x1b[1;31mred\r\x1b]8;;http://x\x07link", opts);
    assert_eq!(style(&a, 0, 0), Style::default());
    assert_eq!(style(&a, 3, 0), Style::default().reverse(), "control");
    let link = a.terminal().cell(5, 0).link.clone();
    assert_eq!(link.map(|l| l.url.to_string()).as_deref(), Some("http://x"));
    // color brings the styles back.
    a.run_command("view_style color");
    a.draw();
    assert_eq!(style(&a, 0, 0).fg, Color::Palette(1));
}

#[test]
fn draw_links() {
    let a = app(20, 5, "\x1b]8;;http://x\x1b\\a\x1b]8;;\x1b\\b", options());
    let c = a.terminal().cell(0, 0);
    assert_eq!(c.link.as_ref().map(|l| &*l.url), Some("http://x"));
    assert_eq!(c.style, Style::default());
    let c = a.terminal().cell(1, 0);
    assert!(c.link.is_none());
    assert_eq!(c.style, Style::default());
}

#[test]
fn selection_style_is_constant() {
    // red, default, reverse-video and a control char, all selected.
    let mut a = app(20, 5, "\x1b[31mr\x1b[md\x1b[7mv\x1b[m\r", options());
    assert_eq!(style(&a, 0, 0).fg, Color::Palette(1), "unselected");
    press(&mut a, "Ctrl+a");
    for (x, name) in ["red", "default", "reverse", "^", "M"]
        .into_iter()
        .enumerate()
    {
        assert_eq!(style(&a, x, 0), SELECTION, "selected {name}");
    }
}

/// Drawing a screen again sends the terminal nothing, wide and linked
/// cells included; a cursor move off the first row changes the status
/// row alone.
#[test]
fn redraw_sends_only_changes() {
    let input = "plain\n日本語 👍🏽 é\n\x1b]8;;http://x\x1b\\link\x1b]8;;\x1b\\\n\ttab \x01\n";
    let mut a = app(40, 6, input, options());
    a.terminal_mut().take_draws();
    a.draw();
    assert!(a.terminal_mut().take_draws().is_empty());
    press(&mut a, "Down");
    let draws = a.terminal_mut().take_draws();
    assert!(!draws.is_empty());
    assert!(draws.iter().all(|&(_, y)| y == 5), "{draws:?}");
    assert!(
        draws
            .iter()
            .any(|&(x, y)| a.terminal().cell(x, y).text == "2"),
        "the new line number was not sent"
    );
}

/// With nothing cleared first, cells the last frame drew and this one
/// does not are blanked.
#[test]
fn draw_blanks_what_it_leaves() {
    let (open, gens) = opener(0);
    let mut a = app(
        20,
        4,
        "日本語日本語\nsecond line\nthird\n",
        with_opener(open),
    );
    a.handle(Event::Changed);
    finish_reload(&mut a, &gens, 0, "x\n");
    for (y, want) in ["x", "", ""].into_iter().enumerate() {
        assert_eq!(row(&a, y), want, "row {y}");
    }
    for x in 0..20 {
        let c = a.terminal().cell(x, 1);
        assert_eq!((c.style, c.width), (Style::default(), 1), "cell {x}");
    }
}

/// The row naming an input draws the name bold and the dashes dim.
#[test]
fn separator_style() {
    let mut f = Fill::memory(false);
    f.part();
    f.write(b"a\n");
    f.part();
    f.write(b"b\n");
    f.finish(None);
    let opts = Options {
        names: vec!["a.txt".into(), "b.txt".into()],
        ..options()
    };
    let mut a = App::new(TestTerminal::new(20, 4), f.buf, GEN, opts);
    a.draw();
    assert_eq!(row(&a, 0), "── a.txt ───────────");
    for (x, want) in [
        (0, SEP_RULE),
        (2, SEP_RULE),
        (3, SEP_NAME),
        (7, SEP_NAME),
        (8, SEP_RULE),
        (19, SEP_RULE),
    ] {
        assert_eq!(style(&a, x, 0), want, "cell {x}");
    }
}

#[test]
fn read_error_in_status() {
    let (mut a, mut ix) = reading(30, 4, options());
    feed(&mut a, &mut ix, "partial");
    done(&mut a, &mut ix, Some("disk on fire"));
    assert_eq!(row(&a, 3), "read error: disk on fire");
    assert_eq!(row(&a, 0), "partial", "text kept after error");
}

/// A read error brings a hidden status line back, and keeps it through
/// a key.
#[test]
fn read_error_shows_hidden_status() {
    let opts = Options {
        settings: Settings {
            status_line: false,
            ..Settings::default()
        },
        ..options()
    };
    let (mut a, mut ix) = reading(30, 2, opts);
    feed(&mut a, &mut ix, "one\ntwo");
    done(&mut a, &mut ix, None);
    assert_eq!(row(&a, 1), "two", "the text has the bottom row");
    a.handle(Event::Progress(GEN, finished(Some("disk on fire"))));
    press(&mut a, "j");
    assert_eq!(row(&a, 1), "read error: disk on fire");
    assert_eq!(row(&a, 0), "two", "the cursor's row");
}

#[test]
fn truncated_file_draws() {
    let text: String = (0..2000).map(|i| format!("line {i}\n")).collect();
    let path = temp(text.as_bytes());
    let mut f = Fill::paged(vec![File::open(&path).unwrap()]);
    f.part();
    f.write(text.as_bytes());
    f.finish(None);
    let mut a = App::new(TestTerminal::new(30, 4), f.buf, GEN, options());
    press(&mut a, "Right Right");
    // Evict the decoded block, not the layouts.
    a.view.buf().line(100);
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(0)
        .unwrap();
    a.draw();
    assert_eq!(row(&a, 0), "");
    assert_eq!(row(&a, 3), "read error: input truncated");
    // From past the line's new end.
    press(&mut a, "Left");
    assert_eq!(a.cursor(), Pos::default());
}

/// `run` returns once the events' sender is gone, as the terminal's is
/// when it closes.
#[test]
fn run_ends_with_screen() {
    let (done_tx, done_rx) = channel();
    thread::spawn(move || {
        let mut a = app(10, 4, "x", options());
        let (tx, rx) = channel::<Event>();
        drop(tx);
        a.run(rx);
        let _ = done_tx.send(());
    });
    done_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("run did not return");
}

/// Progress queued together is taken up once: a source that writes a
/// line at a time would otherwise decode the last block per line.
#[test]
fn queued_progress_decodes_once() {
    let (done_tx, done_rx) = channel();
    thread::spawn(move || {
        let opts = Options {
            follow: true,
            ..options()
        };
        let (mut a, mut ix) = reading(10, 4, opts);
        let (tx, rx) = channel::<Event>();
        for _ in 0..20 {
            let indexed = ix.feed(b"x\n");
            let p = Progress::Data {
                chunk: Some(Arc::from(&b"x\n"[..])),
                indexed,
            };
            tx.send(Event::Progress(GEN, p)).unwrap();
        }
        drop(tx);
        a.run(rx);
        let _ = done_tx.send((a.view.buf().decodes(), a.cursor().line));
    });
    let (decodes, line) = done_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("run did not return");
    assert_eq!(line, 19, "the view followed");
    assert_eq!(decodes, 1);
}

#[test]
fn highlight_keeps_link() {
    let mut a = app(
        20,
        5,
        "\x1b]8;id=k;http://x\x1b\\a\x1b]8;;\x1b\\a",
        options(),
    );
    press(&mut a, "/ a Enter");
    let link = |a: &App<TestTerminal>, x| {
        a.terminal()
            .cell(x, 0)
            .link
            .clone()
            .map(|l| (l.url.to_string(), l.id.to_string()))
    };
    let linked = Some(("http://x".to_string(), "k".to_string()));
    assert_eq!((style(&a, 0, 0), link(&a, 0)), (CUR_MATCH, linked.clone()));
    assert_eq!((style(&a, 1, 0), link(&a, 1)), (MATCH, None));
    press(&mut a, "Ctrl+a");
    assert_eq!((style(&a, 0, 0), link(&a, 0)), (SELECTION, linked));
    assert_eq!((style(&a, 1, 0), link(&a, 1)), (SELECTION, None));
}

#[test]
fn match_style_is_constant() {
    // red, default and reverse-video text, all matched.
    let mut a = app(20, 5, "\x1b[31mo\x1b[mo\x1b[7mo\x1b[m", options());
    press(&mut a, "/ o Enter");
    for (x, name) in ["red", "default", "reverse"].into_iter().enumerate() {
        // Each o is a match, and the cursor's is drawn apart.
        for cx in 0..3 {
            let want = if x == cx { CUR_MATCH } else { MATCH };
            assert_eq!(style(&a, x, 0), want, "cursor on {cx}: matched {name}");
            press(&mut a, "n");
        }
    }
    // A selected match is drawn as selected.
    press(&mut a, "Shift+Right");
    assert_eq!(style(&a, 0, 0), SELECTION);
}

fn mouse(a: &mut App<TestTerminal>, kind: MouseKind, x: usize, y: usize) {
    a.handle(Event::Mouse(Mouse {
        kind,
        x,
        y,
        shift: false,
    }));
    a.draw();
}

/// A drag held at the bottom row arms the auto-scroll tick; releasing
/// the button disarms it, so the loop has nothing to wait for.
#[test]
fn tick_deadline_clears_when_drag_ends() {
    let mut a = app(30, 4, "a\nb\nc\nd\ne", options());
    assert_eq!(a.deadline(), None);
    mouse(&mut a, MouseKind::Press, 0, 0);
    mouse(&mut a, MouseKind::Drag, 0, 3);
    assert!(a.deadline().is_some(), "armed at the edge");
    mouse(&mut a, MouseKind::Release, 0, 3);
    assert_eq!(a.deadline(), None);
    a.handle(Event::Tick);
    a.draw();
    assert_eq!(row(&a, 0), "a", "a tick after release scrolls nothing");
}

/// Options whose clock the test moves.
fn clocked() -> (Options, Rc<Cell<Instant>>) {
    let now = Rc::new(Cell::new(Instant::now()));
    let clock = Rc::clone(&now);
    let opts = Options {
        clock: Box::new(move || clock.get()),
        ..options()
    };
    (opts, now)
}

fn with_opener(open: Opener) -> Options {
    Options {
        open: Some(open),
        ..options()
    }
}

/// A test opener: each call hands out an empty buffer and records the
/// generation asked for, and whether through open_cmd, after failing
/// `fail` times.
type Gens = Rc<RefCell<Vec<(Gen, bool)>>>;

fn opener(fail: usize) -> (Opener, Gens) {
    let gens: Gens = Rc::default();
    let left = Rc::new(Cell::new(fail));
    let (g, l) = (Rc::clone(&gens), left);
    let open = Box::new(move |generation, through| {
        if l.get() > 0 {
            l.set(l.get() - 1);
            return Err(io::Error::other("boom"));
        }
        g.borrow_mut().push((generation, through));
        Ok(Buffer::new(Storage::Memory(Vec::new()), false))
    });
    (open, gens)
}

/// Ends reload `i` with `text`, as its reader would.
fn finish_reload(a: &mut App<TestTerminal>, gens: &Gens, i: usize, text: &str) {
    end_reload(a, gens, i, text, None);
}

/// `finish_reload`, with the error the read ended in.
fn end_reload(a: &mut App<TestTerminal>, gens: &Gens, i: usize, text: &str, err: Option<&str>) {
    let g = gens.borrow()[i].0;
    let mut ix = Indexer::new();
    let data = |chunk: Option<&str>, indexed| Progress::Data {
        chunk: chunk.map(|c| Arc::from(c.as_bytes())),
        indexed,
    };
    a.handle(Event::Progress(g, Progress::Part { offset: 0 }));
    a.handle(Event::Progress(
        g,
        data(Some(text), ix.feed(text.as_bytes())),
    ));
    a.handle(Event::Progress(g, data(None, ix.finish())));
    a.handle(Event::Progress(g, finished(err)));
    a.draw();
}

fn open_cmd_options(open: Opener) -> Options {
    Options {
        names: vec!["a.md".into()],
        open_cmd: Some(Box::new(|name: &str| {
            let cmd = match name.rsplit('.').next() {
                Some("md") => "mdcat",
                Some("txt") => "cat",
                _ => return None,
            };
            Some(vec![cmd.into(), name.into()])
        })),
        ..with_opener(open)
    }
}

/// The message names the input the cursor is in when the text lands,
/// not the one it was in when O was pressed.
#[test]
fn open_cmd_status_follows_cursor() {
    let (open, gens) = opener(0);
    let opts = Options {
        names: vec!["a.md".into(), "b.txt".into()],
        ..open_cmd_options(open)
    };
    let mut a = App::new(TestTerminal::new(40, 4), two_parts(), GEN, opts);
    a.draw();
    press(&mut a, "O");
    press(&mut a, "O");
    press(&mut a, "Down");
    let g = gens.borrow()[1].0;
    let mut ix = Indexer::new();
    let chunk = |s: &str| Some(Arc::from(s.as_bytes()));
    a.handle(Event::Progress(g, Progress::Part { offset: 0 }));
    let indexed = ix.feed(b"x\n");
    a.handle(Event::Progress(
        g,
        Progress::Data {
            chunk: chunk("x\n"),
            indexed,
        },
    ));
    let offset = ix.offset();
    a.handle(Event::Progress(g, Progress::Part { offset }));
    let indexed = ix.feed(b"y\n");
    a.handle(Event::Progress(
        g,
        Progress::Data {
            chunk: chunk("y\n"),
            indexed,
        },
    ));
    let indexed = ix.finish();
    a.handle(Event::Progress(
        g,
        Progress::Data {
            chunk: None,
            indexed,
        },
    ));
    a.handle(Event::Progress(g, finished(None)));
    a.draw();
    assert!(
        row(&a, 3).starts_with("open_cmd cat b.txt"),
        "{:?}",
        row(&a, 3)
    );
}

/// O's message waits for the text it describes, and a read error
/// takes its place.
#[test]
fn open_cmd_status_lands_with_text() {
    let (open, gens) = opener(0);
    let mut a = app(40, 4, "old\n", open_cmd_options(open));
    press(&mut a, "O");
    assert!(row(&a, 3).contains("reloading…"), "{:?}", row(&a, 3));
    finish_reload(&mut a, &gens, 0, "raw\n");
    assert!(row(&a, 3).starts_with("open_cmd ''"), "{:?}", row(&a, 3));
    press(&mut a, "O");
    end_reload(&mut a, &gens, 1, "", Some("exit 1"));
    assert!(
        row(&a, 3).starts_with("read error: exit 1"),
        "{:?}",
        row(&a, 3)
    );
}

/// O during a reload drops it and starts its own: the old reader's
/// data is not taken up.
#[test]
fn open_cmd_toggle_replaces_pending_reload() {
    let (open, gens) = opener(0);
    let mut a = app(40, 4, "old\n", open_cmd_options(open));
    press(&mut a, "O");
    press(&mut a, "O");
    let through: Vec<bool> = gens.borrow().iter().map(|g| g.1).collect();
    assert_eq!(through, [false, true]);
    finish_reload(&mut a, &gens, 1, "new\n");
    assert_eq!(row(&a, 0), "new");
    assert!(
        row(&a, 3).starts_with("open_cmd mdcat a.md"),
        "{:?}",
        row(&a, 3)
    );
    finish_reload(&mut a, &gens, 0, "stale\n");
    assert_eq!(row(&a, 0), "new");
}

/// Progress before a draw shares one deadline; the draw clears it, and
/// the next progress sets it again.
#[test]
fn redraw_hold_coalesces_progress() {
    let (opts, now) = clocked();
    let (mut a, mut ix) = reading_drawn(10, 4, opts);
    let t0 = now.get();
    feed(&mut a, &mut ix, "x\n");
    feed(&mut a, &mut ix, "y\n");
    assert_eq!(a.deadline(), Some(t0 + REDRAW_EVERY));
    a.draw();
    assert_eq!(a.deadline(), None);
    feed(&mut a, &mut ix, "z\n");
    assert_eq!(a.deadline(), Some(t0 + REDRAW_EVERY));
}

/// Progress within the hold of the last draw is not drawn until the
/// hold passes.
#[test]
fn redraw_held_while_reading() {
    let (opts, now) = clocked();
    let (mut a, mut ix) = reading_drawn(10, 4, opts);
    feed(&mut a, &mut ix, "x\n");
    assert!(!a.redraw_due(), "held within the hold");
    now.set(now.get() + 2 * REDRAW_EVERY);
    assert!(a.redraw_due(), "due once the hold passed");
}

/// The first data is drawn at once: the empty first frame took none up,
/// so it starts no hold.
#[test]
fn first_data_is_not_held() {
    let (opts, _) = clocked();
    let (mut a, mut ix) = reading(10, 4, opts);
    feed(&mut a, &mut ix, "x\n");
    assert_eq!(a.deadline(), None);
    assert!(a.redraw_due());
}

/// The end of input is not held back, even over a held progress.
#[test]
fn eof_is_drawn_at_once() {
    let (opts, _) = clocked();
    let (mut a, mut ix) = reading_drawn(10, 4, opts);
    feed(&mut a, &mut ix, "x\n");
    assert!(a.deadline().is_some(), "held");
    a.handle(Event::Progress(
        GEN,
        Progress::Data {
            chunk: None,
            indexed: ix.finish(),
        },
    ));
    a.handle(Event::Progress(GEN, finished(None)));
    assert_eq!(a.deadline(), None);
    assert!(a.redraw_due());
}

/// A key's draw takes held progress up; progress after it is held from
/// that draw, once.
#[test]
fn held_progress_draws_once_after_key() {
    let (opts, now) = clocked();
    let (mut a, mut ix) = reading_drawn(10, 4, opts);
    feed(&mut a, &mut ix, "x\n");
    now.set(now.get() + REDRAW_EVERY / 2);
    press(&mut a, "j");
    assert_eq!(a.deadline(), None, "the key's draw took the data up");
    feed(&mut a, &mut ix, "y\n");
    assert_eq!(a.deadline(), Some(now.get() + REDRAW_EVERY));
}

fn two_parts() -> Buffer {
    let mut f = Fill::memory(false);
    f.part();
    f.write(b"a\n");
    f.part();
    f.write(b"b\n");
    f.finish(None);
    f.buf
}

/// A two-row view of `buf` with separators, the second input's
/// separator row at the top.
fn on_second_input(buf: Buffer) -> TextView {
    let geom = Geometry {
        rows: 2,
        height: 3,
        cols: 20,
        separators: true,
        ..Geometry::default()
    };
    let mut v = TextView::new(buf, Start::default(), geom);
    v.go(Motion::NextInput, false);
    assert_eq!(v.top(), Row::sep(1));
    v
}

#[test]
fn reload_keeps_separator_top() {
    let mut v = on_second_input(two_parts());
    v.change_text(|b| *b = two_parts());
    assert_eq!(v.top(), Row::sep(1));
}

/// A separator top past the new end starts over from the first row,
/// the first line's separator if it has one.
#[test]
fn reload_separator_top_past_end() {
    let mut f = Fill::memory(false);
    f.part();
    f.write(b"a\n");
    f.finish(None);
    let mut v = on_second_input(two_parts());
    v.change_text(|b| *b = f.buf);
    assert_eq!(v.top(), Row::sep(0));
    assert_eq!(v.cursor(), Pos::default());
}

#[test]
fn reload_pending_status() {
    let (open, gens) = opener(0);
    let mut a = app(40, 4, "old\n", with_opener(open));
    a.handle(Event::Changed);
    a.draw();
    assert!(row(&a, 3).ends_with("  reloading…"), "{:?}", row(&a, 3));
    assert_eq!(row(&a, 0), "old");
    finish_reload(&mut a, &gens, 0, "new\n");
    assert_eq!(row(&a, 0), "new");
    assert!(!row(&a, 3).contains("reloading"), "{:?}", row(&a, 3));
}

/// Changes during a reload start exactly one more after it.
#[test]
fn reload_coalesces() {
    let (open, gens) = opener(0);
    let mut a = app(30, 4, "old\n", with_opener(open));
    for _ in 0..3 {
        a.handle(Event::Changed);
    }
    assert_eq!(gens.borrow().len(), 1, "opens during a reload");
    finish_reload(&mut a, &gens, 0, "one\n");
    assert_eq!(gens.borrow().len(), 2, "opens after the first swap");
    finish_reload(&mut a, &gens, 1, "two\n");
    assert_eq!(gens.borrow().len(), 2, "opens after the second swap");
    assert_eq!(row(&a, 0), "two");
}

/// auto_reload no at the : prompt drops the reload a change during the
/// running one asked for.
#[test]
fn auto_reload_off_drops_queued_reload() {
    let (open, gens) = opener(0);
    let mut a = app(30, 4, "old\n", with_opener(open));
    a.handle(Event::Changed);
    a.handle(Event::Changed);
    a.run_command("auto_reload no");
    finish_reload(&mut a, &gens, 0, "one\n");
    assert_eq!(gens.borrow().len(), 1, "opens after the swap");
    assert_eq!(row(&a, 0), "one");
}

#[test]
fn reload_open_fails_on_change() {
    let (open, _) = opener(usize::MAX);
    let mut a = app(30, 4, "old\n", with_opener(open));
    a.handle(Event::Changed);
    a.draw();
    assert_eq!(a.status, None, "only the key reports");
    assert_eq!(row(&a, 0), "old");
}

/// A reload lands while the startup buffer is still being read; the old
/// reader's later data does not come back.
#[test]
fn reload_during_first_read() {
    let (open, gens) = opener(0);
    let (mut a, mut ix) = reading(30, 4, with_opener(open));
    feed(&mut a, &mut ix, "one\n");
    a.handle(Event::Changed);
    finish_reload(&mut a, &gens, 0, "two\n");
    assert_eq!(row(&a, 0), "two");
    feed(&mut a, &mut ix, "more\n");
    done(&mut a, &mut ix, None);
    assert_eq!(row(&a, 0), "two", "the old reader's data");
    assert!(!row(&a, 3).contains("reading"), "{:?}", row(&a, 3));
}

#[test]
fn reload_key_reports_failure() {
    let (open, _) = opener(usize::MAX);
    let mut a = app(30, 4, "old\n", with_opener(open));
    press(&mut a, "R");
    assert!(
        row(&a, 3).starts_with("reload failed: boom"),
        "{:?}",
        row(&a, 3)
    );
}

/// A failed reload by O says so, not which command.
#[test]
fn open_cmd_key_reports_failure() {
    let (open, _) = opener(usize::MAX);
    let mut a = app(30, 4, "old\n", with_opener(open));
    press(&mut a, "O");
    assert_eq!(a.status.as_deref(), Some("reload failed: boom"));
}

/// A file that vanished when a change came is read again on the next
/// change.
#[test]
fn reload_after_vanished_file_still_reloads() {
    let (open, gens) = opener(1);
    let mut a = app(30, 4, "old\n", with_opener(open));
    a.handle(Event::Changed);
    assert!(gens.borrow().is_empty());
    a.handle(Event::Changed);
    assert_eq!(gens.borrow().len(), 1);
    finish_reload(&mut a, &gens, 0, "new\n");
    assert_eq!(row(&a, 0), "new");
}

// A copier that takes time reports on `Event::Copied`, which its done
// wakes the loop for; keys meanwhile are handled.
#[test]
fn copy_reports_when_the_copier_is_done() {
    let done = Rc::new(RefCell::new(Vec::new()));
    let woke = Arc::new(AtomicUsize::new(0));
    let mut opts = options();
    opts.copier = Box::new(Slow(Rc::clone(&done)));
    let w = Arc::clone(&woke);
    opts.wake = Arc::new(move || {
        w.fetch_add(1, Ordering::SeqCst);
    });
    let mut a = app(20, 3, "ab\ncd\n", opts);
    press(&mut a, "Shift+Right y");
    assert_eq!(a.status.as_deref(), Some("copying"));
    assert!(a.copying());
    press(&mut a, "Down");
    assert_eq!(a.status, None, "a key clears the status");
    a.handle(Event::Copied);
    assert!(a.copying(), "no result yet");
    done.borrow_mut().remove(0)(Ok(()));
    assert_eq!(woke.load(Ordering::SeqCst), 1);
    a.handle(Event::Copied);
    assert_eq!(a.status.as_deref(), Some("copied 1 line"));
    assert!(!a.copying());
}

// A copy started before the last is done supersedes it: the first's
// result, whenever it comes, is not the one reported.
#[test]
fn copy_supersedes_one_under_way() {
    let done = Rc::new(RefCell::new(Vec::new()));
    let mut opts = options();
    opts.copier = Box::new(Slow(Rc::clone(&done)));
    let mut a = app(20, 3, "ab\ncd\n", opts);
    press(&mut a, "Shift+Right y Shift+Down y");
    assert_eq!(done.borrow().len(), 2);
    done.borrow_mut().remove(0)(Ok(()));
    a.handle(Event::Copied);
    assert!(a.copying(), "the first copy's result is dropped");
    assert_eq!(a.status.as_deref(), Some("copying"));
    done.borrow_mut().remove(0)(Err("late".into()));
    a.handle(Event::Copied);
    assert_eq!(a.status.as_deref(), Some("copy failed: late"));
}

// Enter quits on success, so it waits for the result.
#[test]
fn copy_and_quit_waits_for_the_copier() {
    let (tx, rx) = channel::<Done>();
    struct Handoff(Sender<Done>);
    impl Copier for Handoff {
        fn copy(&mut self, _: &str, done: Done) -> Option<Killer> {
            self.0.send(done).unwrap();
            None
        }
    }
    let answer = thread::spawn(move || {
        let done = rx.recv().unwrap();
        thread::sleep(Duration::from_millis(50));
        done(Err("late".into()));
    });
    let mut opts = options();
    opts.copier = Box::new(Handoff(tx));
    let mut a = app(20, 3, "ab\n", opts);
    assert_eq!(press(&mut a, "Shift+Right Enter"), Flow::Continue);
    assert_eq!(a.status.as_deref(), Some("copy failed: late"));
    answer.join().unwrap();
}
