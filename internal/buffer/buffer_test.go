package buffer

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"runtime"
	"strings"
	"testing"

	"github.com/gdamore/tcell/v2"
)

func lines(b *Buffer) []string {
	var out []string
	for i := 0; i < b.Len(); i++ {
		out = append(out, string(b.Line(i).Text))
	}
	return out
}

func TestFillSplitsLines(t *testing.T) {
	tests := []struct {
		name string
		in   string
		want []string
	}{
		{"lf", "a\nb", []string{"a", "b"}},
		{"crlf", "a\r\nb\r\n", []string{"a", "b"}},
		{"trailing lf no extra line", "a\n", []string{"a"}},
		{"bare cr kept", "x\ry\n", []string{"x\ry"}},
		{"cr at eof kept", "x\r", []string{"x\r"}},
		{"empty input", "", nil},
		{"blank lines", "\n\n", []string{"", ""}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			b := New()
			Fill(strings.NewReader(tc.in), b, func() {})
			got := lines(b)
			if strings.Join(got, "|") != strings.Join(tc.want, "|") || len(got) != len(tc.want) {
				t.Fatalf("lines = %q, want %q", got, tc.want)
			}
			if eof, err := b.Finished(); !eof || err != nil {
				t.Fatalf("Finished = %v, %v; want true, nil", eof, err)
			}
		})
	}
}

func TestFillRecordsTrailingNewline(t *testing.T) {
	for _, tc := range []struct {
		in   string
		want bool
	}{
		{"a\n", true}, {"a\r\n", true}, {"\n", true}, {"a", false}, {"a\nb", false}, {"", false},
	} {
		b := New()
		Fill(strings.NewReader(tc.in), b, func() {})
		if got := b.TrailingNewline(); got != tc.want {
			t.Errorf("TrailingNewline(%q) = %v, want %v", tc.in, got, tc.want)
		}
	}
}

func TestFillStyleCarriesAcrossLines(t *testing.T) {
	b := New()
	Fill(strings.NewReader("\x1b[31ma\nb"), b, func() {})
	runs := b.Line(1).Runs
	want := tcell.StyleDefault.Foreground(tcell.PaletteColor(1))
	if len(runs) != 1 || runs[0].Style != want {
		t.Fatalf("line 1 runs = %v, want one red run", runs)
	}
}

type failReader struct{ n int }

func (f *failReader) Read(p []byte) (int, error) {
	if f.n == 0 {
		f.n++
		return copy(p, "ok\npartial"), nil
	}
	return 0, errors.New("boom")
}

func TestFillReportsReadError(t *testing.T) {
	b := New()
	calls := 0
	Fill(&failReader{}, b, func() { calls++ })
	if got := lines(b); len(got) != 2 || got[0] != "ok" || got[1] != "partial" {
		t.Fatalf("lines = %q, want [ok partial]", got)
	}
	eof, err := b.Finished()
	if !eof || err == nil || err.Error() != "boom" {
		t.Fatalf("Finished = %v, %v; want true, boom", eof, err)
	}
	if calls < 2 {
		t.Fatalf("notify called %d times, want at least 2 (one read plus finish)", calls)
	}
}

func TestLineOutOfRange(t *testing.T) {
	b := New()
	if l := b.Line(0); l.Text != nil || l.Runs != nil {
		t.Fatalf("Line(0) on empty buffer = %v, want zero", l)
	}
	if l := b.Line(-1); l.Text != nil {
		t.Fatalf("Line(-1) = %v, want zero", l)
	}
}

func TestPosLess(t *testing.T) {
	if !(Pos{0, 5}).Less(Pos{1, 0}) || !(Pos{1, 0}).Less(Pos{1, 1}) || (Pos{1, 1}).Less(Pos{1, 1}) {
		t.Fatal("Pos.Less ordering is wrong")
	}
}

var _ io.Reader = (*failReader)(nil)

