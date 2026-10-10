//! The inputs, read one after another as one.

use std::fs::File;
use std::io::{self, Read, Stdin};

use crate::text::buffer::ReadError;

/// One input: stdin, a file, or a command's output.
pub trait Source: Send {
    /// `Ok(0)` is the end.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;

    /// What went wrong, learned only at the end: a command's exit
    /// status.
    fn read_error(&mut self) -> Option<ReadError> {
        None
    }
}

impl Source for Stdin {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        Read::read(self, buf)
    }
}

impl Source for File {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        Read::read(self, buf)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Chunk {
    Data(usize),
    /// The next part begins, the first included.
    Part,
    Eof,
}

/// Parts one after another, each but the last ending in a newline: a
/// part that ends mid-line is given one, so the next part's first line
/// is its own; the last ends as it ends, so a text whose last row was
/// not blank is known not to have been.
pub struct Concat {
    parts: Vec<Box<dyn Source>>,
    cur: usize,
    /// The last byte read from the current part; `\n` before any.
    last: u8,
    /// The next read announces part `cur`.
    part_due: bool,
    /// What the part that ended the input reported.
    err: Option<ReadError>,
}

impl Concat {
    pub fn new(parts: Vec<Box<dyn Source>>) -> Concat {
        Concat {
            part_due: !parts.is_empty(),
            parts,
            cur: 0,
            last: b'\n',
            err: None,
        }
    }

    pub fn read(&mut self, buf: &mut [u8]) -> io::Result<Chunk> {
        // A source's 0 for an empty `buf` would read as its end.
        if buf.is_empty() {
            return Ok(Chunk::Data(0));
        }
        loop {
            if self.part_due {
                self.part_due = false;
                return Ok(Chunk::Part);
            }
            let Some(src) = self.parts.get_mut(self.cur) else {
                return Ok(Chunk::Eof);
            };
            let n = match src.read(buf) {
                Ok(n) => n,
                // A signal, such as a resize, is no reason to stop.
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
            if n > 0 {
                self.last = buf[n - 1];
                return Ok(Chunk::Data(n));
            }
            if let Some(e) = src.read_error() {
                self.err = Some(e);
                self.cur = self.parts.len();
                return Ok(Chunk::Eof);
            }
            self.cur += 1;
            let last = std::mem::replace(&mut self.last, b'\n');
            if self.cur < self.parts.len() {
                self.part_due = true;
                if last != b'\n' {
                    buf[0] = b'\n';
                    return Ok(Chunk::Data(1));
                }
            }
        }
    }

    /// The error a part reported at its end, which ended the input.
    pub fn read_error(&mut self) -> Option<ReadError> {
        self.err.take()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A part read `step` bytes at a time.
    pub(crate) struct Mem {
        data: &'static [u8],
        step: usize,
    }

    impl Source for Mem {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = buf.len().min(self.step).min(self.data.len());
            buf[..n].copy_from_slice(&self.data[..n]);
            self.data = &self.data[n..];
            Ok(n)
        }
    }

    pub(crate) fn mem(data: &'static str) -> Box<dyn Source> {
        Box::new(Mem {
            data: data.as_bytes(),
            step: usize::MAX,
        })
    }

    /// Reads `c` with a buffer of `size`, writing `|` for each part
    /// boundary.
    fn read_all(c: &mut Concat, size: usize) -> String {
        let mut buf = vec![0; size];
        let mut out = String::new();
        loop {
            match c.read(&mut buf).unwrap() {
                Chunk::Data(n) => out.push_str(std::str::from_utf8(&buf[..n]).unwrap()),
                Chunk::Part => out.push('|'),
                Chunk::Eof => return out,
            }
        }
    }

    // Each part is announced before its first byte. An unterminated
    // part is given the newline it lacks so the next starts a line of
    // its own; the last is kept as it ended; an empty part adds nothing
    // but its boundary.
    #[test]
    fn concat_joins_parts() {
        let mut c = Concat::new(vec![mem("ab"), mem(""), mem("c\nd")]);
        assert_eq!(read_all(&mut c, 64), "|ab\n||c\nd");
    }

    #[test]
    fn concat_keeps_last_ending() {
        let mut c = Concat::new(vec![mem("ab\ncd")]);
        assert_eq!(read_all(&mut c, 64), "|ab\ncd");
    }

    #[test]
    fn concat_of_nothing_is_eof() {
        let mut c = Concat::new(Vec::new());
        assert_eq!(read_all(&mut c, 64), "");
    }

    #[test]
    fn concat_gives_no_newline_to_a_part_that_has_one() {
        let mut c = Concat::new(vec![mem("a\n"), mem("b\n")]);
        assert_eq!(read_all(&mut c, 64), "|a\n|b\n");
    }

    // A part whose last bytes fill the buffer gets its newline from the
    // next read, before the next part begins.
    #[test]
    fn concat_newline_after_a_full_buffer() {
        let mut c = Concat::new(vec![mem("ab"), mem("c\n")]);
        let mut buf = [0; 2];
        assert_eq!(c.read(&mut buf).unwrap(), Chunk::Part);
        assert_eq!(c.read(&mut buf).unwrap(), Chunk::Data(2));
        assert_eq!(&buf, b"ab");
        assert_eq!(c.read(&mut buf).unwrap(), Chunk::Data(1));
        assert_eq!(buf[0], b'\n');
        assert_eq!(c.read(&mut buf).unwrap(), Chunk::Part);
        assert_eq!(read_all(&mut c, 2), "c\n");
    }

    #[test]
    fn concat_reads_parts_in_small_steps() {
        let one = Box::new(Mem {
            data: b"ab",
            step: 1,
        });
        let mut c = Concat::new(vec![one, mem(""), mem("c\nd")]);
        assert_eq!(read_all(&mut c, 1), "|ab\n||c\nd");
    }

    struct Failing;

    impl Source for Failing {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Ok(0)
        }

        fn read_error(&mut self) -> Option<ReadError> {
            Some(ReadError::Command("exit status 1".into()))
        }
    }

    // A command's failure ends the input, as a read error would.
    #[test]
    fn concat_stops_at_a_part_read_error() {
        let mut c = Concat::new(vec![mem("a"), Box::new(Failing), mem("b")]);
        assert_eq!(read_all(&mut c, 64), "|a\n|");
        assert!(matches!(c.read_error(), Some(ReadError::Command(s)) if s == "exit status 1"));
    }
}
