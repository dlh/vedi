//! Parsing, indexing, decoding and searching a large colored input.

use std::hint::black_box;
use std::sync::Arc;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use vedi::text::ansi::Parser;
use vedi::text::buffer::{Buffer, Pos, Progress, Storage};
use vedi::text::index::{BLOCK_LINES, Indexer};
use vedi::text::search::{self, Matcher};

const LINES: usize = 100_000;
/// The reader's buffer, so the indexer sees chunks as it does live.
const CHUNK: usize = 64 << 10;

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

/// `input`'s lines as a terminal capture writes them: blank cells as
/// cursor forwards, ESC [ n C, and a rule as one char and a repeat,
/// ESC [ n b.
fn captured(n: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for i in 1..=n {
        out.extend_from_slice(
            format!(
                "{i:08}:\x1b[2C\x1b[32msome\x1b[1Cplain\x1b[1Ctext\x1b[1Cabout\x1b[0m\x1b[1Csixty\x1b[1C\x1b[1mbold\x1b[0m\x1b[1Ccolumns\x1b[1Cwide\x1b[1C─\x1b[9b\n"
            )
            .as_bytes(),
        );
    }
    out
}

fn buffer(input: &[u8]) -> Buffer {
    let mut ix = Indexer::new();
    let mut b = Buffer::new(Storage::Memory(Vec::new()), false);
    let indexed = ix.feed(input);
    b.apply(Progress::Data {
        chunk: Some(Arc::from(input)),
        indexed,
    });
    b.apply(Progress::Done {
        err: None,
        trailing_newline: true,
    });
    b
}

fn parse(c: &mut Criterion) {
    let line = &input(1)[..];
    let line = &line[..line.len() - 1];
    let (mut text, mut runs) = (Vec::new(), Vec::new());
    // A fresh parser each time, so every iteration parses from the same
    // state.
    c.bench_function("ansi/parse", |b| {
        b.iter(|| Parser::default().parse(black_box(line), &mut text, &mut runs))
    });
    let line = &captured(1)[..];
    let line = &line[..line.len() - 1];
    Parser::default().parse(line, &mut text, &mut runs);
    assert_eq!(
        text.iter().collect::<String>(),
        "00000001:  some plain text about sixty bold columns wide ──────────"
    );
    c.bench_function("ansi/parse_captured", |b| {
        b.iter(|| Parser::default().parse(black_box(line), &mut text, &mut runs))
    });
}

fn index(c: &mut Criterion) {
    let input = input(LINES);
    let mut g = c.benchmark_group("index");
    g.throughput(Throughput::Bytes(input.len() as u64));
    g.bench_function("feed", |b| {
        b.iter(|| {
            let mut ix = Indexer::new();
            for chunk in input.chunks(CHUNK) {
                black_box(ix.feed(chunk));
            }
            ix.finish()
        })
    });
    g.finish();
}

fn decode(c: &mut Criterion) {
    let buf = buffer(&input(LINES));
    let blocks = buf.blocks();
    let mut k = 0;
    // The cache holds one block, so each line from the next block
    // decodes it whole.
    c.bench_function("buffer/decode_block", |b| {
        b.iter(|| {
            k = (k + 1) % blocks;
            buf.line(k * BLOCK_LINES)
        })
    });
}

fn search(c: &mut Criterion) {
    let buf = buffer(&input(LINES));
    let mut g = c.benchmark_group("search");
    // "zzz" rules every block out by its bytes; the x of "some texts" is
    // on every line, so each line is checked.
    for (name, pat) in [("miss", "zzz"), ("miss_every_line", "some texts")] {
        let m = Matcher::new(pat);
        g.bench_function(name, |b| {
            b.iter(|| assert!(search::next(&buf, &m, Pos::default(), false).is_none()))
        });
    }
    // As miss_every_line, with each space a cursor forward, which every
    // line's strip turns back into one.
    let buf = buffer(&captured(LINES));
    let m = Matcher::new("some texts");
    g.bench_function("miss_every_line_captured", |b| {
        b.iter(|| assert!(search::next(&buf, &m, Pos::default(), false).is_none()))
    });
    g.finish();
}

criterion_group!(benches, parse, index, decode, search);
criterion_main!(benches);
