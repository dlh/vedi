//! The input as bytes, in memory or on disk, with lines decoded on
//! demand from the index.

use std::cell::{Ref, RefCell};
use std::fmt;
use std::fs::File;
use std::io::{self, Write};
use std::ops::Range;
use std::os::unix::fs::FileExt;
use std::sync::Arc;

use crate::text::ansi::{self, Parser, Run};
use crate::text::index::{BLOCK_LINES, Block, Indexed};

/// A line index and a char index within it. `col` may equal the line's
/// length, meaning after its last char.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

/// One line, decoded. `title` is the window title in effect once the
/// line is drawn: set on it, or carried from a line before.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Line {
    pub text: Vec<char>,
    pub runs: Vec<Run>,
    pub title: Option<Arc<str>>,
}

#[derive(Clone, Debug)]
pub enum ReadError {
    Io(Arc<io::Error>),
    /// Indexed bytes are gone: the file shrank.
    Truncated,
    /// What a command source reported.
    Command(String),
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ReadError::Io(e) => e.fmt(f),
            ReadError::Truncated => f.write_str("input truncated"),
            ReadError::Command(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for ReadError {}

pub enum Storage {
    /// The bytes as they came, kept: stdin, or a file that may change.
    Memory(Vec<Arc<[u8]>>),
    /// Files read back in place, one per input.
    Paged(Vec<PagedPart>),
}

pub struct PagedPart {
    pub file: File,
    /// Where the file's bytes begin in the input: its `Progress::Part`'s
    /// offset, which is 0 for the first.
    pub start: u64,
}

/// What the reader thread sends the buffer.
pub enum Progress {
    /// `chunk` is the bytes indexed, kept by a `Memory` buffer.
    Data {
        chunk: Option<Arc<[u8]>>,
        indexed: Indexed,
    },
    /// The next input begins at `offset`.
    Part { offset: u64 },
    Done {
        err: Option<ReadError>,
        trailing_newline: bool,
    },
}

/// Block `block`, decoded when it ended at `end`. A block cut short by
/// a read holds fewer lines than it should, and `err` says why.
struct Decoded {
    block: usize,
    end: u64,
    lines: Vec<Line>,
    err: Option<ReadError>,
}

pub struct Buffer {
    storage: Storage,
    /// Where each `Memory` chunk begins.
    chunk_starts: Vec<u64>,
    held: u64,
    /// The `Paged` parts begun, whose `start` is known.
    started: usize,
    literal: bool,
    n: usize,
    end: u64,
    blocks: Vec<Block>,
    parsers: Vec<Parser>,
    titles: Vec<usize>,
    parts: Vec<usize>,
    done: Option<Result<(), ReadError>>,
    trailing_newline: bool,
    cache: RefCell<Option<Decoded>>,
    read_err: RefCell<Option<ReadError>>,
    #[cfg(test)]
    decodes: std::cell::Cell<usize>,
}

impl Buffer {
    /// A literal buffer decodes lines as their bytes: escapes and
    /// controls are text, with no runs and no titles.
    pub fn new(storage: Storage, literal: bool) -> Buffer {
        let (mut chunk_starts, mut held) = (Vec::new(), 0);
        if let Storage::Memory(chunks) = &storage {
            for c in chunks {
                chunk_starts.push(held);
                held += c.len() as u64;
            }
        }
        Buffer {
            storage,
            chunk_starts,
            held,
            started: 0,
            literal,
            n: 0,
            end: 0,
            blocks: Vec::new(),
            parsers: Vec::new(),
            titles: Vec::new(),
            parts: Vec::new(),
            done: None,
            trailing_newline: true,
            cache: RefCell::new(None),
            read_err: RefCell::new(None),
            #[cfg(test)]
            decodes: std::cell::Cell::new(0),
        }
    }

    /// How many blocks `line` has decoded.
    #[cfg(test)]
    pub fn decodes(&self) -> usize {
        self.decodes.get()
    }

    pub fn apply(&mut self, p: Progress) {
        match p {
            Progress::Data { chunk, indexed } => {
                if let (Some(c), Storage::Memory(chunks)) = (chunk, &mut self.storage)
                    && !c.is_empty()
                {
                    self.chunk_starts.push(self.held);
                    self.held += c.len() as u64;
                    chunks.push(c);
                }
                self.n += indexed.lines;
                self.end = indexed.end;
                self.blocks.extend(indexed.blocks);
                self.parsers.extend(indexed.parsers);
                self.titles.extend(indexed.titled);
            }
            Progress::Part { offset } => {
                // A line the last input left open stays with it.
                let start = self.n + usize::from(offset > self.end);
                if self.parts.is_empty() && start > 0 {
                    self.parts.push(0);
                }
                self.parts.push(start);
                if let Storage::Paged(parts) = &mut self.storage
                    && let Some(p) = parts.get_mut(self.started)
                {
                    p.start = offset;
                }
                self.started += 1;
            }
            Progress::Done {
                err,
                trailing_newline,
            } => {
                self.done = Some(err.map_or(Ok(()), Err));
                self.trailing_newline = trailing_newline;
            }
        }
    }

    pub fn len(&self) -> usize {
        self.n
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    pub fn literal(&self) -> bool {
        self.literal
    }

    pub fn set_literal(&mut self, on: bool) {
        if self.literal != on {
            self.literal = on;
            *self.cache.get_mut() = None;
        }
    }

    /// Line `i` decoded; empty when out of range or its bytes cannot be
    /// read. The guard holds the block cache: another block decoded
    /// while it lives panics.
    pub fn line(&self, i: usize) -> Ref<'_, Line> {
        static EMPTY: Line = Line {
            text: Vec::new(),
            runs: Vec::new(),
            title: None,
        };
        self.cached(i)
            .unwrap_or_else(|| Ref::map(self.cache.borrow(), |_| &EMPTY))
    }

    /// Replaces `dst` with line `i`'s chars, as `line` would give them.
    pub fn text(&self, i: usize, dst: &mut Vec<char>) {
        dst.clear();
        if let Some(l) = self.cached(i) {
            dst.extend_from_slice(&l.text);
        }
    }

    fn cached(&self, i: usize) -> Option<Ref<'_, Line>> {
        if i >= self.n {
            return None;
        }
        let k = i / BLOCK_LINES;
        let end = self.block_end(k);
        // The last block is decoded again as lines join it. The hit test
        // borrows shared, so a hit works while a line guard lives.
        if !matches!(&*self.cache.borrow(), Some(d) if d.block == k && d.end == end) {
            *self.cache.borrow_mut() = Some(self.decode(k));
        }
        let cache = self.cache.borrow();
        match Ref::filter_map(cache, |c| c.as_ref()?.lines.get(i % BLOCK_LINES)) {
            Ok(l) => Some(l),
            Err(c) => {
                let err = c.as_ref().and_then(|d| d.err.clone());
                self.note(err.unwrap_or(ReadError::Truncated));
                None
            }
        }
    }

    fn decode(&self, k: usize) -> Decoded {
        #[cfg(test)]
        self.decodes.set(self.decodes.get() + 1);
        let (start, end) = (self.blocks[k].start, self.block_end(k));
        let mut scratch = Vec::new();
        let (raw, err) = match self.bytes(start, (end - start) as usize, &mut scratch) {
            Ok(raw) => (raw, None),
            Err(e) => (&scratch[..], Some(e)),
        };
        let count = self.block_lines(k).len();
        let mut parser = self.parsers[self.blocks[k].parser as usize].clone();
        let mut title: Option<Arc<str>> = None;
        let mut lines = Vec::with_capacity(count);
        let mut rest = raw;
        while lines.len() < count {
            let line = match rest.iter().position(|&c| c == b'\n') {
                Some(nl) => {
                    let line = rest[..nl].strip_suffix(b"\r").unwrap_or(&rest[..nl]);
                    rest = &rest[nl + 1..];
                    line
                }
                // The input's last line, unterminated.
                None if err.is_none() && !rest.is_empty() => std::mem::take(&mut rest),
                None => break,
            };
            lines.push(self.decode_line(&mut parser, line, &mut title));
        }
        Decoded {
            block: k,
            end,
            lines,
            err,
        }
    }

    /// `title` is the last line's, shared while it stays the same.
    fn decode_line(&self, p: &mut Parser, raw: &[u8], title: &mut Option<Arc<str>>) -> Line {
        let mut line = Line::default();
        if self.literal {
            ansi::literal(raw, &mut line.text);
            return line;
        }
        p.parse(raw, &mut line.text, &mut line.runs);
        if p.title().is_empty() {
            *title = None;
        } else if title.as_deref() != Some(p.title()) {
            *title = Some(p.title().into());
        }
        line.title = title.clone();
        line
    }

    /// Block `k`'s bytes, line endings and escapes kept: for scanning
    /// many lines without decoding any. They are borrowed when they lie
    /// in one chunk in memory, else read into `scratch`, which on error
    /// holds the bytes that could be read.
    pub fn raw_block<'a>(
        &'a self,
        k: usize,
        scratch: &'a mut Vec<u8>,
    ) -> Result<&'a [u8], ReadError> {
        let Some(b) = self.blocks.get(k) else {
            return Ok(&[]);
        };
        let r = self.bytes(b.start, (self.block_end(k) - b.start) as usize, scratch);
        if let Err(e) = &r {
            self.note(e.clone());
        }
        r
    }

    pub fn blocks(&self) -> usize {
        self.blocks.len()
    }

    pub fn block_lines(&self, k: usize) -> Range<usize> {
        let start = (k * BLOCK_LINES).min(self.n);
        start..(start + BLOCK_LINES).min(self.n)
    }

    fn block_end(&self, k: usize) -> u64 {
        self.blocks.get(k + 1).map_or(self.end, |b| b.start)
    }

    /// `len` bytes from `off`, borrowed when one chunk holds them all,
    /// else read into `scratch` as `read_at` would.
    fn bytes<'a>(
        &'a self,
        off: u64,
        len: usize,
        scratch: &'a mut Vec<u8>,
    ) -> Result<&'a [u8], ReadError> {
        if let Storage::Memory(chunks) = &self.storage
            && let Some(i) = self
                .chunk_starts
                .partition_point(|&s| s <= off)
                .checked_sub(1)
        {
            let at = (off - self.chunk_starts[i]) as usize;
            if let Some(b) = chunks[i].get(at..at + len) {
                return Ok(b);
            }
        }
        self.read_at(off, len, scratch)?;
        Ok(scratch)
    }

    /// Replaces `dst` with `len` bytes from `off`, or as many as can be
    /// read.
    fn read_at(&self, off: u64, len: usize, dst: &mut Vec<u8>) -> Result<(), ReadError> {
        dst.clear();
        let want = off + len as u64;
        let mut off = off;
        match &self.storage {
            Storage::Memory(chunks) => {
                while off < want && off < self.held {
                    let Some(i) = self
                        .chunk_starts
                        .partition_point(|&s| s <= off)
                        .checked_sub(1)
                    else {
                        break;
                    };
                    let c = &chunks[i][(off - self.chunk_starts[i]) as usize..];
                    let c = &c[..c.len().min((want - off) as usize)];
                    dst.extend_from_slice(c);
                    off += c.len() as u64;
                }
            }
            Storage::Paged(parts) => {
                // The first part starts at 0 whether or not it was begun.
                let parts = &parts[..self.started.max(1).min(parts.len())];
                while off < want && !parts.is_empty() {
                    let Some(i) = parts.partition_point(|p| p.start <= off).checked_sub(1) else {
                        break;
                    };
                    let next = parts.get(i + 1).map(|p| p.start);
                    let stop = next.map_or(want, |n| n.min(want));
                    off += read_full(&parts[i].file, off - parts[i].start, stop - off, dst)?;
                    if off < stop {
                        // The reader gave a part not ending in "\n" one;
                        // its file has none to read. The last part begun
                        // may be owed it before the next part's offset
                        // arrives, but never once the input is done.
                        let given = match next {
                            Some(n) => n == stop,
                            None => self.done.is_none() && want == self.end,
                        };
                        if given && off + 1 == stop {
                            dst.push(b'\n');
                            off += 1;
                        } else {
                            break;
                        }
                    }
                }
            }
        }
        if off < want {
            return Err(ReadError::Truncated);
        }
        Ok(())
    }

    fn note(&self, e: ReadError) {
        self.read_err.borrow_mut().get_or_insert(e);
    }

    /// Copies the input's bytes to `w` as they came.
    pub fn write_to(&self, w: &mut dyn Write) -> io::Result<()> {
        match &self.storage {
            Storage::Memory(chunks) => chunks.iter().try_for_each(|c| w.write_all(c)),
            Storage::Paged(_) => {
                const PIECE: u64 = 64 * 1024;
                let mut buf = Vec::new();
                let mut off = 0;
                while off < self.end {
                    let len = PIECE.min(self.end - off);
                    self.read_at(off, len as usize, &mut buf)
                        .map_err(io::Error::other)?;
                    w.write_all(&buf)?;
                    off += len;
                }
                Ok(())
            }
        }
    }

    /// Whether the input ended with "\n", which adds no line: tells a
    /// terminated last line from a blank bottom row. True while reading.
    pub fn trailing_newline(&self) -> bool {
        self.trailing_newline
    }

    /// The line each input begins at, ascending: an empty input repeats
    /// the next one's start, and lines before the first input are part 0.
    pub fn parts(&self) -> &[usize] {
        &self.parts
    }

    /// The input line `i` came from, as an index into `parts`.
    pub fn part_at(&self, i: usize) -> usize {
        self.part(i).0
    }

    /// The input line `i` came from, and the lines it holds. An empty
    /// input yields to the next.
    pub fn part(&self, i: usize) -> (usize, Range<usize>) {
        match self.parts.partition_point(|&s| s <= i) {
            0 => (0, 0..self.n),
            k => {
                let end = self.parts.get(k).copied().unwrap_or(self.n);
                (k - 1, self.parts[k - 1]..end)
            }
        }
    }

    pub fn starts_part(&self, i: usize) -> bool {
        self.parts.binary_search(&i).is_ok()
    }

    /// The lines that set a window title, ascending; none when literal.
    pub fn titles(&self) -> &[usize] {
        if self.literal { &[] } else { &self.titles }
    }

    /// `None` while reading; then the reader's error, if any.
    pub fn finished(&self) -> Option<Result<(), ReadError>> {
        self.done.clone()
    }

    /// The first error reading a line hit.
    pub fn read_error(&self) -> Option<ReadError> {
        self.read_err.borrow().clone()
    }
}

