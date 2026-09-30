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
		{"éx", []int{0, 1, 1, 2}},                       // combining mark has zero width
		{"👨\u200d👩\u200d👧x", []int{0, 2, 2, 2, 2, 2, 3}}, // a ZWJ sequence is one cell pair
		{"🇺🇸x", []int{0, 2, 2, 3}},                       // so is a flag
		{"❤\ufe0fx", []int{0, 2, 2, 3}},                  // an emoji presentation selector widens its base
	}
	for _, tc := range tests {
		if got := (Layout{}).Cells([]rune(tc.in)); !reflect.DeepEqual(got, tc.want) {
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
		{"wrap even", Layout{Width: 4, Mode: Wrap}, "abcdefghij", []Segment{{0, 4}, {4, 8}, {8, 10}}},
		{"wrap exact", Layout{Width: 4, Mode: Wrap}, "abcdefgh", []Segment{{0, 4}, {4, 8}}},
		{"wrap short", Layout{Width: 10, Mode: Wrap}, "abc", []Segment{{0, 3}}},
		{"wrap empty", Layout{Width: 10, Mode: Wrap}, "", []Segment{{0, 0}}},
		{"wide moves down", Layout{Width: 5, Mode: Wrap}, "日本語", []Segment{{0, 2}, {2, 3}}},
		{"tab moves down", Layout{Width: 6, Mode: Wrap}, "abcd\tx", []Segment{{0, 4}, {4, 6}}},
		{"rune wider than width", Layout{Width: 1, Mode: Wrap}, "日本", []Segment{{0, 1}, {1, 2}}},
		{"nowrap", Layout{Width: 4, Mode: NoWrap}, "abcdefghij", []Segment{{0, 10}}},
		{"zero width", Layout{Width: 0, Mode: Wrap}, "abc", []Segment{{0, 3}}},
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
				l := Layout{Width: width, Mode: mode}
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
	return col < len(text) && Layout{}.RuneWidth(text[col], 0) == 0
}

func TestPos(t *testing.T) {
	l := Layout{Width: 4, Mode: Wrap}
	text := []rune("abcdefghij")
	tests := []struct{ col, row, x int }{{0, 0, 0}, {3, 0, 3}, {4, 1, 0}, {9, 2, 1}, {10, 2, 2}}
	for _, tc := range tests {
		if row, x := l.Pos(text, tc.col); row != tc.row || x != tc.x {
			t.Errorf("Pos(%d) = (%d,%d), want (%d,%d)", tc.col, row, x, tc.row, tc.x)
		}
	}
	if row, x := (Layout{Width: 4, Mode: NoWrap}).Pos(text, 9); row != 0 || x != 9 {
		t.Errorf("nowrap Pos(9) = (%d,%d), want (0,9)", row, x)
	}
}

func TestColNeverInsideWideRune(t *testing.T) {
	l := Layout{Width: 10, Mode: NoWrap}
	text := []rune("日本")
	for x, want := range []int{0, 0, 1, 1, 2, 2} {
		if got := l.Col(text, 0, x); got != want {
			t.Errorf("Col(x=%d) = %d, want %d", x, got, want)
		}
	}
}

func TestColPastRowEnd(t *testing.T) {
	l := Layout{Width: 4, Mode: Wrap}
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
	// abce\u0301 | fgh: the first row ends in a combining mark, which has
	// no cell; past the end is its base rune.
	text = []rune("abce\u0301fgh")
	if got := l.Col(text, 0, 99); got != 3 {
		t.Errorf("Col past end of row ending in a mark = %d, want 3", got)
	}
}

func TestNewlineRow(t *testing.T) {
	tests := []struct {
		name string
		l    Layout
		in   string
		want []Segment
	}{
		{"full", Layout{Width: 4, Mode: Wrap}, "abcd", []Segment{{0, 4}, {4, 4}}},
		{"full last row", Layout{Width: 4, Mode: Wrap}, "abcdefgh", []Segment{{0, 4}, {4, 8}, {8, 8}}},
		{"wider than width", Layout{Width: 1, Mode: Wrap}, "日", []Segment{{0, 1}, {1, 1}}},
		{"short", Layout{Width: 4, Mode: Wrap}, "abc", []Segment{{0, 3}}},
		{"empty", Layout{Width: 4, Mode: Wrap}, "", []Segment{{0, 0}}},
		{"nowrap", Layout{Width: 4, Mode: NoWrap}, "abcd", []Segment{{0, 4}}},
		{"zero width", Layout{Width: 0, Mode: Wrap}, "abcd", []Segment{{0, 4}}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			ln := tc.l.Line([]rune(tc.in))
			before := len(ln.Segments())
			got := tc.l.NewlineRow(ln).Segments()
			if !reflect.DeepEqual(got, tc.want) {
				t.Fatalf("NewlineRow = %v, want %v", got, tc.want)
			}
			if len(ln.Segments()) != before {
				t.Fatal("NewlineRow changed its argument")
			}
		})
	}
}

func TestSegmentAt(t *testing.T) {
	l := Layout{Width: 4, Mode: Wrap}
	text := []rune("abcdefghij")
	for col, want := range map[int]int{0: 0, 3: 0, 4: 1, 8: 2, 10: 2} {
		if got := l.SegmentAt(text, col); got != want {
			t.Errorf("SegmentAt(%d) = %d, want %d", col, got, want)
		}
	}
}

// TestTabWidth: a tab reaches the next multiple of Tab; zero is 8.
func TestTabWidth(t *testing.T) {
	for _, tc := range []struct {
		tab  int
		in   string
		want []int
	}{
		{4, "a\tb", []int{0, 1, 4, 5}},
		{4, "\t\t", []int{0, 4, 8}},
		{4, "abcd\tx", []int{0, 1, 2, 3, 4, 8, 9}},
		{1, "a\tb", []int{0, 1, 2, 3}},
		{0, "a\tb", []int{0, 1, 8, 9}},
	} {
		l := Layout{Width: 20, Mode: Wrap, Tab: tc.tab}
		if got := l.Cells([]rune(tc.in)); !reflect.DeepEqual(got, tc.want) {
			t.Errorf("Layout{Tab: %d}.Cells(%q) = %v, want %v", tc.tab, tc.in, got, tc.want)
		}
	}
	if got := (Layout{Width: 6, Mode: Wrap, Tab: 4}).Segments([]rune("ab\tcdef")); !reflect.DeepEqual(got, []Segment{{0, 5}, {5, 7}}) {
		t.Errorf("Segments = %v, want [{0 5} {5 7}]", got)
	}
}

func TestGlyph(t *testing.T) {
	// 👨ZWJ👩ZWJ👧 x e ́ : glyphs [0,5) [5,6) [6,8), then the newline.
	ln := Layout{}.Line([]rune("👨‍👩‍👧xé"))
	for col, want := range [][2]int{{0, 5}, {0, 5}, {0, 5}, {0, 5}, {0, 5}, {5, 6}, {6, 8}, {6, 8}, {8, 9}} {
		if i, j := ln.Glyph(col); i != want[0] || j != want[1] {
			t.Errorf("Glyph(%d) = [%d,%d), want [%d,%d)", col, i, j, want[0], want[1])
		}
	}
}
