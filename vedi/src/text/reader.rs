//! Reads the input on a thread of its own, indexing it as it comes.

use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crate::text::buffer::{Progress, ReadError};
use crate::text::index::Indexer;
use crate::text::source::{Chunk, Concat};

/// Which read a message belongs to: a reopened input starts a new one,
/// and messages from the old are dropped.
pub type Gen = u32;

// An `Arc`'d chunk carries two counts; with them it fills 64 KiB
// exactly, where macOS's malloc would round 64 KiB and more up to 80.
const READ_SIZE: usize = 64 * 1024 - 2 * size_of::<usize>();

/// Reads `src` to its end, sending `sink` each chunk indexed, each
/// part boundary, and `Done`. A paged buffer reads its files in place,
/// so no bytes are sent. Stops early once `sink` returns `false`.
/// That is noticed only after a read returns, so the handle must not be
/// joined while a source, such as a tty, may block: the process exits
/// with the thread still waiting.
pub fn spawn(
    mut src: Concat,
    paged: bool,
    generation: Gen,
    mut sink: impl FnMut(Gen, Progress) -> bool + Send + 'static,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("vedi-reader".into())
        .spawn(move || {
            let mut ix = Indexer::new();
            let mut buf = vec![0; READ_SIZE];
            let mut trailing_newline = false;
            let err = loop {
                let p = match src.read(&mut buf) {
                    Ok(Chunk::Data(n)) => {
                        let data = &buf[..n];
                        trailing_newline = data.last() == Some(&b'\n');
                        Progress::Data {
                            indexed: ix.feed(data),
                            chunk: (!paged).then(|| Arc::from(data)),
                        }
                    }
                    Ok(Chunk::Part) => Progress::Part {
                        offset: ix.offset(),
                    },
                    Ok(Chunk::Eof) => break src.read_error(),
                    Err(e) => break Some(ReadError::Io(Arc::new(e))),
                };
                if !sink(generation, p) {
                    return;
                }
            };
            let indexed = ix.finish();
            if sink(
                generation,
                Progress::Data {
                    chunk: None,
                    indexed,
                },
            ) {
                sink(
                    generation,
                    Progress::Done {
                        err,
                        trailing_newline,
                    },
                );
            }
        })
        .expect("spawn reader thread")
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io;
    use std::sync::mpsc;

    use super::*;
    use crate::text::buffer::tests::{red, temp};
    use crate::text::buffer::{Buffer, PagedPart, Storage};
    use crate::text::source::Source;
    use crate::text::source::tests::mem;

    fn run(parts: Vec<Box<dyn Source>>, paged: bool) -> Vec<Progress> {
        let (tx, rx) = mpsc::channel();
        spawn(Concat::new(parts), paged, 7, move |g, p| {
            assert_eq!(g, 7);
            tx.send(p).is_ok()
        })
        .join()
        .unwrap();
        rx.into_iter().collect()
    }

    fn done(ps: &[Progress]) -> (Option<&ReadError>, bool) {
        match ps.last() {
            Some(Progress::Done {
                err,
                trailing_newline,
            }) => (err.as_ref(), *trailing_newline),
            _ => panic!("last message is not Done"),
        }
    }

    fn lines(b: &Buffer) -> Vec<String> {
        (0..b.len())
            .map(|i| b.line(i).text.iter().collect())
            .collect()
    }

    #[test]
    fn reader_sends_done_with_trailing_newline() {
        assert!(matches!(done(&run(vec![mem("a\n")], false)), (None, true)));
        assert!(matches!(done(&run(vec![mem("a")], false)), (None, false)));
        assert!(matches!(done(&run(vec![mem("")], false)), (None, false)));
    }

    #[test]
    fn reader_paged_sends_no_bytes() {
        let ps = run(vec![mem("a\nb\n")], true);
        let mut lines = 0;
        for p in &ps {
            if let Progress::Data { chunk, indexed } = p {
                assert!(chunk.is_none());
                lines += indexed.lines;
            }
        }
        assert_eq!(lines, 2);
    }

    #[test]
    fn reader_sends_data_before_part_and_never_mixes_parts() {
        let ps = run(vec![mem("a\nb"), mem("c\n")], false);
        let mut seen = String::new();
        let mut parts = Vec::new();
        for p in &ps {
            match p {
                Progress::Data { chunk: Some(c), .. } => {
                    seen.push_str(std::str::from_utf8(c).unwrap())
                }
                Progress::Part { offset } => parts.push((seen.clone(), *offset)),
                _ => {}
            }
        }
        // Part 0's bytes, its given newline included, all precede part 1.
        assert_eq!(parts, [(String::new(), 0), ("a\nb\n".into(), 4)]);
        assert_eq!(seen, "a\nb\nc\n");
    }

    #[test]
    fn reader_reports_a_command_error() {
        struct Failing;
        impl Source for Failing {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Ok(0)
            }
            fn read_error(&mut self) -> Option<ReadError> {
                Some(ReadError::Command("boom".into()))
            }
        }
        let ps = run(vec![Box::new(Failing)], false);
        assert!(matches!(done(&ps), (Some(ReadError::Command(s)), false) if s == "boom"));
    }

    #[test]
    fn reader_stops_when_the_sink_is_gone() {
        struct Endless;
        impl Source for Endless {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                buf.fill(b'\n');
                Ok(buf.len())
            }
        }
        let mut calls = 0;
        spawn(
            Concat::new(vec![Box::new(Endless)]),
            false,
            0,
            move |_, _| {
                calls += 1;
                assert_eq!(calls, 1);
                false
            },
        )
        .join()
        .unwrap();
    }

    // SGR state carries from one input into the next.
    #[test]
    fn reader_fills_a_buffer() {
        let mut b = Buffer::new(Storage::Memory(Vec::new()), false);
        for p in run(vec![mem("\x1b[31mone"), mem("\ntwo\n")], false) {
            b.apply(p);
        }
        assert_eq!(lines(&b), ["one", "", "two"]);
        assert_eq!(b.line(0).runs.len(), 1);
        assert_eq!(b.line(0).runs[0].style, red());
        assert_eq!(b.line(2).runs, b.line(0).runs);
    }

    // Each file is read back in place, the newline given the first
    // included, and SGR state carries across them.
    #[test]
    fn reader_fills_a_paged_buffer() {
        let (a, c) = (temp(b"\x1b[31mone"), temp(b"\ntwo\n"));
        let open = || [&a, &c].map(|p| File::open(p).unwrap());
        let parts = open()
            .into_iter()
            .map(|file| PagedPart { file, start: 0 })
            .collect();
        let mut b = Buffer::new(Storage::Paged(parts), false);
        let sources = open()
            .into_iter()
            .map(|f| Box::new(f) as Box<dyn Source>)
            .collect();
        for p in run(sources, true) {
            b.apply(p);
        }
        assert_eq!(lines(&b), ["one", "", "two"]);
        assert!(b.read_error().is_none(), "{:?}", b.read_error());
        assert_eq!(b.line(0).runs[0].style, red());
        assert_eq!(b.line(2).runs, b.line(0).runs);
    }
}
