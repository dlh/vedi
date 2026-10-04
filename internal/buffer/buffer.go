// Package buffer holds the input as bytes and decodes lines on demand.
package buffer

import (
	"bytes"
	"cmp"
	"errors"
	"io"
	"slices"
	"sync"
	"unicode/utf8"

	"go.dlh.dev/vedi/internal/ansi"
)

// Pos is a logical position: a line index and a rune index within it.
// Col may equal len(Text), meaning "after the last rune".
type Pos struct{ Line, Col int }

// Less reports whether p comes before q.
func (p Pos) Less(q Pos) bool {
	return p.Line < q.Line || (p.Line == q.Line && p.Col < q.Col)
}

// Line is one logical line, decoded. Lines never change once indexed.
// Title is the window title in effect once the line is drawn: set on
// it with OSC 0 or 2, or carried from a line before; "" for none.
type Line struct {
	Text  []rune
	Runs  []ansi.Run
	Title string
}

// cacheLines is how many decoded lines are kept before starting over.
const cacheLines = 1024

// BlockLines is how many lines share one index entry: the index says
// where each block starts, and a block is scanned for the lines in it.
const (
	blockShift = 6
	BlockLines = 1 << blockShift
)

// errTruncated is the read error when indexed bytes are gone: the file
// shrank.
var errTruncated = errors.New("input truncated")

// Buffer indexes the input's lines. The bytes live in src: an arena
// Write fills, or the caller's ReaderAt. The index samples every
// BlockLines lines: where the block starts and, since SGR and OSC 8
// carry across lines, the parser there, interned in parsers. Reading
// a line scans its block, kept until another is read.
// Safe for one writer and any number of readers.
type Buffer struct {
	mu  sync.Mutex
	src io.ReaderAt
	mem *arena // src, when Write keeps the bytes

	// Changed under mu, by the writer alone, so it reads them freely.
	n       int      // lines
	last    int64    // just past line n-1's "\n", or the input's end
	starts  []int64  // where line BlockLines*k begins
	state   []uint32 // the parser at line BlockLines*k, an index into parsers
	parsers []ansi.Parser
	written int64
	parts   []int // the line each input begins at; empty until StartPart
	titles  []int // the lines that set a window title

	// The writer's alone. A Write indexes its bytes into staged
	// outside mu, so readers wait only for publish.
	ids     map[ansi.Parser]uint32
	parser  ansi.Parser // after the last indexed line
	lines   int         // indexed, staged or published
	end     int64       // just past the last indexed line
	pending []byte      // bytes after the last "\n"
	staged  index       // blocks begun since the last publish

	// The readers', under mu.
	block block
	cache map[int]Line
	raw   []byte // the scanned block's bytes

	literal bool // see SetLiteral

	eof bool
	err error
	nl  bool // the input ended with "\n"
}

// index is what a run of lines adds to starts, state, parsers and
// titles.
type index struct {
	starts  []int64
	state   []uint32
	parsers []ansi.Parser
	titles  []int
}

// block is block k scanned up to end: n lines from k*BlockLines and
// where each ends; err is the read error that cut the scan short. The
// parser at each line's start is learned as lines are decoded in
// order, or skipped to: known is how many are. The block is scanned
// again when its end moves: the last block's, as lines are published.
type block struct {
	k, n, known int
	end         int64
	err         error
	ends        [BlockLines]int64
	state       [BlockLines]ansi.Parser
}

// New is a buffer that keeps what is written to it.
func New() *Buffer {
	m := &arena{}
	return &Buffer{src: m, mem: m, ids: map[ansi.Parser]uint32{}}
}

// NewFrom is a buffer over src, which already holds the input: Write
// indexes its bytes, given in order, and keeps none.
func NewFrom(src io.ReaderAt) *Buffer {
	return &Buffer{src: src, ids: map[ansi.Parser]uint32{}}
}

func (b *Buffer) Len() int {
	b.mu.Lock()
	defer b.mu.Unlock()
	return b.n
}

