package search

import (
	"fmt"
	"slices"
	"strings"
	"testing"

	"go.dlh.dev/vedi/internal/buffer"
)

func mk(lines ...string) *buffer.Buffer {
	b := buffer.New()
	buffer.Fill(strings.NewReader(strings.Join(lines, "\n")), b, func() {})
	return b
}

func at(line, col int) buffer.Pos { return buffer.Pos{Line: line, Col: col} }

func TestSmartcase(t *testing.T) {
	if New("abc").Find([]rune("xABCx"), 0) != 1 {
		t.Error("lower-case pattern should match case-insensitively")
	}
	if New("Abc").Find([]rune("xabcx"), 0) != -1 {
		t.Error("pattern with upper-case should match case-sensitively")
	}
	if New("Abc").Find([]rune("xAbcx"), 0) != 1 {
		t.Error("exact case should match")
	}
	if New("").Find([]rune("abc"), 0) != -1 || !New("").Empty() {
		t.Error("empty pattern never matches")
	}
}

func TestFindFromAndLast(t *testing.T) {
	m := New("ab")
	text := []rune("ab ab ab")
	if got := m.Find(text, 1); got != 3 {
		t.Errorf("Find from 1 = %d, want 3", got)
	}
	if got := m.Find(text, 7); got != -1 {
		t.Errorf("Find from 7 = %d, want -1", got)
	}
	if got := m.FindLast(text, 6); got != 3 {
		t.Errorf("FindLast before 6 = %d, want 3", got)
	}
	if got := m.FindLast(text, 0); got != -1 {
		t.Errorf("FindLast before 0 = %d, want -1", got)
	}
	if got := New("aa").All([]rune("aaaa")); !slices.Equal(got, []int{0, 1, 2}) {
		t.Errorf("All = %v, want [0 1 2] (overlapping)", got)
	}
}

func TestNextWraps(t *testing.T) {
	b := mk("foo", "bar foo", "foo")
	m := New("foo")
	steps := []struct {
		from    buffer.Pos
		after   bool
		want    buffer.Pos
		wrapped bool
	}{
		{at(0, 0), false, at(0, 0), false},
		{at(0, 0), true, at(1, 4), false},
		{at(1, 4), true, at(2, 0), false},
		{at(2, 0), true, at(0, 0), true},
		{at(1, 5), false, at(2, 0), false},
	}
	for _, s := range steps {
		pos, wrapped, found := Next(b, m, s.from, s.after)
		if !found || pos != s.want || wrapped != s.wrapped {
			t.Errorf("Next(%v, after=%v) = %v, wrapped=%v, found=%v; want %v, %v", s.from, s.after, pos, wrapped, found, s.want, s.wrapped)
		}
	}
	if _, _, found := Next(b, New("zzz"), buffer.Pos{}, false); found {
		t.Error("Next found a non-existent pattern")
	}
	if _, _, found := Next(buffer.New(), m, buffer.Pos{}, false); found {
		t.Error("Next found something in an empty buffer")
	}
}

func TestPrevWraps(t *testing.T) {
	b := mk("foo", "bar foo", "foo")
	m := New("foo")
	steps := []struct {
		from    buffer.Pos
		want    buffer.Pos
		wrapped bool
	}{
		{at(2, 0), at(1, 4), false},
		{at(1, 4), at(0, 0), false},
		{at(0, 0), at(2, 0), true},
		{at(1, 7), at(1, 4), false},
	}
	for _, s := range steps {
		pos, wrapped, found := Prev(b, m, s.from)
		if !found || pos != s.want || wrapped != s.wrapped {
			t.Errorf("Prev(%v) = %v, wrapped=%v, found=%v; want %v, %v", s.from, pos, wrapped, found, s.want, s.wrapped)
		}
	}
}

func TestSingleMatchWrapsToItself(t *testing.T) {
	b := mk("x", "foo", "y")
	pos, wrapped, found := Next(b, New("foo"), at(1, 0), true)
	if !found || pos != at(1, 0) || !wrapped {
		t.Errorf("Next = %v wrapped=%v found=%v, want (1,0) wrapped", pos, wrapped, found)
	}
}

// BenchmarkNextMiss searches 100k lines for a pattern that is not
// there, which must not allocate a line at a time.
func BenchmarkNextMiss(b *testing.B) {
	var in strings.Builder
	for i := 0; i < 100_000; i++ {
		fmt.Fprintf(&in, "\x1b[32mline %d\x1b[0m of some text\n", i)
	}
	buf := buffer.New()
	buffer.Fill(strings.NewReader(in.String()), buf, func() {})
	m := New("zzz")
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if _, _, found := Next(buf, m, buffer.Pos{}, false); found {
			b.Fatal("found")
		}
	}
}