/// Appends up to `len` bytes of `file` from `pos` to `dst`, stopping
/// early at its end; returns how many.
fn read_full(file: &File, pos: u64, len: u64, dst: &mut Vec<u8>) -> Result<u64, ReadError> {
    let base = dst.len();
    dst.resize(base + len as usize, 0);
    let mut n = 0;
    while n < len as usize {
        match file.read_at(&mut dst[base + n..], pos + n as u64) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => {
                dst.truncate(base + n);
                return Err(ReadError::Io(Arc::new(e)));
            }
        }
    }
    dst.truncate(base + n);
    Ok(n as u64)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::text::index::Indexer;
    use crate::view::style::{Color, Style};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A buffer and the indexer feeding it, as the reader thread would.
    pub(crate) struct Fill {
        ix: Indexer,
        pub(crate) buf: Buffer,
        paged: bool,
        nl: bool,
    }

    impl Fill {
        pub(crate) fn memory(literal: bool) -> Fill {
            Fill::new(Storage::Memory(Vec::new()), literal)
        }

        pub(crate) fn paged(files: Vec<File>) -> Fill {
            let parts = files
                .into_iter()
                .map(|file| PagedPart { file, start: 0 })
                .collect();
            Fill::new(Storage::Paged(parts), false)
        }

        fn new(storage: Storage, literal: bool) -> Fill {
            let paged = matches!(storage, Storage::Paged(_));
            Fill {
                ix: Indexer::new(),
                buf: Buffer::new(storage, literal),
                paged,
                nl: true,
            }
        }

        pub(crate) fn write(&mut self, p: &[u8]) {
            let indexed = self.ix.feed(p);
            let chunk = (!self.paged).then(|| Arc::from(p));
            self.buf.apply(Progress::Data { chunk, indexed });
            if let Some(&c) = p.last() {
                self.nl = c == b'\n';
            }
        }

        pub(crate) fn part(&mut self) {
            let offset = self.ix.offset();
            self.buf.apply(Progress::Part { offset });
        }

        pub(crate) fn finish(&mut self, err: Option<ReadError>) {
            let indexed = self.ix.finish();
            self.buf.apply(Progress::Data {
                chunk: None,
                indexed,
            });
            let trailing_newline = self.nl && self.ix.offset() > 0;
            self.buf.apply(Progress::Done {
                err,
                trailing_newline,
            });
        }
    }

    pub(crate) fn memory(input: &[u8]) -> Buffer {
        let mut f = Fill::memory(false);
        f.write(input);
        f.finish(None);
        f.buf
    }

    fn lines(b: &Buffer) -> Vec<String> {
        (0..b.len())
            .map(|i| b.line(i).text.iter().collect())
            .collect()
    }

    fn text(l: &Line) -> String {
        l.text.iter().collect()
    }

    pub(crate) fn red() -> Style {
        Style::default().fg(Color::Palette(1))
    }

    /// A temp file, removed when dropped; a file open on it stays readable.
    pub(crate) struct Temp(PathBuf);

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    impl AsRef<Path> for Temp {
        fn as_ref(&self) -> &Path {
            &self.0
        }
    }

    pub(crate) fn temp(data: &[u8]) -> Temp {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("vedi-buffer-{}-{n}", std::process::id()));
        fs::write(&path, data).unwrap();
        Temp(path)
    }

    /// A paged buffer over a temp file holding `data`, indexed in full.
    fn paged(data: &[u8]) -> (Buffer, Temp) {
        let path = temp(data);
        let mut f = Fill::paged(vec![File::open(&path).unwrap()]);
        f.part();
        f.write(data);
        f.finish(None);
        (f.buf, path)
    }

    #[test]
    fn fill_splits_lines() {
        let tests: &[(&str, &[u8], &[&str])] = &[
            ("lf", b"a\nb", &["a", "b"]),
            ("crlf", b"a\r\nb\r\n", &["a", "b"]),
            ("trailing lf no extra line", b"a\n", &["a"]),
            ("bare cr kept", b"x\ry\n", &["x\ry"]),
            ("cr at eof kept", b"x\r", &["x\r"]),
            ("empty input", b"", &[]),
            ("blank lines", b"\n\n", &["", ""]),
        ];
        for (name, input, want) in tests {
            let b = memory(input);
            assert_eq!(lines(&b), *want, "{name}");
            assert!(matches!(b.finished(), Some(Ok(()))), "{name}");
            let (b, _) = paged(input);
            assert_eq!(lines(&b), *want, "{name}: paged");
        }
    }

    #[test]
    fn fill_records_trailing_newline() {
        let tests: &[(&[u8], bool)] = &[
            (b"a\n", true),
            (b"a\r\n", true),
            (b"\n", true),
            (b"a", false),
            (b"a\nb", false),
            (b"", false),
        ];
        for &(input, want) in tests {
            assert_eq!(memory(input).trailing_newline(), want, "{input:?}");
        }
        let f = Fill::memory(false);
        assert!(f.buf.trailing_newline(), "true while reading");
    }

    #[test]
    fn fill_style_carries_across_lines() {
        let b = memory(b"\x1b[31ma\nb");
        let runs = b.line(1).runs.clone();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].style, red());
    }

    #[test]
    fn line_title() {
        let mut input = b"none\n\x1b]2;one\x07a\nb\n".to_vec();
        for _ in 0..2 * BLOCK_LINES {
            input.extend_from_slice(b"more\n");
        }
        input.extend_from_slice(b"\x1b]0;two\x07c\n\x1b]2;\x07d\ne\n");
        let b = memory(&input);
        let n = b.len();
        let want = |i: usize| match i {
            1 | 2 => Some("one"),
            i if i == n / 2 => Some("one"),
            i if i == n - 3 => Some("two"),
            _ => None,
        };
        for i in [n - 1, n - 2, n - 3, n / 2, 2, 1, 0] {
            assert_eq!(b.line(i).title.as_deref(), want(i), "line {i}");
        }
        assert_eq!(b.line(n).title, None);
    }

    #[test]
    fn parts() {
        let mut f = Fill::memory(false);
        f.part();
        f.write(b"a1\na2");
        f.part();
        f.write(b"b1\nb2\n");
        f.part();
        f.part();
        f.write(b"d1\n");
        f.finish(None);
        for (line, want) in [0, 0, 1, 3].into_iter().enumerate() {
            assert_eq!(f.buf.part_at(line), want, "part_at({line})");
        }
        assert_eq!(lines(&f.buf), ["a1", "a2b1", "b2", "d1"]);
    }

    #[test]
    fn titles() {
        let mut f = Fill::memory(false);
        f.write(b"intro\n\x1b]2;hunk\x07@@ one\na\n\x1b]2;hu");
        f.write(b"nk\x07@@ two\n\x1b[31mred\n");
        f.write(b"\x1b]2;\x07cleared");
        assert_eq!(f.buf.titles(), [1, 3], "before EOF");
        f.finish(None);
        assert_eq!(f.buf.titles(), [1, 3, 5]);
    }

    #[test]
    fn parts_before_any_line() {
        let mut f = Fill::memory(false);
        assert_eq!(f.buf.part_at(0), 0, "empty");
        f.write(b"x\n");
        f.part();
        f.write(b"y\n");
        assert_eq!(f.buf.part_at(0), 0);
        assert_eq!(f.buf.part_at(1), 1);
    }

    #[test]
    fn fill_reports_read_error() {
        let mut f = Fill::memory(false);
        f.write(b"ok\npartial");
        f.finish(Some(ReadError::Io(Arc::new(io::Error::other("boom")))));
        assert_eq!(lines(&f.buf), ["ok", "partial"]);
        match f.buf.finished() {
            Some(Err(e)) => assert_eq!(e.to_string(), "boom"),
            other => panic!("finished = {other:?}, want boom"),
        }
    }

    #[test]
    fn unfinished_until_done() {
        let mut f = Fill::memory(false);
        f.write(b"a\n");
        assert!(f.buf.finished().is_none());
    }

    #[test]
    fn line_out_of_range() {
        let b = Fill::memory(false).buf;
        assert_eq!(*b.line(0), Line::default());
        assert!(b.is_empty());
    }

    #[test]
    fn pos_order() {
        let p = |line, col| Pos { line, col };
        assert!(p(0, 5) < p(1, 0));
        assert!(p(1, 0) < p(1, 1));
        assert!(p(1, 1) >= p(1, 1));
    }

    #[test]
    fn write_joins_across_writes() {
        let mut f = Fill::memory(false);
        let long = "x".repeat(100_000);
        f.write(b"ab");
        f.write(format!("c\n{}", &long[..50_000]).as_bytes());
        assert_eq!(lines(&f.buf), ["abc"]);
        f.write(format!("{}\nd", &long[50_000..]).as_bytes());
        assert_eq!(lines(&f.buf), ["abc", long.as_str()]);
        f.finish(None);
        assert_eq!(lines(&f.buf), ["abc", long.as_str(), "d"]);
    }

    #[test]
    fn write_crlf_across_writes() {
        let mut f = Fill::memory(false);
        f.write(b"a\r");
        f.write(b"\nb\rc\nd\r");
        f.finish(None);
        assert_eq!(lines(&f.buf), ["a", "b\rc", "d\r"]);
    }

    #[test]
    fn state_carries_out_of_order() {
        let b = memory(b"\x1b[31ma\n\x1b]8;;http://x\x1b\\b\ncut \x1b[3\nc\n\x1b[0md");
        let url = |r: &Run| r.link.as_ref().map(|l| l.url.to_string());
        let runs = b.line(3).runs.clone();
        assert_eq!(runs.len(), 1, "line 3 (read first)");
        assert_eq!(
            (runs[0].style, url(&runs[0])),
            (red(), Some("http://x".into()))
        );
        let runs = b.line(4).runs.clone();
        assert_eq!(runs.len(), 1, "line 4");
        assert_eq!(
            (runs[0].style, url(&runs[0])),
            (Style::default(), Some("http://x".into()))
        );
        let runs = b.line(0).runs.clone();
        assert_eq!(runs.len(), 1, "line 0");
        assert_eq!(runs[0].style, red());
        assert_eq!(text(&b.line(2)), "cut ");
    }

    #[test]
    fn lines_read_right_on_every_pass() {
        let input: String = (0..1024 + 5).map(|i| format!("{i}\n")).collect();
        let b = memory(input.as_bytes());
        for pass in 0..2 {
            for i in 0..b.len() {
                assert_eq!(text(&b.line(i)), i.to_string(), "pass {pass}");
            }
        }
    }

    #[test]
    fn paged_reads_lines_from_the_file() {
        let (b, _) = paged(b"a\n\x1b[1mb\r\nc");
        assert_eq!(lines(&b), ["a", "b", "c"]);
        let runs = b.line(1).runs.clone();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].style, Style::default().bold());
        assert!(!b.trailing_newline());
    }

    #[test]
    fn paged_parts_read_their_files() {
        // The reader gives "a" its newline; the file has none to read.
        let (a, b, c) = (temp(b"a"), temp(b""), temp(b"b\nc"));
        let files = [&a, &b, &c].map(|p| File::open(p).unwrap());
        let mut f = Fill::paged(files.into());
        f.part();
        f.write(b"a\n");
        f.part();
        f.part();
        f.write(b"b\nc");
        f.finish(None);
        assert_eq!(lines(&f.buf), ["a", "b", "c"]);
        assert_eq!(f.buf.parts(), [0, 1, 1]);
        assert_eq!(f.buf.raw_block(0, &mut Vec::new()).unwrap(), b"a\nb\nc");
        assert!(f.buf.read_error().is_none());
    }

    #[test]
    fn text_decodes_into_scratch() {
        let b = memory(b"\x1b[31mab\ncd");
        let mut dst = vec!['x'];
        b.text(1, &mut dst);
        assert_eq!(dst, ['c', 'd']);
        b.text(0, &mut dst);
        assert_eq!(dst, ['a', 'b']);
        b.text(7, &mut dst);
        assert!(dst.is_empty(), "out of range");
    }

    #[test]
    fn read_failure_gives_empty_line() {
        // A directory opens, but reading it fails.
        let mut f = Fill::paged(vec![File::open(std::env::temp_dir()).unwrap()]);
        f.write(b"abc\n");
        f.finish(None);
        assert_eq!(f.buf.len(), 1);
        assert_eq!(*f.buf.line(0), Line::default());
        let mut dst = Vec::new();
        f.buf.text(0, &mut dst);
        assert!(dst.is_empty());
        assert!(
            matches!(f.buf.read_error(), Some(ReadError::Io(_))),
            "the file's error, not truncation: {:?}",
            f.buf.read_error()
        );
    }

    #[test]
    fn short_block_keeps_its_lines() {
        let (b, path) = paged(b"one\ntwo\nthree\n");
        fs::write(&path, b"one\ntwo\n").unwrap();
        assert_eq!(text(&b.line(0)), "one");
        assert!(
            b.read_error().is_none(),
            "after reading a line that is there"
        );
        assert_eq!(*b.line(2), Line::default());
        assert!(matches!(b.read_error(), Some(ReadError::Truncated)));
    }

    #[test]
    fn rewritten_block_is_error() {
        let (b, path) = paged(b"a\nb\n");
        fs::write(&path, b"abcd").unwrap();
        assert_eq!(*b.line(1), Line::default());
        assert!(matches!(b.read_error(), Some(ReadError::Truncated)));
    }

    #[test]
    fn short_block_stays_decoded() {
        let input: String = (0..2 * BLOCK_LINES).map(|i| format!("{i}\n")).collect();
        let (b, path) = paged(input.as_bytes());
        fs::write(&path, &input.as_bytes()[..input.len() / 4]).unwrap();
        let mut dst = Vec::new();
        for i in 0..BLOCK_LINES {
            b.text(i, &mut dst);
            b.line(i);
        }
        // Restored, the block is not read again: the short one stays decoded.
        fs::write(&path, &input).unwrap();
        assert_eq!(*b.line(BLOCK_LINES - 1), Line::default());
    }

    #[test]
    fn truncated_file_reports_error() {
        let input: String = (0..200).map(|i| format!("line {i}\n")).collect();
        let (b, path) = paged(input.as_bytes());
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(10)
            .unwrap();
        assert_eq!(*b.line(150), Line::default());
        assert!(matches!(b.read_error(), Some(ReadError::Truncated)));
        assert_eq!(ReadError::Truncated.to_string(), "input truncated");
    }

    #[test]
    fn write_to_round_trip() {
        let input = b"\x1b[31ma\r\nb\n\nc";
        let (p, _) = paged(input);
        for (name, b) in [("memory", memory(input)), ("paged", p)] {
            let mut out = Vec::new();
            b.write_to(&mut out).unwrap();
            assert_eq!(out, input, "{name}");
        }
    }

    #[test]
    fn lines_across_blocks() {
        let input: String = (0..3 * BLOCK_LINES + 5)
            .map(|i| match i {
                i if i == BLOCK_LINES - 1 => format!("\x1b[31m{i}\n"),
                i => format!("{i}\n"),
            })
            .collect();
        let (p, _) = paged(input.as_bytes());
        for (name, b) in [("memory", memory(input.as_bytes())), ("paged", p)] {
            let order = [
                2 * BLOCK_LINES + 1,
                0,
                BLOCK_LINES,
                3 * BLOCK_LINES + 4,
                BLOCK_LINES - 1,
                2 * BLOCK_LINES - 1,
                BLOCK_LINES + 1,
                3 * BLOCK_LINES,
            ];
            for i in order {
                let l = b.line(i);
                assert_eq!(text(&l), i.to_string(), "{name}: line {i}");
                if i >= BLOCK_LINES - 1 {
                    assert_eq!(l.runs.len(), 1, "{name}: line {i}");
                    assert_eq!(l.runs[0].style, red(), "{name}: line {i}");
                }
            }
        }
    }

    #[test]
    fn last_block_grows() {
        let mut f = Fill::memory(false);
        for i in 0..BLOCK_LINES + 3 {
            f.write(i.to_string().as_bytes());
            assert_eq!(f.buf.len(), i, "before line {i}'s newline");
            f.write(b"\n");
            assert_eq!(f.buf.len(), i + 1, "after line {i}");
            assert_eq!(text(&f.buf.line(i)), i.to_string());
        }
        f.write(b"partial");
        assert_eq!(f.buf.len(), BLOCK_LINES + 3, "partial line pending");
        f.finish(None);
        assert_eq!(text(&f.buf.line(BLOCK_LINES + 3)), "partial");
    }

    #[test]
    fn short_read_is_error() {
        let path = temp(b"one\ntwo\n");
        let mut f = Fill::paged(vec![File::open(&path).unwrap()]);
        f.write(b"one\ntwo\n");
        fs::write(&path, b"one\nt").unwrap();
        assert_eq!(*f.buf.line(1), Line::default());
        assert_eq!(text(&f.buf.line(0)), "one", "its bytes are still there");
        f.finish(None);
        assert!(matches!(f.buf.finished(), Some(Ok(()))));
        assert!(matches!(f.buf.read_error(), Some(ReadError::Truncated)));
    }

    #[test]
    fn parts_and_starts_part() {
        let mut f = Fill::memory(false);
        assert!(f.buf.parts().is_empty());
        f.part();
        f.write(b"a1\na2");
        f.part();
        f.write(b"b1\nb2\n");
        f.part();
        f.part();
        f.write(b"d1\n");
        f.finish(None);
        assert_eq!(f.buf.parts(), [0, 2, 3, 3]);
        for (line, want) in [true, false, true, true, false].into_iter().enumerate() {
            assert_eq!(f.buf.starts_part(line), want, "starts_part({line})");
        }
    }

    #[test]
    fn part() {
        let mut f = Fill::memory(false);
        f.part();
        f.write(b"a1\na2");
        f.part();
        f.write(b"b1\nb2\n");
        f.part();
        f.part();
        f.write(b"d1\n");
        f.finish(None);
        for (line, want) in [
            (0, (0, 0..2)),
            (1, (0, 0..2)),
            (2, (1, 2..3)),
            (3, (3, 3..4)),
        ] {
            assert_eq!(f.buf.part(line), want, "part({line})");
        }
        assert_eq!(memory(b"x\ny\n").part(1), (0, 0..2), "no parts");
        assert_eq!(Fill::memory(false).buf.part(0), (0, 0..0), "empty");
    }

    #[test]
    fn raw_block_is_its_bytes() {
        let first: String = (0..BLOCK_LINES)
            .map(|i| format!("\x1b[31m{i}\r\n"))
            .collect();
        let second = "x\ny";
        let b = memory(format!("{first}{second}").as_bytes());
        assert_eq!(b.blocks(), 2);
        assert_eq!(b.block_lines(1), BLOCK_LINES..BLOCK_LINES + 2);
        let mut dst = Vec::new();
        assert_eq!(b.raw_block(0, &mut dst).unwrap(), first.as_bytes());
        assert_eq!(b.raw_block(1, &mut dst).unwrap(), second.as_bytes());
        assert!(b.raw_block(2, &mut dst).unwrap().is_empty(), "out of range");
    }

    #[test]
    fn raw_block_borrows_within_a_chunk() {
        let mut f = Fill::memory(false);
        f.write(b"ab\n");
        f.write(b"cd\n");
        f.finish(None);
        let mut scratch = b"junk".to_vec();
        assert_eq!(f.buf.raw_block(0, &mut scratch).unwrap(), b"ab\ncd\n");
        assert_eq!(scratch, b"ab\ncd\n", "straddling chunks: copied");

        let b = memory(b"ab\ncd\n");
        let mut scratch = b"junk".to_vec();
        assert_eq!(b.raw_block(0, &mut scratch).unwrap(), b"ab\ncd\n");
        assert_eq!(scratch, b"junk", "one chunk: borrowed");

        let (b, _path) = paged(b"ab\ncd\n");
        let mut scratch = b"junk".to_vec();
        assert_eq!(b.raw_block(0, &mut scratch).unwrap(), b"ab\ncd\n");
        assert_eq!(scratch, b"ab\ncd\n", "paged: read");
    }

    #[test]
    fn raw_block_of_short_block_is_error() {
        let (b, path) = paged(b"one\ntwo\nthree\n");
        fs::write(&path, b"one\ntwo\n").unwrap();
        let mut dst = Vec::new();
        assert!(matches!(
            b.raw_block(0, &mut dst),
            Err(ReadError::Truncated)
        ));
        assert_eq!(dst, b"one\ntwo\n", "the bytes left");
        assert!(matches!(b.read_error(), Some(ReadError::Truncated)));
    }

    #[test]
    fn literal() {
        let input = b"\x1b]2;t\x07\x1b[31mred\nmore\x1b[0m\n";
        let b = memory(input);
        let l = b.line(1);
        assert_eq!(
            (text(&l), l.runs.len(), l.title.as_deref()),
            ("more".into(), 1, Some("t"))
        );
        assert_eq!(b.titles(), [0]);
        assert!(!b.literal());

        let mut f = Fill::memory(true);
        f.write(input);
        f.finish(None);
        let b = f.buf;
        assert!(b.literal());
        for (i, want) in ["\x1b]2;t\x07\x1b[31mred", "more\x1b[0m"]
            .into_iter()
            .enumerate()
        {
            let l = b.line(i);
            assert_eq!(
                (text(&l), l.runs.len(), l.title.clone()),
                (want.into(), 0, None)
            );
            let mut dst = Vec::new();
            b.text(i, &mut dst);
            assert_eq!(dst.iter().collect::<String>(), want);
        }
        assert!(b.titles().is_empty());
    }

    #[test]
    fn paged_given_newline_before_next_part() {
        let (a, b) = (temp(b"a"), temp(b"b\n"));
        let mut f = Fill::paged(vec![File::open(&a).unwrap(), File::open(&b).unwrap()]);
        f.part();
        f.write(b"a\n");
        assert_eq!(text(&f.buf.line(0)), "a");
        assert!(f.buf.read_error().is_none(), "{:?}", f.buf.read_error());
        f.part();
        f.write(b"b\n");
        f.finish(None);
        assert_eq!(lines(&f.buf), ["a", "b"]);
        assert!(f.buf.read_error().is_none());
    }

    #[test]
    fn last_part_owes_no_newline_once_done() {
        let path = temp(b"a\nb");
        let mut f = Fill::paged(vec![File::open(&path).unwrap()]);
        f.part();
        f.write(b"a\nbc");
        f.finish(None);
        assert_eq!(*f.buf.line(1), Line::default());
        assert!(matches!(f.buf.read_error(), Some(ReadError::Truncated)));
    }

    #[test]
    fn set_literal_redecodes() {
        let mut b = memory(b"\x1b[31mred\x1b[0m\n");
        let probe = |b: &Buffer| {
            let l = b.line(0);
            (text(&l), l.runs.len())
        };
        assert_eq!(probe(&b), ("red".into(), 1));
        b.set_literal(true);
        assert_eq!(probe(&b), ("\x1b[31mred\x1b[0m".into(), 0));
        b.set_literal(false);
        assert_eq!(probe(&b), ("red".into(), 1));
    }

    #[test]
    fn memory_reads_span_chunks() {
        let want: Vec<u8> = b"0123456789".repeat(100);
        let mut f = Fill::memory(false);
        f.write(&want[..497]);
        f.write(&want[497..503]);
        f.write(&want[503..]);
        let mut got = Vec::new();
        f.buf.read_at(0, want.len(), &mut got).unwrap();
        assert_eq!(got, want);
        f.buf.read_at(497, 6, &mut got).unwrap();
        assert_eq!(got, &want[497..503]);
        let end = want.len() as u64;
        assert!(matches!(
            f.buf.read_at(end - 2, 6, &mut got),
            Err(ReadError::Truncated)
        ));
        assert_eq!(got, &want[want.len() - 2..]);
        assert!(matches!(
            f.buf.read_at(end + 1, 6, &mut got),
            Err(ReadError::Truncated)
        ));
        assert!(got.is_empty());
    }
}
