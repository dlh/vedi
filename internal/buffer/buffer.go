// Package buffer holds the immutable, append-only lines of the input.
package buffer

import (
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
	mu    sync.RWMutex
	lines []Line
	eof   bool
	err   error
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

func (b *Buffer) Append(l Line) {
	b.mu.Lock()
	b.lines = append(b.lines, l)
	b.mu.Unlock()
}

// Finish marks the end of input. err is the read error, or nil at EOF.
func (b *Buffer) Finish(err error) {
	b.mu.Lock()
	b.eof, b.err = true, err
	b.mu.Unlock()
}

// Finished reports whether input has ended and with what error.
func (b *Buffer) Finished() (bool, error) {
	b.mu.RLock()
	defer b.mu.RUnlock()
	return b.eof, b.err
}
