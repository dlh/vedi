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