// TestWriteJoinsAcrossWrites: a line is one line however many writes
// carry it, and it is not a line until its "\n" or Finish.
func TestWriteJoinsAcrossWrites(t *testing.T) {
	b := New()
	long := strings.Repeat("x", 100_000)
	b.Write([]byte("ab"))
	b.Write([]byte("c\n" + long[:50_000]))
	if got := lines(b); len(got) != 1 || got[0] != "abc" {
		t.Fatalf("lines = %q, want [abc]", got)
	}
	b.Write([]byte(long[50_000:] + "\nd"))
	if got := lines(b); len(got) != 2 || got[1] != long {
		t.Fatalf("second line has %d runes, want %d", len(b.Line(1).Text), len(long))
	}
	b.Finish(nil, false)
	if got := lines(b); len(got) != 3 || got[2] != "d" {
		t.Fatalf("lines after Finish = %d, want 3 ending in d", len(got))
	}
}

// TestWriteCRLFAcrossWrites: the "\r" before a "\n" goes even when
// they arrive apart; any other "\r" stays.
func TestWriteCRLFAcrossWrites(t *testing.T) {
	b := New()
	b.Write([]byte("a\r"))
	b.Write([]byte("\nb\rc\nd\r"))
	b.Finish(nil, false)
	if got := lines(b); strings.Join(got, "|") != "a|b\rc|d\r" {
		t.Fatalf("lines = %q", got)
	}
}

// TestStateCarriesOutOfOrder: a line decoded first still gets the
// color and link set above it, and an escape cut off at a line end
// changes nothing.
func TestStateCarriesOutOfOrder(t *testing.T) {
	b := New()
	Fill(strings.NewReader("\x1b[31ma\n\x1b]8;;http://x\x1b\\b\ncut \x1b[3\nc\n\x1b[0md"), b, func() {})
	red := tcell.StyleDefault.Foreground(tcell.PaletteColor(1))
	if runs := b.Line(3).Runs; len(runs) != 1 || runs[0].Style != red.Url("http://x") || runs[0].Url != "http://x" {
		t.Fatalf("line 3 (read first) runs = %v, want red link", runs)
	}
	if runs := b.Line(4).Runs; len(runs) != 1 || runs[0].Style != tcell.StyleDefault.Url("http://x") {
		t.Fatalf("line 4 runs = %v, want link only", runs)
	}
	if runs := b.Line(0).Runs; len(runs) != 1 || runs[0].Style != red {
		t.Fatalf("line 0 runs = %v, want red", runs)
	}
	if got := string(b.Line(2).Text); got != "cut " {
		t.Fatalf("line 2 = %q, want the cut escape dropped", got)
	}
}

// TestCacheResets: reading more lines than the cache holds keeps
// every line right.
func TestCacheResets(t *testing.T) {
	b := New()
	var in strings.Builder
	for i := 0; i < cacheLines+5; i++ {
		fmt.Fprintf(&in, "%d\n", i)
	}
	Fill(strings.NewReader(in.String()), b, func() {})
	for pass := 0; pass < 2; pass++ {
		for i := 0; i < b.Len(); i++ {
			if got := string(b.Line(i).Text); got != fmt.Sprint(i) {
				t.Fatalf("pass %d line %d = %q", pass, i, got)
			}
		}
	}
}

// TestNewFromKeepsOnlyTheIndex: a file-backed buffer reads lines from
// the source it was given.
func TestNewFromKeepsOnlyTheIndex(t *testing.T) {
	src := strings.NewReader("a\n\x1b[1mb\r\nc")
	b := NewFrom(src)
	Fill(src, b, func() {})
	if got := lines(b); strings.Join(got, "|") != "a|b|c" {
		t.Fatalf("lines = %q", got)
	}
	if runs := b.Line(1).Runs; len(runs) != 1 || runs[0].Style != tcell.StyleDefault.Bold(true) {
		t.Fatalf("line 1 runs = %v, want bold", runs)
	}
	if b.TrailingNewline() {
		t.Fatal("TrailingNewline = true for input ending in c")
	}
}

