package buffer

import (
	"fmt"
	"io"
	"strings"
	"testing"
	"testing/iotest"

	"github.com/gdamore/tcell/v3"
)

// TestConcatJoinsParts: Read is the parts in order, an unterminated
// part given the newline it lacks so the next starts a line of its
// own, the last kept as it ended, an empty part adding nothing;
// ReadAt sees everything Read has passed, boundaries and given
// newlines included.
func TestConcatJoinsParts(t *testing.T) {
	c := NewConcat(strings.NewReader("ab"), strings.NewReader(""), strings.NewReader("c\nd"))
	all, err := io.ReadAll(c)
	if err != nil || string(all) != "ab\nc\nd" {
		t.Fatalf("ReadAll = %q, %v", all, err)
	}
	p := make([]byte, 3)
	if n, err := c.ReadAt(p, 1); n != 3 || err != nil || string(p) != "b\nc" {
		t.Fatalf("ReadAt(1) = %q, %d, %v", p, n, err)
	}
	if n, err := c.ReadAt(p[:1], 2); n != 1 || err != nil || p[0] != '\n' {
		t.Fatalf("ReadAt(2) = %q, %d, %v; want the given newline", p[:n], n, err)
	}
	if n, err := c.ReadAt(p, 5); n != 1 || err != io.EOF || p[0] != 'd' {
		t.Fatalf("ReadAt(5) = %q, %d, %v; want d, 1, EOF", p[:n], n, err)
	}
}

// TestConcatKeepsLastEnding: a lone input that ends mid-line is read
// as it is, so the pager knows its last row was not blank.
func TestConcatKeepsLastEnding(t *testing.T) {
	all, err := io.ReadAll(NewConcat(strings.NewReader("ab\ncd")))
	if err != nil || string(all) != "ab\ncd" {
		t.Fatalf("ReadAll = %q, %v", all, err)
	}
}

// eofWithData returns the last bytes and io.EOF together, as a reader
// may; ReadAt is the string's.
type eofWithData struct {
	r  io.Reader
	at *strings.Reader
}

func (r eofWithData) Read(p []byte) (int, error)              { return r.r.Read(p) }
func (r eofWithData) ReadAt(p []byte, off int64) (int, error) { return r.at.ReadAt(p, off) }

// TestConcatNewlineOwedAcrossReads: a part whose last bytes fill the
// buffer and end with EOF at once is given its newline by the next
// Read, before the next part begins.
func TestConcatNewlineOwedAcrossReads(t *testing.T) {
	first := strings.NewReader("ab")
	c := NewConcat(eofWithData{iotest.DataErrReader(first), first}, strings.NewReader("c\n"))
	var parts []int
	c.OnPart = func(i int) { parts = append(parts, i) }
	p := make([]byte, 2)
	if n, err := c.Read(p); n != 2 || err != nil || string(p) != "ab" {
		t.Fatalf("first Read = %q, %d, %v", p[:n], n, err)
	}
	if n, err := c.Read(p); n != 1 || err != nil || p[0] != '\n' {
		t.Fatalf("second Read = %q, %d, %v; want the newline", p[:n], n, err)
	}
	if len(parts) != 1 {
		t.Fatalf("OnPart calls before the newline = %v, want just 0", parts)
	}
	rest, err := io.ReadAll(c)
	if err != nil || string(rest) != "c\n" || len(parts) != 2 {
		t.Fatalf("rest = %q, %v, parts %v", rest, err, parts)
	}
}

// TestConcatOnPart: OnPart is called once per part, in order, before
// its first byte is read, an empty part included.
func TestConcatOnPart(t *testing.T) {
	c := NewConcat(strings.NewReader("ab"), strings.NewReader(""), strings.NewReader("c\nd"))
	var got []string
	read := ""
	c.OnPart = func(i int) { got = append(got, fmt.Sprintf("%d@%q", i, read)) }
	p := make([]byte, 1)
	for {
		n, err := c.Read(p)
		read += string(p[:n])
		if err == io.EOF {
			break
		}
	}
	want := `0@"" 1@"ab\n" 2@"ab\n"`
	if strings.Join(got, " ") != want || read != "ab\nc\nd" {
		t.Fatalf("OnPart calls = %q, read %q; want %s", got, read, want)
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
	if got := lines(b); strings.Join(got, "|") != "one||two" {
		t.Fatalf("lines = %q", got)
	}
	if runs := b.Line(2).Runs; len(runs) != 1 || runs[0].Style != tcell.StyleDefault.Foreground(tcell.PaletteColor(1)) {
		t.Fatalf("line 1 runs = %v, want red carried across files", runs)
	}
}
