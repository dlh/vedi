// Package layout maps logical (line, rune index) positions to visual
// (row, cell) positions for a given width and wrap mode.
package layout

import "github.com/rivo/uniseg"

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

// Segments splits text into visual rows. NoWrap, or a width of zero,
// gives one row. Wrap moves a rune that does not fit to the next row.
// An empty line is one empty row.
func (l Layout) Segments(text []rune) []Segment {
	if l.Mode == NoWrap || l.Width <= 0 {
		return []Segment{{0, len(text)}}
	}
	xs := Cells(text)
	var segs []Segment
	start, x0 := 0, 0
	for i := range text {
		w := xs[i+1] - xs[i]
		if xs[i]-x0+w > l.Width && i > start {
			segs = append(segs, Segment{start, i})
			start, x0 = i, xs[i]
		}
	}
	return append(segs, Segment{start, len(text)})
}

// Rows is the number of visual rows text occupies.
func (l Layout) Rows(text []rune) int { return len(l.Segments(text)) }

// SegmentAt returns the index of the segment containing rune index col.
// col == len(text) belongs to the last segment.
func (l Layout) SegmentAt(text []rune, col int) int {
	segs := l.Segments(text)
	for i, s := range segs {
		if col < s.End {
			return i
		}
	}
	return len(segs) - 1
}

// Pos maps rune index col, clamped to 0..len(text), to its row and cell
// x within it.
func (l Layout) Pos(text []rune, col int) (row, x int) {
	col = max(0, min(col, len(text)))
	xs := Cells(text)
	row = l.SegmentAt(text, col)
	return row, xs[col] - xs[l.Segments(text)[row].Start]
}

// Col maps a row and cell x back to the rune whose cells hold x. Past
// the row's end it is len(text) on the last row and End-1 on any other,
// so the cursor stays on that row. Rows are clamped.
func (l Layout) Col(text []rune, row, x int) int {
	segs := l.Segments(text)
	row = max(0, min(row, len(segs)-1))
	s := segs[row]
	xs := Cells(text)
	for i := s.Start; i < s.End; i++ {
		if xs[i+1]-xs[s.Start] > x {
			return i
		}
	}
	if row == len(segs)-1 {
		return len(text)
	}
	return s.End - 1
}
