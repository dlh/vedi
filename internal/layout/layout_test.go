package layout

import (
	"reflect"
	"testing"
)

func TestCells(t *testing.T) {
	tests := []struct {
		in   string
		want []int
	}{
		{"", []int{0}},
		{"abc", []int{0, 1, 2, 3}},
		{"a\tb", []int{0, 1, 8, 9}},
		{"\t\t", []int{0, 8, 16}},
		{"日本", []int{0, 2, 4}},
		{"a\rb", []int{0, 1, 3, 4}},
		{"éx", []int{0, 1, 1, 2}}, // combining mark has zero width
	}
	for _, tc := range tests {
		if got := Cells([]rune(tc.in)); !reflect.DeepEqual(got, tc.want) {
			t.Errorf("Cells(%q) = %v, want %v", tc.in, got, tc.want)
		}
	}
}

func TestSegments(t *testing.T) {
	tests := []struct {
		name string
		l    Layout
		in   string
		want []Segment
	}{
		{"wrap even", Layout{4, Wrap}, "abcdefghij", []Segment{{0, 4}, {4, 8}, {8, 10}}},
		{"wrap exact", Layout{4, Wrap}, "abcdefgh", []Segment{{0, 4}, {4, 8}}},
		{"wrap short", Layout{10, Wrap}, "abc", []Segment{{0, 3}}},
		{"wrap empty", Layout{10, Wrap}, "", []Segment{{0, 0}}},
		{"wide moves down", Layout{5, Wrap}, "日本語", []Segment{{0, 2}, {2, 3}}},
		{"tab moves down", Layout{6, Wrap}, "abcd\tx", []Segment{{0, 4}, {4, 6}}},
		{"rune wider than width", Layout{1, Wrap}, "日本", []Segment{{0, 1}, {1, 2}}},
		{"nowrap", Layout{4, NoWrap}, "abcdefghij", []Segment{{0, 10}}},
		{"zero width", Layout{0, Wrap}, "abc", []Segment{{0, 3}}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := tc.l.Segments([]rune(tc.in))
			if !reflect.DeepEqual(got, tc.want) {
				t.Fatalf("Segments = %v, want %v", got, tc.want)
			}
			if r := tc.l.Rows([]rune(tc.in)); r != len(tc.want) {
				t.Fatalf("Rows = %d, want %d", r, len(tc.want))
			}
		})
	}
}

func TestPosColRoundTrip(t *testing.T) {
	texts := []string{"", "abc", "abcdefghij", "日本語テキスト", "a\tb\tc", "x\ry", "éabc"}
	for _, s := range texts {
		text := []rune(s)
		for _, mode := range []Mode{Wrap, NoWrap} {
			for width := 1; width <= 12; width++ {
				l := Layout{width, mode}
				for col := 0; col <= len(text); col++ {
					row, x := l.Pos(text, col)
					got := l.Col(text, row, x)
					if got != col && !zeroWidthAt(text, col) {
						t.Errorf("%q %v w=%d: Col(Pos(%d)=(%d,%d)) = %d", s, mode, width, col, row, x, got)
					}
				}
			}
		}
	}
}

// zeroWidthAt reports whether the rune at col has zero width; Col cannot
// land on such a rune, which is intended.
func zeroWidthAt(text []rune, col int) bool {
	return col < len(text) && RuneWidth(text[col], 0) == 0
}

func TestPos(t *testing.T) {
	l := Layout{4, Wrap}
	text := []rune("abcdefghij")
	tests := []struct{ col, row, x int }{{0, 0, 0}, {3, 0, 3}, {4, 1, 0}, {9, 2, 1}, {10, 2, 2}}
	for _, tc := range tests {
		if row, x := l.Pos(text, tc.col); row != tc.row || x != tc.x {
			t.Errorf("Pos(%d) = (%d,%d), want (%d,%d)", tc.col, row, x, tc.row, tc.x)
		}
	}
	if row, x := (Layout{4, NoWrap}).Pos(text, 9); row != 0 || x != 9 {
		t.Errorf("nowrap Pos(9) = (%d,%d), want (0,9)", row, x)
	}
}

func TestColNeverInsideWideRune(t *testing.T) {
	l := Layout{10, NoWrap}
	text := []rune("日本")
	for x, want := range []int{0, 0, 1, 1, 2, 2} {
		if got := l.Col(text, 0, x); got != want {
			t.Errorf("Col(x=%d) = %d, want %d", x, got, want)
		}
	}
}

func TestColPastRowEnd(t *testing.T) {
	l := Layout{4, Wrap}
	text := []rune("abcdefghij")
	if got := l.Col(text, 0, 99); got != 3 {
		t.Errorf("Col past end of wrapped row = %d, want 3", got)
	}
	if got := l.Col(text, 2, 99); got != 10 {
		t.Errorf("Col past end of last row = %d, want 10", got)
	}
	if got := l.Col(text, 99, 0); got != 8 {
		t.Errorf("Col with row out of range = %d, want 8 (clamped to last row)", got)
	}
}

func TestSegmentAt(t *testing.T) {
	l := Layout{4, Wrap}
	text := []rune("abcdefghij")
	for col, want := range map[int]int{0: 0, 3: 0, 4: 1, 8: 2, 10: 2} {
		if got := l.SegmentAt(text, col); got != want {
			t.Errorf("SegmentAt(%d) = %d, want %d", col, got, want)
		}
	}
}