// Write appends input. Bytes up to the last "\n" become lines; the
// rest wait for the next Write, or Finish.
func (b *Buffer) Write(p []byte) {
	if b.mem != nil {
		b.mem.Write(p)
	}
	written := b.written
	for {
		i := bytes.IndexByte(p, '\n')
		if i < 0 {
			b.pending = append(b.pending, p...)
			written += int64(len(p))
			break
		}
		line := p[:i]
		if len(b.pending) > 0 {
			line = append(b.pending, line...)
		}
		written += int64(i + 1)
		b.add(bytes.TrimSuffix(line, []byte{'\r'}), written)
		b.pending = b.pending[:0]
		p = p[i+1:]
	}
	b.mu.Lock()
	b.publish(written)
	b.mu.Unlock()
}

// add indexes one line ending at end, given without its "\n" or the
// "\r" before it: the first of a block is staged.
func (b *Buffer) add(raw []byte, end int64) {
	if b.lines&(BlockLines-1) == 0 {
		b.staged.starts = append(b.staged.starts, b.end)
		b.staged.state = append(b.staged.state, b.intern(b.parser))
	}
	if b.parser.Skip(raw) {
		b.staged.titles = append(b.staged.titles, b.lines)
	}
	b.lines++
	b.end = end
}

func (b *Buffer) intern(p ansi.Parser) uint32 {
	if id, ok := b.ids[p]; ok {
		return id
	}
	id := uint32(len(b.parsers) + len(b.staged.parsers))
	b.staged.parsers = append(b.staged.parsers, p)
	b.ids[p] = id
	return id
}

// publish makes the indexed lines readable, with written bytes in all.
// Called with mu held.
func (b *Buffer) publish(written int64) {
	b.starts = append(b.starts, b.staged.starts...)
	b.state = append(b.state, b.staged.state...)
	b.parsers = append(b.parsers, b.staged.parsers...)
	b.titles = append(b.titles, b.staged.titles...)
	b.n, b.last, b.written = b.lines, b.end, written
	b.staged.starts = b.staged.starts[:0]
	b.staged.state = b.staged.state[:0]
	b.staged.parsers = b.staged.parsers[:0]
	b.staged.titles = b.staged.titles[:0]
}

// Finish marks the end of input: a partial last line becomes a line;
// err is the read error, or nil at EOF; trailingNewline reports
// whether the input ended with "\n".
func (b *Buffer) Finish(err error, trailingNewline bool) {
	if len(b.pending) > 0 {
		b.add(b.pending, b.written)
		b.pending = nil
	}
	b.mu.Lock()
	b.publish(b.written)
	b.eof, b.nl = true, trailingNewline
	if err != nil {
		b.err = err
	}
	b.mu.Unlock()
}

// SetLiteral says whether lines are decoded as their bytes: escapes
// and controls are text, and there are no runs and no titles.
func (b *Buffer) SetLiteral(on bool) {
	b.mu.Lock()
	defer b.mu.Unlock()
	if b.literal != on {
		b.literal, b.cache = on, nil
	}
}

func (b *Buffer) Literal() bool {
	b.mu.Lock()
	defer b.mu.Unlock()
	return b.literal
}

// Line returns line i decoded, or a zero Line when i is out of range
// or its bytes cannot be read. Decoded lines are kept, cacheLines at
// a time.
func (b *Buffer) Line(i int) Line {
	b.mu.Lock()
	defer b.mu.Unlock()
	if i < 0 || i >= b.n {
		return Line{}
	}
	if l, ok := b.cache[i]; ok {
		return l
	}
	raw, p, ok := b.read(i)
	if !ok {
		return Line{}
	}
	var l Line
	if b.literal {
		l.Text = ansi.Literal(make([]rune, 0, utf8.RuneCount(raw)), raw)
	} else {
		text, runs := p.Parse(raw)
		b.learn(i, p)
		l = Line{Text: text, Runs: runs, Title: p.Title()}
	}
	if b.cache == nil || len(b.cache) >= cacheLines {
		b.cache = map[int]Line{}
	}
	b.cache[i] = l
	return l
}

