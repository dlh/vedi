package buffer

import (
	"io"
	"strings"
	"testing"

	"github.com/gdamore/tcell/v2"
)

// TestConcatJoinsParts: Read is the parts in order, an unterminated
// part joining the next, an empty part adding nothing; ReadAt sees
// everything Read has passed, boundaries included.
func TestConcatJoinsParts(t *testing.T) {
	c := NewConcat(strings.NewReader("ab"), strings.NewReader(""), strings.NewReader("c\nd"))
	all, err := io.ReadAll(c)
	if err != nil || string(all) != "abc\nd" {
		t.Fatalf("ReadAll = %q, %v", all, err)
	}
	p := make([]byte, 3)
	if n, err := c.ReadAt(p, 1); n != 3 || err != nil || string(p) != "bc\n" {
		t.Fatalf("ReadAt(1) = %q, %d, %v", p, n, err)
	}
	if n, err := c.ReadAt(p, 4); n != 1 || err != io.EOF || p[0] != 'd' {
		t.Fatalf("ReadAt(4) = %q, %d, %v; want d, 1, EOF", p[:n], n, err)
	}
}

// TestConcatReadAtBehindRead: while a part is still being read, ReadAt
// serves what Read has returned from it.
func TestConcatReadAtBehindRead(t *testing.T) {
	c := NewConcat(strings.NewReader("ab\n"), strings.NewReader("cd\nef\n"))
	p := make([]byte, 6)
	if n, err := io.ReadFull(c, p); n != 6 || err != nil {
		t.Fatalf("ReadFull = %d, %v", n, err)
	}
	q := make([]byte, 3)
	if n, err := c.ReadAt(q, 3); n != 3 || err != nil || string(q) != "cd\n" {
		t.Fatalf("ReadAt(3) = %q, %d, %v", q, n, err)
	}
}

// TestConcatBacksABuffer: NewFrom over a Concat pages files without
// holding their bytes.
func TestConcatBacksABuffer(t *testing.T) {
	c := NewConcat(strings.NewReader("\x1b[31mone"), strings.NewReader("\ntwo\n"))
	b := NewFrom(c)
	Fill(c, b, func() {})
	if got := lines(b); strings.Join(got, "|") != "one|two" {
		t.Fatalf("lines = %q", got)
	}
	if runs := b.Line(1).Runs; len(runs) != 1 || runs[0].Style != tcell.StyleDefault.Foreground(tcell.PaletteColor(1)) {
		t.Fatalf("line 1 runs = %v, want red carried across files", runs)
	}
}