// TestTextDecodesIntoScratch: Text gives the same runes as Line, in
// the caller's slice, without touching the cache.
func TestTextDecodesIntoScratch(t *testing.T) {
	b := New()
	Fill(strings.NewReader("\x1b[31mab\ncd"), b, func() {})
	dst := make([]rune, 0, 8)
	got := b.Text(1, dst)
	if string(got) != "cd" || &got[0] != &dst[:1][0] {
		t.Fatalf("Text(1) = %q, want cd in dst", string(got))
	}
	if got := b.Text(0, got); string(got) != "ab" {
		t.Fatalf("Text(0) = %q", string(got))
	}
	if got := b.Text(7, dst); len(got) != 0 {
		t.Fatalf("Text out of range = %q, want empty", string(got))
	}
}

type failAt struct{}

func (failAt) ReadAt([]byte, int64) (int, error) { return 0, errors.New("gone") }

// TestReadFailureGivesEmptyLine: a source that cannot be read gives
// empty lines, not a panic.
func TestReadFailureGivesEmptyLine(t *testing.T) {
	b := NewFrom(failAt{})
	Fill(strings.NewReader("abc\n"), b, func() {})
	if l := b.Line(0); b.Len() != 1 || l.Text != nil || l.Runs != nil {
		t.Fatalf("Line(0) = %v, want zero", l)
	}
	if got := b.Text(0, nil); len(got) != 0 {
		t.Fatalf("Text(0) = %q, want empty", string(got))
	}
	if _, err := b.Finished(); err == nil || err.Error() != "gone" {
		t.Fatalf("err = %v, want gone: the source's error, not truncation", err)
	}
}

// TestShortBlockKeepsItsLines: a line whose bytes are there reads
// without error even when later lines of its block are gone; the
// error comes with a line that is.
func TestShortBlockKeepsItsLines(t *testing.T) {
	src := &shrinking{data: []byte("one\ntwo\nthree\n")}
	b := NewFrom(src)
	b.Write(src.data)
	src.data = src.data[:8]
	if got := string(b.Line(0).Text); got != "one" {
		t.Errorf("line 0 = %q, want one", got)
	}
	if _, err := b.Finished(); err != nil {
		t.Errorf("err = %v after reading a line that is there, want nil", err)
	}
	if got := b.Line(2).Text; len(got) != 0 {
		t.Errorf("line 2 = %q, want empty", string(got))
	}
	if _, err := b.Finished(); err != errTruncated {
		t.Errorf("err = %v, want %v", err, errTruncated)
	}
}

// TestRewrittenBlockIsError: bytes rewritten with fewer newlines read
// in full, but the lines past the last newline are gone.
func TestRewrittenBlockIsError(t *testing.T) {
	src := &shrinking{data: []byte("a\nb\n")}
	b := NewFrom(src)
	b.Write(src.data)
	src.data = []byte("abcd")
	if got := b.Line(1).Text; len(got) != 0 {
		t.Errorf("line 1 = %q, want empty", string(got))
	}
	if _, err := b.Finished(); err != errTruncated {
		t.Errorf("err = %v, want %v", err, errTruncated)
	}
}

// counting counts ReadAt calls on a shrinking source.
type counting struct {
	shrinking
	reads int
}

func (c *counting) ReadAt(p []byte, off int64) (int, error) {
	c.reads++
	return c.shrinking.ReadAt(p, off)
}

// TestShortBlockIsScannedOnce: a block that stays short is not read
// again for every line in it.
func TestShortBlockIsScannedOnce(t *testing.T) {
	src := &counting{}
	for i := 0; i < 2*blockLines; i++ {
		src.data = append(src.data, fmt.Sprintf("%d\n", i)...)
	}
	b := NewFrom(src)
	b.Write(src.data)
	src.data = src.data[:len(src.data)/4] // inside block 0
	var text []rune
	for i := 0; i < blockLines; i++ {
		text = b.Text(i, text)
		b.Line(i)
	}
	if src.reads > 2 {
		t.Errorf("%d reads for one short block, want at most 2", src.reads)
	}
}

