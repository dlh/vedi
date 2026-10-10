//! Laying lines out and drawing frames.

use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use vedi::view::layout::{Layout, Mode};
use vedi::view::screen::{Grid, Screen, TestTerminal};
use vedi::view::style::{Color, Style};

const W: usize = 200;
const H: usize = 60;

/// Go's `BenchmarkCells` lines: ASCII skips the segmenter, the rest
/// run it.
const KINDS: [(&str, &str, usize); 6] = [
    ("ascii", "some plain text, about eighty bytes wide, ", 2),
    ("latin", "café naïve résumé façade coöperate ", 2),
    ("cjk", "日本語のテキスト、中文文本 ", 4),
    ("emoji", "👨\u{200d}👩\u{200d}👧\u{200d}👦 🇺🇸 👍🏽 ", 6),
    ("marks", "e\u{301}a\u{308}o\u{302} ", 10),
    ("tabs", "a\tbb\tccc\t", 6),
];

fn cells(c: &mut Criterion) {
    let mut g = c.benchmark_group("layout/cells");
    for (name, text, n) in KINDS {
        let text: Vec<char> = text.repeat(n).chars().collect();
        g.bench_function(name, |b| b.iter(|| Layout::default().cells(&text)));
    }
    g.finish();
}

fn line(c: &mut Criterion) {
    let mixed: String = KINDS[1..5].iter().map(|k| k.1).collect();
    let mut g = c.benchmark_group("layout/line");
    for (name, unit) in [("ascii", KINDS[0].1), ("mixed", mixed.as_str())] {
        let text: Vec<char> = unit.chars().cycle().take(1 << 16).collect();
        for mode in [Mode::Wrap, Mode::NoWrap] {
            let l = Layout {
                width: 80,
                mode,
                ..Layout::default()
            };
            g.bench_function(BenchmarkId::new(name, format!("{mode:?}")), |b| {
                b.iter(|| l.line(&text))
            });
        }
    }
    g.finish();
}

fn frame() -> Grid {
    let mut g = Grid::new(W, H);
    fill(&mut g);
    g
}

fn fill(g: &mut Grid) {
    let styles = [
        Style::default(),
        Style::default().fg(Color::Palette(2)),
        Style::default().bold(),
    ];
    let mut chars = "some plain text about sixty bold columns wide "
        .chars()
        .cycle();
    let mut buf = [0; 4];
    for y in 0..H {
        for x in 0..W {
            let c = chars.next().unwrap().encode_utf8(&mut buf);
            g.put(x, y, c, styles[(x / 7) % styles.len()], None);
        }
    }
}

fn grid(c: &mut Criterion) {
    c.bench_function("grid/put_frame", |b| {
        b.iter_batched_ref(|| Grid::new(W, H), fill, BatchSize::SmallInput)
    });
}

fn present(c: &mut Criterion) {
    let a = frame();
    let mut changed = a.clone();
    changed.put(W / 2, H / 2, "#", Style::default().reverse(), None);
    let mut g = c.benchmark_group("screen/present");
    g.bench_function("unchanged", |b| {
        let mut s = Screen::new(TestTerminal::new(W, H));
        s.present(&a, None);
        b.iter(|| s.present(black_box(&a), None))
    });
    g.bench_function("one_cell", |b| {
        let mut s = Screen::new(TestTerminal::new(W, H));
        let mut flip = false;
        b.iter(|| {
            flip = !flip;
            s.present(if flip { &changed } else { &a }, None);
            // Unread, the draws would grow for the whole run.
            s.terminal_mut().take_draws()
        })
    });
    g.finish();
}

criterion_group!(benches, cells, line, grid, present);
criterion_main!(benches);
