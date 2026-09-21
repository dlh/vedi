// Package buffer holds the input as bytes and decodes lines on demand.
package buffer

import (
	"bytes"
	"errors"
	"io"
	"sync"

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
type Line struct {
	Text []rune
	Runs []ansi.Run
}

// cacheLines is how many decoded lines are kept before starting over.
const cacheLines = 1024

// errTruncated is the read error when indexed bytes are gone: the file
// shrank.
var errTruncated = errors.New("input truncated")

// Buffer indexes the input's lines. The bytes live in src: an arena
// Write fills, or the caller's ReaderAt. SGR and OSC 8 carry across
// lines, so each line's start state is kept, interned in parsers.
// Safe for one writer and any number of readers.
type Buffer struct {
	mu  sync.Mutex
	src io.ReaderAt
	mem *arena // src, when Write keeps the bytes

	// Changed under mu, by the writer alone, so it reads them freely.
	ends    []int64  // just past line i's "\n", or the input's end
	state   []uint32 // the parser at line i's start, an index into parsers
	parsers []ansi.Parser
	written int64

	// The writer's alone. A Write indexes its bytes into staged
	// outside mu, so readers wait only for publish.
	ids     map[ansi.Parser]uint32
	parser  ansi.Parser // after the last indexed line
	pending []byte      // bytes after the last "\n"
	staged  index       // lines indexed since the last publish

	cache map[int]Line
	raw   []byte // read's scratch

	eof bool
	err error
	nl  bool // the input ended with "\n"
}

// index is what a run of lines adds to ends, state and parsers.
type index struct {
	ends    []int64
	state   []uint32
	parsers []ansi.Parser
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
	return len(b.ends)
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

// add stages one line ending at end, given without its "\n" or the
// "\r" before it.
func (b *Buffer) add(raw []byte, end int64) {
	b.staged.ends = append(b.staged.ends, end)
	b.staged.state = append(b.staged.state, b.intern(b.parser))
	b.parser.Skip(raw)
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

// publish makes the staged lines readable, with written bytes in all.
// Called with mu held.
func (b *Buffer) publish(written int64) {
	b.ends = append(b.ends, b.staged.ends...)
	b.state = append(b.state, b.staged.state...)
	b.parsers = append(b.parsers, b.staged.parsers...)
	b.written = written
	b.staged.ends = b.staged.ends[:0]
	b.staged.state = b.staged.state[:0]
	b.staged.parsers = b.staged.parsers[:0]
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

// Line returns line i decoded, or a zero Line when i is out of range
// or its bytes cannot be read. Decoded lines are kept, cacheLines at
// a time.
func (b *Buffer) Line(i int) Line {
	b.mu.Lock()
	defer b.mu.Unlock()
	if i < 0 || i >= len(b.ends) {
		return Line{}
	}
	if l, ok := b.cache[i]; ok {
		return l
	}
	raw, ok := b.read(i)
	if !ok {
		return Line{}
	}
	p := b.parsers[b.state[i]]
	text, runs := p.Parse(raw)
	if b.cache == nil || len(b.cache) >= cacheLines {
		b.cache = map[int]Line{}
	}
	l := Line{Text: text, Runs: runs}
	b.cache[i] = l
	return l
}

// Text decodes line i's runes into dst, from dst[:0], and returns
// them: for walking many lines without keeping any. Out of range, or
// unreadable, is empty.
func (b *Buffer) Text(i int, dst []rune) []rune {
	b.mu.Lock()
	defer b.mu.Unlock()
	if i < 0 || i >= len(b.ends) {
		return dst[:0]
	}
	raw, ok := b.read(i)
	if !ok {
		return dst[:0]
	}
	p := b.parsers[b.state[i]]
	return p.Text(dst, raw)
}

// read is line i's bytes without its "\n" or the "\r" before it, in
// b.raw, good until the next read. A short read is recorded as the
// read error.
func (b *Buffer) read(i int) ([]byte, bool) {
	var start int64
	if i > 0 {
		start = b.ends[i-1]
	}
	n := int(b.ends[i] - start)
	if cap(b.raw) < n {
		b.raw = make([]byte, n)
	}
	raw := b.raw[:n]
	if n, err := b.src.ReadAt(raw, start); n < len(raw) {
		if b.err == nil {
			if err == nil || err == io.EOF || err == io.ErrUnexpectedEOF {
				err = errTruncated
			}
			b.err = err
		}
		return nil, false
	}
	if n := len(raw); n > 0 && raw[n-1] == '\n' {
		raw = raw[:n-1]
		if n := len(raw); n > 0 && raw[n-1] == '\r' {
			raw = raw[:n-1]
		}
	}
	return raw, true
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

// Finished reports whether input has ended, and the read error: from
// the reader, or from a line whose bytes are gone.
func (b *Buffer) Finished() (bool, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	return b.eof, b.err
}
