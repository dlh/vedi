package search

import (
	"fmt"
	"math"
	"slices"
	"strings"
	"testing"
	"unicode"

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
	for i := range 100_000 {
		fmt.Fprintf(&in, "\x1b[32mline %d\x1b[0m of some text\n", i)
	}
	buf := buffer.New()
	buffer.Fill(strings.NewReader(in.String()), buf, func() {})
	m := New("zzz")
	b.ReportAllocs()
	for b.Loop() {
		if _, _, found := Next(buf, m, buffer.Pos{}, false); found {
			b.Fatal("found")
		}
	}
}

// BenchmarkNextMissEveryLine is the miss that rules no block out: the
// pattern's rarest byte is on every line.
func BenchmarkNextMissEveryLine(b *testing.B) {
	var in strings.Builder
	for i := range 100_000 {
		fmt.Fprintf(&in, "\x1b[32mline %d\x1b[0m of some text\n", i)
	}
	buf := buffer.New()
	buffer.Fill(strings.NewReader(in.String()), buf, func() {})
	m := New("some texts")
	b.ReportAllocs()
	for b.Loop() {
		if _, _, found := Next(buf, m, buffer.Pos{}, false); found {
			b.Fatal("found")
		}
	}
}

// TestMayMatch: the bytes of a line rule it out only when its text
// cannot match.
func TestMayMatch(t *testing.T) {
	for _, tc := range []struct {
		pat, line string
		want      bool
	}{
		{"foo", "a foo", true},
		{"foo", "a FOO", true},
		{"foo", "fo\x1b[31mo", true},
		{"foo", "bar", false},
		{"foo", "fo", false},
		{"foo", "\x1b]0;foo\x07bar", false},
		{"foo", "ab\x1b]0;foo", false},
		{"Foo", "a foo", false},
		{"Foo", "a Foo", true},
		// A dropped control may sit inside a match.
		{"foo", "f\x01oo", true},
		{"foo", "f\x7foo", true},
		{"foo", "f\too", false},
		// K and İ fold to k and i.
		{"kelvin", "\u212Aelvin", true},
		{"istanbul", "\u0130stanbul", true},
		{"elvin", "\u212Aelvin", true},
		{"foo", "\u212Aelvin", false},
		// Bytes do not fold past ASCII, so they rule nothing out.
		{"é", "plain", true},
		{"É", "plain", false},
		{"É", "CAFÉ", true},
		{"\uFFFD", "plain", true},
		{"kiki", "plain", true},
	} {
		var scratch []byte
		if got := New(tc.pat).mayMatch([]byte(tc.line), &scratch); got != tc.want {
			t.Errorf("New(%q).mayMatch(%q) = %v, want %v", tc.pat, tc.line, got, tc.want)
		}
	}
}

// TestOnlyTwoRunesFoldToASCII: mayMatch counts on K and İ being the
// only runes past ASCII that lower-case into it.
func TestOnlyTwoRunesFoldToASCII(t *testing.T) {
	for r := rune(0x80); r <= unicode.MaxRune; r++ {
		if l := unicode.ToLower(r); l < 0x80 && r != 0x212A && r != 0x0130 {
			t.Errorf("%U lowers to %q", r, l)
		}
	}
}

// refNext and refPrev decode and search every line: what Next and
// Prev must agree with.
func refNext(buf *buffer.Buffer, m Matcher, from buffer.Pos, after bool) (pos buffer.Pos, wrapped, found bool) {
	n := buf.Len()
	if n == 0 || m.Empty() {
		return
	}
	col := from.Col
	if after {
		col++
	}
	for k := 0; k <= n; k++ {
		li := (from.Line + k) % n
		if k > 0 {
			col = 0
		}
		if i := m.Find(buf.Text(li, nil), col); i >= 0 {
			return buffer.Pos{Line: li, Col: i}, from.Line+k >= n, true
		}
	}
	return
}

func refPrev(buf *buffer.Buffer, m Matcher, from buffer.Pos) (pos buffer.Pos, wrapped, found bool) {
	n := buf.Len()
	if n == 0 || m.Empty() {
		return
	}
	for k := 0; k <= n; k++ {
		li := ((from.Line-k)%n + n) % n
		before := math.MaxInt
		if k == 0 {
			before = from.Col
		}
		if i := m.FindLast(buf.Text(li, nil), before); i >= 0 {
			return buffer.Pos{Line: li, Col: i}, from.Line-k < 0, true
		}
	}
	return
}

// TestWalkMatchesEveryLineDecoded: from every position, Next and Prev
// find what decoding every line finds, over text that bytes alone
// misjudge and across block edges.
func TestWalkMatchesEveryLineDecoded(t *testing.T) {
	odd := []string{
		"foo bar foo",
		"fo\x1b[31mo\x1b[0m",
		"f\x01oo",
		"FOO Foo",
		"\u212Aelvin \u0130stanbul",
		"caf\xc3\xa9 CAF\xc3\x89",
		"a\xffb",
		"zoo\r",
		"ab \x1b]0;foo",
		"\x1b]0;foo\x07bar",
		"",
	}
	var lines []string
	for i := range 2*buffer.BlockLines + 9 {
		lines = append(lines, "x")
		if i%13 == 5 {
			lines[i] = odd[i/13%len(odd)]
		}
	}
	lines[buffer.BlockLines-1], lines[buffer.BlockLines] = "foo", "bar"
	lines = append(lines, "end foo")
	for _, in := range []string{strings.Join(lines, "\n"), strings.Join(lines[:3], "\r\n") + "\r\n", "foo", "x"} {
		buf := buffer.New()
		buffer.Fill(strings.NewReader(in), buf, func() {})
		for _, pat := range []string{"foo", "Foo", "FOO", "oo", "bar", "k", "i", "ki", "kelvin", "istanbul", "é", "É", "café", "\uFFFD", "a\uFFFDb", "x", "zzz", "end"} {
			m := New(pat)
			for li := range buf.Len() {
				for col := 0; col <= len(buf.Text(li, nil)); col++ {
					from := at(li, col)
					for _, after := range []bool{false, true} {
						pos, wrapped, found := Next(buf, m, from, after)
						wpos, wwrapped, wfound := refNext(buf, m, from, after)
						if pos != wpos || wrapped != wwrapped || found != wfound {
							t.Fatalf("Next(%q, %v, %v) = %v, %v, %v; want %v, %v, %v", pat, from, after, pos, wrapped, found, wpos, wwrapped, wfound)
						}
					}
					pos, wrapped, found := Prev(buf, m, from)
					wpos, wwrapped, wfound := refPrev(buf, m, from)
					if pos != wpos || wrapped != wwrapped || found != wfound {
						t.Fatalf("Prev(%q, %v) = %v, %v, %v; want %v, %v, %v", pat, from, pos, wrapped, found, wpos, wwrapped, wfound)
					}
				}
			}
		}
	}
}