// TestWriteToRoundTrip: -F prints the input as it came, from either
// backend.
func TestWriteToRoundTrip(t *testing.T) {
	in := "\x1b[31ma\r\nb\n\nc"
	for name, b := range map[string]*Buffer{"memory": New(), "file": NewFrom(strings.NewReader(in))} {
		Fill(strings.NewReader(in), b, func() {})
		var out bytes.Buffer
		if _, err := b.WriteTo(&out); err != nil || out.String() != in {
			t.Errorf("%s: WriteTo = %q, %v; want %q", name, out.String(), err, in)
		}
	}
}

// styledLines is n colored lines, about 80 bytes each.
func styledLines(n int) []byte {
	var in bytes.Buffer
	for i := 0; i < n; i++ {
		fmt.Fprintf(&in, "\x1b[32m%08d\x1b[0m some plain text, about eighty bytes wide, \x1b[1mbold\x1b[0m end\n", i)
	}
	return in.Bytes()
}

// TestMemoryPerLine: a memory-backed buffer keeps about its input plus
// the index; a file-backed one the index alone, which samples every
// blockLines lines, so it is small next to the lines. Retained heap,
// not allocation: parsing escapes allocates and frees as it goes.
func TestMemoryPerLine(t *testing.T) {
	const n = 100_000
	in := styledLines(n)
	size := int64(len(in))
	retained := func(b *Buffer, r io.Reader) int64 {
		var m0, m1 runtime.MemStats
		runtime.GC()
		runtime.ReadMemStats(&m0)
		Fill(r, b, func() {})
		runtime.GC()
		runtime.ReadMemStats(&m1)
		runtime.KeepAlive(b)
		return int64(m1.HeapAlloc) - int64(m0.HeapAlloc)
	}
	if got := retained(New(), bytes.NewReader(in)); got > size+2*n {
		t.Errorf("memory-backed: keeps %d bytes for %d of input, want at most input + 2/line", got, size)
	}
	if got := retained(NewFrom(bytes.NewReader(in)), bytes.NewReader(in)); got > 2*n {
		t.Errorf("file-backed: keeps %d bytes, want at most 2/line", got)
	}
}

// TestIndexAllocation: indexing allocates about the index, not copies
// of it: a growing slice would allocate several times its final size.
func TestIndexAllocation(t *testing.T) {
	const n = 100_000
	in := styledLines(n)
	var m0, m1 runtime.MemStats
	runtime.ReadMemStats(&m0)
	Fill(bytes.NewReader(in), NewFrom(bytes.NewReader(in)), func() {})
	runtime.ReadMemStats(&m1)
	if got := int64(m1.TotalAlloc - m0.TotalAlloc); got > 24*n {
		t.Errorf("file-backed: allocates %d bytes, want at most 24/line", got)
	}
}

// BenchmarkFill indexes 100k styled lines into memory.
func BenchmarkFill(b *testing.B) {
	in := styledLines(100_000)
	b.SetBytes(int64(len(in)))
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		Fill(bytes.NewReader(in), New(), func() {})
	}
}

// BenchmarkFillFile indexes 100k styled lines already on disk, keeping
// none.
func BenchmarkFillFile(b *testing.B) {
	in := styledLines(100_000)
	b.SetBytes(int64(len(in)))
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		Fill(bytes.NewReader(in), NewFrom(bytes.NewReader(in)), func() {})
	}
}