// Text decodes line i's runes into dst, from dst[:0], and returns
// them: for walking many lines without keeping any. Out of range, or
// unreadable, is empty.
func (b *Buffer) Text(i int, dst []rune) []rune {
	b.mu.Lock()
	defer b.mu.Unlock()
	if i < 0 || i >= b.n {
		return dst[:0]
	}
	raw, p, ok := b.read(i)
	if !ok {
		return dst[:0]
	}
	if b.literal {
		return ansi.Literal(dst, raw)
	}
	text := p.Text(dst, raw)
	b.learn(i, p)
	return text
}

// Raw copies block k's bytes, as they came, into dst[:0] and returns
// them: lines k*BlockLines on, each with its ending, for scanning many
// lines without decoding any. Bytes that are gone are left out, and
// recorded as the read error.
func (b *Buffer) Raw(k int, dst []byte) []byte {
	b.mu.Lock()
	defer b.mu.Unlock()
	if k < 0 || k >= len(b.starts) {
		return dst[:0]
	}
	raw, err := b.load(k, dst)
	if err != nil && b.err == nil {
		b.err = err
	}
	return raw
}

// load reads block k into dst, grown to hold it; err is why the bytes
// returned are fewer than the block's.
func (b *Buffer) load(k int, dst []byte) ([]byte, error) {
	start := b.starts[k]
	size := int(b.blockEnd(k) - start)
	if cap(dst) < size {
		dst = make([]byte, size)
	}
	n, err := b.src.ReadAt(dst[:size], start)
	if n == size {
		return dst[:size], nil
	}
	if err == nil || err == io.EOF || err == io.ErrUnexpectedEOF {
		err = errTruncated
	}
	return dst[:n], err
}

// read is line i's bytes without its "\n" or the "\r" before it, in
// b.raw, good until the next read, and the parser at its start, which
// a literal buffer has no use for and leaves zero. A line whose bytes
// are gone is recorded as the read error.
func (b *Buffer) read(i int) ([]byte, ansi.Parser, bool) {
	k, j := i>>blockShift, i&(BlockLines-1)
	if b.block.k != k || b.block.end != b.blockEnd(k) {
		b.scan(k)
	}
	if j >= b.block.n {
		if b.err == nil {
			b.err = cmp.Or(b.block.err, errTruncated)
		}
		return nil, ansi.Parser{}, false
	}
	if b.literal {
		return b.line(j), ansi.Parser{}, true
	}
	for m := b.block.known; m <= j; m++ {
		p := b.block.state[m-1]
		p.Skip(b.line(m - 1))
		b.block.state[m] = p
		b.block.known = m + 1
	}
	return b.line(j), b.block.state[j], true
}

// learn records p, the parser after decoding line i, as the next
// line's when it is the first not yet known.
func (b *Buffer) learn(i int, p ansi.Parser) {
	j := i&(BlockLines-1) + 1
	if j == b.block.known && j < b.block.n {
		b.block.state[j] = p
		b.block.known = j + 1
	}
}

// line is line j of the scanned block, without its ending.
func (b *Buffer) line(j int) []byte {
	start := b.starts[b.block.k]
	if j > 0 {
		start = b.block.ends[j-1]
	}
	return strip(b.raw[start-b.starts[b.block.k] : b.block.ends[j]-b.starts[b.block.k]])
}

// blockEnd is just past block k's last line: the next block's start,
// or the input's last line's end.
func (b *Buffer) blockEnd(k int) int64 {
	if k+1 < len(b.starts) {
		return b.starts[k+1]
	}
	return b.last
}

