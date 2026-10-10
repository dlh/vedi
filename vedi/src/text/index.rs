//! Finds the input's lines as its bytes arrive, sampling every
//! `BLOCK_LINES` of them, so a line is read back by scanning one block.

use std::collections::HashMap;

use crate::text::ansi::Parser;

pub const BLOCK_LINES: usize = 64;

/// Where a block starts, and the parser before its first line: SGR and
/// OSC 8 carry across lines, so a block is decoded from it.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Block {
    pub start: u64,
    /// An index into the interned parsers.
    pub parser: u32,
}

/// What one chunk added to the index.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Indexed {
    pub lines: usize,
    /// Just past the last line indexed so far: its `\n`, or the input's
    /// end after `finish`.
    pub end: u64,
    pub blocks: Vec<Block>,
    /// The states this chunk interned first, in numbering order.
    pub parsers: Vec<Parser>,
    /// The lines that set a window title.
    pub titled: Vec<usize>,
}

#[derive(Default)]
pub struct Indexer {
    parser: Parser,
    ids: HashMap<Parser, u32>,
    lines: usize,
    offset: u64,
    end: u64,
    /// Bytes after the last `\n`.
    pending: Vec<u8>,
}

impl Indexer {
    /// The parser runs whatever the view style: a buffer can turn
    /// literal and back.
    pub fn new() -> Indexer {
        Indexer::default()
    }

    /// Indexes the lines `chunk` ends; the rest waits for the next chunk,
    /// or `finish`.
    pub fn feed(&mut self, mut chunk: &[u8]) -> Indexed {
        let mut out = Indexed::default();
        while let Some(i) = chunk.iter().position(|&c| c == b'\n') {
            self.offset += i as u64 + 1;
            if self.pending.is_empty() {
                self.add(&mut out, strip_cr(&chunk[..i]), self.offset);
            } else {
                let mut line = std::mem::take(&mut self.pending);
                line.extend_from_slice(&chunk[..i]);
                self.add(&mut out, strip_cr(&line), self.offset);
                line.clear();
                self.pending = line;
            }
            chunk = &chunk[i + 1..];
        }
        self.offset += chunk.len() as u64;
        self.pending.extend_from_slice(chunk);
        out.end = self.end;
        out
    }

    /// Ends the input: a partial last line becomes a line, `\r` and all.
    pub fn finish(&mut self) -> Indexed {
        let mut out = Indexed::default();
        if !self.pending.is_empty() {
            let line = std::mem::take(&mut self.pending);
            self.add(&mut out, &line, self.offset);
        }
        out.end = self.end;
        out
    }

    /// The bytes fed so far.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    fn add(&mut self, out: &mut Indexed, line: &[u8], end: u64) {
        if self.lines.is_multiple_of(BLOCK_LINES) {
            let parser = self.intern(out);
            out.blocks.push(Block {
                start: self.end,
                parser,
            });
        }
        if self.parser.skip(line) {
            out.titled.push(self.lines);
        }
        out.lines += 1;
        self.lines += 1;
        self.end = end;
    }

    fn intern(&mut self, out: &mut Indexed) -> u32 {
        if let Some(&id) = self.ids.get(&self.parser) {
            return id;
        }
        let id = self.ids.len() as u32;
        self.ids.insert(self.parser.clone(), id);
        out.parsers.push(self.parser.clone());
        id
    }
}

fn strip_cr(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r").unwrap_or(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_all(ix: &mut Indexer, chunks: &[&[u8]]) -> Indexed {
        let mut all = Indexed::default();
        let mut fed: Vec<Indexed> = chunks.iter().map(|c| ix.feed(c)).collect();
        fed.push(ix.finish());
        for c in fed {
            all.lines += c.lines;
            all.end = c.end;
            all.blocks.extend(c.blocks);
            all.parsers.extend(c.parsers);
            all.titled.extend(c.titled);
        }
        all
    }

    #[test]
    fn lines_end_at_newline_or_finish() {
        let mut ix = Indexer::new();
        let a = ix.feed(b"a\r\nb");
        assert_eq!((a.lines, a.end, ix.offset()), (1, 3, 4));
        let b = ix.feed(b"c\n");
        assert_eq!((b.lines, b.end, ix.offset()), (1, 6, 6));
        let c = ix.feed(b"d");
        assert_eq!((c.lines, c.end), (0, 6));
        let d = ix.finish();
        assert_eq!((d.lines, d.end, ix.offset()), (1, 7, 7));
        assert_eq!(ix.finish().lines, 0, "finish twice adds nothing");
    }

    #[test]
    fn blocks_start_every_block_lines() {
        let mut ix = Indexer::new();
        let mut input = Vec::new();
        for i in 0..2 * BLOCK_LINES + 1 {
            if i == BLOCK_LINES - 1 {
                input.extend_from_slice(b"\x1b[31m");
            }
            input.extend_from_slice(format!("{i}\n").as_bytes());
        }
        let all = feed_all(&mut ix, &[&input[..100], &input[100..]]);
        assert_eq!(all.lines, 2 * BLOCK_LINES + 1);
        let starts: Vec<u64> = all.blocks.iter().map(|b| b.start).collect();
        let want = |line: usize| {
            let text = String::from_utf8_lossy(&input);
            text.split_inclusive('\n')
                .take(line)
                .map(str::len)
                .sum::<usize>() as u64
        };
        assert_eq!(starts, [0, want(BLOCK_LINES), want(2 * BLOCK_LINES)]);
        // Blocks 1 and 2 both start red: one state, interned once.
        let ids: Vec<u32> = all.blocks.iter().map(|b| b.parser).collect();
        assert_eq!(ids, [0, 1, 1]);
        assert_eq!(all.parsers.len(), 2);
        assert_eq!(all.parsers[0], Parser::default());
        let mut red = Parser::default();
        red.skip(b"\x1b[31m");
        assert_eq!(all.parsers[1], red);
    }

    #[test]
    fn parsers_are_new_per_chunk() {
        let mut ix = Indexer::new();
        let block: Vec<u8> = b"x\n".repeat(BLOCK_LINES);
        let first = ix.feed(&block);
        assert_eq!(first.parsers.len(), 1);
        let second = ix.feed(&block);
        assert_eq!(second.blocks.len(), 1);
        assert!(
            second.parsers.is_empty(),
            "the default state is already known"
        );
    }

    #[test]
    fn titled_lines_are_absolute() {
        let mut ix = Indexer::new();
        let all = feed_all(
            &mut ix,
            &[
                b"intro\n\x1b]2;hunk\x07@@ one\na\n\x1b]2;hu",
                b"nk\x07@@ two\n",
                b"\x1b]2;\x07x",
            ],
        );
        assert_eq!(all.titled, [1, 3, 4]);
    }

    #[test]
    fn crlf_across_chunks() {
        let mut ix = Indexer::new();
        let all = feed_all(&mut ix, &[b"a\r", b"\nb\r"]);
        assert_eq!((all.lines, all.end), (2, 5));
    }
}
