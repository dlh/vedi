package buffer

import (
	"errors"
	"io"
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
	if calls < 3 {
		t.Fatalf("notify called %d times, want at least 3 (two lines plus finish)", calls)
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

func TestRecorderKeepsWhatWasRead(t *testing.T) {
	r := Record(strings.NewReader("ab\ncd\n"))
	if b, err := io.ReadAll(r); err != nil || string(b) != "ab\ncd\n" {
		t.Fatalf("ReadAll = %q, %v", b, err)
	}
	if got := string(r.Bytes()); got != "ab\ncd\n" {
		t.Errorf("Bytes = %q", got)
	}
}

func TestRecorderStopDrops(t *testing.T) {
	r := Record(strings.NewReader("ab\ncd\n"))
	p := make([]byte, 3)
	if _, err := io.ReadFull(r, p); err != nil {
		t.Fatal(err)
	}
	r.Stop()
	if _, err := io.ReadAll(r); err != nil {
		t.Fatal(err)
	}
	if got := r.Bytes(); got != nil {
		t.Errorf("Bytes after Stop = %q, want nil", got)
	}
}
