// Package layout maps logical (line, rune index) positions to visual
// (row, cell) positions for a given width and wrap mode.
package layout

import (
	"sort"

	"github.com/rivo/uniseg"
)

type Mode int

const (
	Wrap Mode = iota
	NoWrap
)

const TabWidth = 8

// RuneWidth is the cells r takes starting at cell x: a tab to the next
// tab stop, a C0 control or DEL two (drawn as ^X), anything else as
// uniseg measures it, 0 for combining marks.
func RuneWidth(r rune, x int) int {
	switch {
	case r == '\t':
		return TabWidth - x%TabWidth
	case r < 0x20 || r == 0x7f:
		return 2
	}
	return uniseg.StringWidth(string(r))
}

// ZeroWidth reports whether r takes no cell of its own: a combining
// mark, drawn on the rune before it.
func ZeroWidth(r rune) bool { return RuneWidth(r, 0) == 0 }

// Cells returns len(text)+1 entries: xs[i] is the cell column where rune
// i starts and xs[len(text)] is the total width.
func Cells(text []rune) []int {
	xs := make([]int, len(text)+1)
	x := 0
	for i, r := range text {
		xs[i] = x
		x += RuneWidth(r, x)
	}
	xs[len(text)] = x
	return xs
}

// Segment is one visual row of a logical line: runes [Start, End).
type Segment struct{ Start, End int }

type Layout struct {
	Width int
	Mode  Mode
}

// Line is text laid out once: its cell columns and visual rows. Lay a
// line out once and keep it; every method is then cheap.
type Line struct {
	xs   []int
	segs []Segment
}

// Line lays text out. NoWrap, or a width of zero, gives one row. Wrap
// moves a rune that does not fit to the next row. An empty line is one
// empty row.
func (l Layout) Line(text []rune) Line {
	xs := Cells(text)
	n := len(text)
	if l.Mode == NoWrap || l.Width <= 0 {
		return Line{xs, []Segment{{0, n}}}
	}
	var segs []Segment
	start, x0 := 0, 0
	for i := 0; i < n; i++ {
		w := xs[i+1] - xs[i]
		if xs[i]-x0+w > l.Width && i > start {
			segs = append(segs, Segment{start, i})
			start, x0 = i, xs[i]
		}
	}
	return Line{xs, append(segs, Segment{start, n})}
}

// NewlineRow returns ln, laid out by l, with an empty row after its last
// when that row is full: a place for the cursor on the newline, which
// has no cell of its own. Otherwise ln is returned as is.
func (l Layout) NewlineRow(ln Line) Line {
	n := len(ln.xs) - 1
	last := ln.segs[len(ln.segs)-1]
	if l.Mode == NoWrap || l.Width <= 0 || ln.xs[n]-ln.xs[last.Start] < l.Width {
		return ln
	}
	segs := append(ln.segs[:len(ln.segs):len(ln.segs)], Segment{n, n})
	return Line{ln.xs, segs}
}

// Cells is the line's cell columns, as the Cells function gives them.
func (ln Line) Cells() []int { return ln.xs }

// Segments is the line's visual rows.
func (ln Line) Segments() []Segment { return ln.segs }

// Rows is the number of visual rows.
func (ln Line) Rows() int { return len(ln.segs) }

// SegmentAt returns the index of the segment containing rune index col.
// col == len(text) belongs to the last segment.
func (ln Line) SegmentAt(col int) int {
	i := sort.Search(len(ln.segs), func(i int) bool { return col < ln.segs[i].End })
	return min(i, len(ln.segs)-1)
}

// Pos maps rune index col, clamped to 0..len(text), to its row and cell
// x within it.
func (ln Line) Pos(col int) (row, x int) {
	col = max(0, min(col, len(ln.xs)-1))
	row = ln.SegmentAt(col)
	return row, ln.xs[col] - ln.xs[ln.segs[row].Start]
}

// Col maps a row and cell x back to the rune whose cells hold x. Past
// the row's end it is len(text) on the last row and the last rune with
// a cell on any other, so the cursor stays on that row. Rows are
// clamped.
func (ln Line) Col(row, x int) int {
	row = max(0, min(row, len(ln.segs)-1))
	s := ln.segs[row]
	for i := s.Start; i < s.End; i++ {
		if ln.xs[i+1]-ln.xs[s.Start] > x {
			return i
		}
	}
	if row == len(ln.segs)-1 {
		return len(ln.xs) - 1
	}
	i := s.End - 1
	for i > s.Start && ln.xs[i+1] == ln.xs[i] {
		i--
	}
	return i
}

// Segments, Rows, SegmentAt, Pos and Col lay text out and answer once;
// see Line for repeated use.
func (l Layout) Segments(text []rune) []Segment        { return l.Line(text).Segments() }
func (l Layout) Rows(text []rune) int                  { return l.Line(text).Rows() }
func (l Layout) SegmentAt(text []rune, col int) int    { return l.Line(text).SegmentAt(col) }
func (l Layout) Pos(text []rune, col int) (row, x int) { return l.Line(text).Pos(col) }
func (l Layout) Col(text []rune, row, x int) int       { return l.Line(text).Col(row, x) }
