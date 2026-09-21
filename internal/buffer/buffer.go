// Package buffer holds the immutable, append-only lines of the input.
package buffer

import (
	"bytes"
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

// Line is one logical line. Lines never change once appended.
type Line struct {
	Text []rune
	Runs []ansi.Run
}

// Buffer is safe for one writer and any number of readers.
type Buffer struct {
	mu      sync.RWMutex
	lines   []Line
	parser  ansi.Parser
	pending []byte // bytes after the last "\n"
	eof     bool
	err     error
	nl      bool // the input ended with "\n"
}

func New() *Buffer { return &Buffer{} }

func (b *Buffer) Len() int {
	b.mu.RLock()
	defer b.mu.RUnlock()
	return len(b.lines)
}

// Line returns line i, or a zero Line when i is out of range.
func (b *Buffer) Line(i int) Line {
	b.mu.RLock()
	defer b.mu.RUnlock()
	if i < 0 || i >= len(b.lines) {
		return Line{}
	}
	return b.lines[i]
}

// Write appends input. Bytes up to the last "\n" become lines; the
// rest wait for the next Write, or Finish.
func (b *Buffer) Write(p []byte) {
	b.mu.Lock()
	defer b.mu.Unlock()
	for {
		i := bytes.IndexByte(p, '\n')
		if i < 0 {
			b.pending = append(b.pending, p...)
			return
		}
		line := p[:i]
		if len(b.pending) > 0 {
			line = append(b.pending, line...)
		}
		b.add(bytes.TrimSuffix(line, []byte{'\r'}))
		b.pending = b.pending[:0]
		p = p[i+1:]
	}
}

// add appends one line, given without its "\n" or the "\r" before it.
func (b *Buffer) add(raw []byte) {
	text, runs := b.parser.Parse(raw)
	b.lines = append(b.lines, Line{Text: text, Runs: runs})
}

// Finish marks the end of input: a partial last line becomes a line;
// err is the read error, or nil at EOF; trailingNewline reports
// whether the input ended with "\n".
func (b *Buffer) Finish(err error, trailingNewline bool) {
	b.mu.Lock()
	if len(b.pending) > 0 {
		b.add(b.pending)
		b.pending = nil
	}
	b.eof, b.err, b.nl = true, err, trailingNewline
	b.mu.Unlock()
}

// TrailingNewline reports whether the input ended with "\n". A trailing
// "\n" adds no line, so this tells a terminated last line from a blank
// bottom row.
func (b *Buffer) TrailingNewline() bool {
	b.mu.RLock()
	defer b.mu.RUnlock()
	return b.nl
}

// Finished reports whether input has ended and with what error.
func (b *Buffer) Finished() (bool, error) {
	b.mu.RLock()
	defer b.mu.RUnlock()
	return b.eof, b.err
}