// scan reads block k into b.raw and finds the lines whose bytes are
// there.
func (b *Buffer) scan(k int) {
	start, end := b.starts[k], b.blockEnd(k)
	size := int(end - start)
	raw, err := b.load(k, b.raw)
	b.raw = raw[:cap(raw)]
	n := len(raw)
	b.block.k, b.block.end, b.block.err = k, end, err
	b.block.n, b.block.known = 0, 1
	b.block.state[0] = b.parsers[b.state[k]]
	pos := 0
	for j := range BlockLines {
		var lineEnd int
		if nl := bytes.IndexByte(raw[pos:], '\n'); nl >= 0 {
			lineEnd = pos + nl + 1
		} else if n == size && pos < size {
			lineEnd = size // the input's last line, unterminated
		} else {
			break
		}
		b.block.ends[j] = start + int64(lineEnd)
		pos = lineEnd
		b.block.n = j + 1
	}
}

// strip drops the "\n" ending a line, and the "\r" before it.
func strip(raw []byte) []byte {
	if n := len(raw); n > 0 && raw[n-1] == '\n' {
		raw = raw[:n-1]
		if n := len(raw); n > 0 && raw[n-1] == '\r' {
			raw = raw[:n-1]
		}
	}
	return raw
}

// WriteTo copies the input's bytes to w, as they came.
func (b *Buffer) WriteTo(w io.Writer) (int64, error) {
	b.mu.Lock()
	src, n := b.src, b.written
	b.mu.Unlock()
	return io.Copy(w, io.NewSectionReader(src, 0, n))
}

// TrailingNewline reports whether the input ended with "\n". A trailing
// "\n" adds no line, so this tells a terminated last line from a blank
// bottom row.
func (b *Buffer) TrailingNewline() bool {
	b.mu.Lock()
	defer b.mu.Unlock()
	return b.nl
}

// StartPart begins the next input, for the writer to call before its
// bytes: it starts at the next line to begin, so a line an unfinished
// input left open stays with that input. Lines written before the
// first StartPart are part 0's, so a first part they precede is part 1.
func (b *Buffer) StartPart() {
	start := b.lines
	if len(b.pending) > 0 {
		start++
	}
	b.mu.Lock()
	if len(b.parts) == 0 && start > 0 {
		b.parts = append(b.parts, 0)
	}
	b.parts = append(b.parts, start)
	b.mu.Unlock()
}

// PartAt is the input line i came from, as an index into the parts
// StartPart began. The last part starting at or before the line is
// its: an empty one yields to the next. Lines before any part, or an
// empty buffer, are part 0's.
func (b *Buffer) PartAt(i int) int {
	k, _, _ := b.Part(i)
	return k
}

// Part is the input line i came from, as PartAt says, with the lines
// it holds as [start, end): to the next input's start, or the end of
// the buffer for the last.
func (b *Buffer) Part(i int) (k, start, end int) {
	b.mu.Lock()
	defer b.mu.Unlock()
	end = b.n
	for k, start = range slices.Backward(b.parts) {
		if start <= i {
			if k+1 < len(b.parts) {
				end = b.parts[k+1]
			}
			return k, start, end
		}
	}
	return 0, 0, end
}

// Parts is a copy of the line each input begins at, as StartPart
// recorded them: ascending, an empty input repeating the start of the
// next, and none before the first StartPart.
func (b *Buffer) Parts() []int {
	b.mu.Lock()
	defer b.mu.Unlock()
	return slices.Clone(b.parts)
}

// Titles is a copy of the lines that set a window title, ascending:
// to the title already in effect, or to none, included. A literal
// buffer has none.
func (b *Buffer) Titles() []int {
	b.mu.Lock()
	defer b.mu.Unlock()
	if b.literal {
		return nil
	}
	return slices.Clone(b.titles)
}

// StartsPart reports whether an input begins at line i.
func (b *Buffer) StartsPart(i int) bool {
	b.mu.Lock()
	defer b.mu.Unlock()
	_, ok := slices.BinarySearch(b.parts, i)
	return ok
}

// Finished reports whether input has ended, and the read error: from
// the reader, or from a line whose bytes are gone.
func (b *Buffer) Finished() (bool, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	return b.eof, b.err
}