// TestLinesAcrossBlocks: lines read in any order come back right on
// both sides of a block edge, with the color set before the edge.
func TestLinesAcrossBlocks(t *testing.T) {
	var in strings.Builder
	for i := 0; i < 3*blockLines+5; i++ {
		if i == blockLines-1 {
			fmt.Fprintf(&in, "\x1b[31m%d\n", i)
		} else {
			fmt.Fprintf(&in, "%d\n", i)
		}
	}
	red := tcell.StyleDefault.Foreground(tcell.PaletteColor(1))
	for name, src := range map[string]func() *Buffer{
		"memory": New,
		"file":   func() *Buffer { return NewFrom(strings.NewReader(in.String())) },
	} {
		b := src()
		Fill(strings.NewReader(in.String()), b, func() {})
		for _, i := range []int{2*blockLines + 1, 0, blockLines, 3*blockLines + 4, blockLines - 1, 2*blockLines - 1, blockLines + 1, 3 * blockLines} {
			if got := string(b.Line(i).Text); got != fmt.Sprint(i) {
				t.Errorf("%s: line %d = %q", name, i, got)
			}
			if runs := b.Line(i).Runs; i >= blockLines-1 && (len(runs) != 1 || runs[0].Style != red) {
				t.Errorf("%s: line %d runs = %v, want red", name, i, runs)
			}
		}
	}
}

// TestLastBlockGrows: the last line is readable as the block it is in
// fills, and bytes after the last "\n" are not a line until Finish.
func TestLastBlockGrows(t *testing.T) {
	b := New()
	for i := 0; i < blockLines+3; i++ {
		b.Write([]byte(fmt.Sprint(i)))
		if n := b.Len(); n != i {
			t.Fatalf("before line %d's newline: Len = %d", i, n)
		}
		b.Write([]byte("\n"))
		if n := b.Len(); n != i+1 {
			t.Fatalf("after line %d: Len = %d", i, n)
		}
		if got := string(b.Line(i).Text); got != fmt.Sprint(i) {
			t.Fatalf("line %d = %q", i, got)
		}
	}
	b.Write([]byte("partial"))
	if n := b.Len(); n != blockLines+3 {
		t.Fatalf("Len = %d with a partial line pending", n)
	}
	b.Finish(nil, false)
	if got := string(b.Line(blockLines + 3).Text); got != "partial" {
		t.Fatalf("last line = %q, want partial", got)
	}
}

// TestShortReadIsError: a line whose bytes are gone is empty, and the
// read error says so, even after a clean Finish.
func TestShortReadIsError(t *testing.T) {
	src := &shrinking{data: []byte("one\ntwo\n")}
	b := NewFrom(src)
	b.Write(src.data)
	src.data = src.data[:5]
	if got := b.Line(1).Text; len(got) != 0 {
		t.Errorf("line 1 = %q, want empty", string(got))
	}
	if got := b.Line(0).Text; string(got) != "one" {
		t.Errorf("line 0 = %q, want one: its bytes are still there", string(got))
	}
	b.Finish(nil, true)
	if _, err := b.Finished(); err != errTruncated {
		t.Errorf("err = %v, want %v", err, errTruncated)
	}
}

// shrinking is a ReaderAt over data, which the test cuts.
type shrinking struct{ data []byte }

func (s *shrinking) ReadAt(p []byte, off int64) (int, error) {
	if off >= int64(len(s.data)) {
		return 0, io.EOF
	}
	n := copy(p, s.data[off:])
	if n < len(p) {
		return n, io.EOF
	}
	return n, nil
}

// TestReadersDuringFill runs every reader against a filling buffer,
// for the race detector: Write indexes outside the lock.
func TestReadersDuringFill(t *testing.T) {
	in := styledLines(20_000)
	for _, b := range []*Buffer{New(), NewFrom(bytes.NewReader(in))} {
		done := make(chan struct{})
		go func() {
			Fill(bytes.NewReader(in), b, func() {})
			close(done)
		}()
		var text []rune
		for eof := false; !eof; eof, _ = b.Finished() {
			n := b.Len()
			b.Line(n - 1)
			text = b.Text(n/2, text)
			b.WriteTo(io.Discard)
			b.TrailingNewline()
		}
		<-done
		if n := b.Len(); n != 20_000 {
			t.Fatalf("Len = %d, want 20000", n)
		}
	}
}
