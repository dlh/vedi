// Package layout maps logical (line, rune index) positions to visual
// (row, cell) positions for a given width and wrap mode.
package layout

import (
	"os"
	"slices"
	"sort"
	"strings"
	"unicode/utf8"

	"github.com/clipperhouse/displaywidth"
)

// widths measures as tcell does, down to the environment variable
// that widens East Asian ambiguous characters: a cluster must take
// the cells the screen gives it.
var widths = func() displaywidth.Options {
	switch strings.ToLower(os.Getenv("RUNEWIDTH_EASTASIAN")) {
	case "1", "true", "yes":
		return displaywidth.Options{EastAsianWidth: true}
	}
	return displaywidth.Options{}
}()

type Mode int

const (
	Wrap Mode = iota
	NoWrap
)

// WrapStyle is where Wrap ends a row.
type WrapStyle int

const (
	WrapStyleChar WrapStyle = iota // at the screen's edge
	WrapStyleWord                  // after a space or tab
)

// DefaultTab is the tab width when Layout.Tab is zero.
const DefaultTab = 8

// RuneWidth is the cells r takes on its own starting at cell x: a tab
// to the next tab stop, a C0 control or DEL two (drawn as ^X), anything
// else as tcell measures it, 0 for combining marks. Cells measures
// runes in their clusters; this is for a rune known to stand alone.
func (l Layout) RuneWidth(r rune, x int) int {
	switch {
	case r == '\t':
		tab := l.Tab
		if tab <= 0 {
			tab = DefaultTab
		}
		return tab - x%tab
	case r < 0x20 || r == 0x7f:
		return 2
	case r < 0x7f:
		return 1
	}
	return widths.Rune(r)
}

// Cells returns len(text)+1 entries: xs[i] is the cell column where rune
// i starts and xs[len(text)] is the total width. A grapheme cluster (a
// letter with its marks, an emoji sequence, a flag) is measured whole
// as tcell draws it: its first rune takes the cells, the rest take none.
func (l Layout) Cells(text []rune) []int {
	n := len(text)
	xs := make([]int, n+1)
	x := 0
	for i := 0; i < n; {
		// Two ASCII runes in a row never share a cluster, so a run of
		// ASCII is measured rune by rune without the segmenter.
		if text[i] < 0x80 && (i+1 == n || text[i+1] < 0x80) {
			xs[i] = x
			x += l.RuneWidth(text[i], x)
			i++
			continue
		}
		// The run text[i:k] ends after the first ASCII pair, or at the
		// end: every cluster in it lies within it.
		k := i + 1
		for k < n && !(text[k-1] < 0x80 && text[k] < 0x80) {
			k++
		}
		for g := widths.StringGraphemes(string(text[i:k])); g.Next(); {
			w := g.Width()
			m := utf8.RuneCountInString(g.Value())
			if r := text[i]; r < 0x20 || r == 0x7f {
				w = l.RuneWidth(r, x) // a control is its own cluster: ^X, or the tab stop
			}
			xs[i] = x
			x += w
			for j := i + 1; j < i+m; j++ {
				xs[j] = x
			}
			i += m
		}
	}
	xs[n] = x
	return xs
}

// Segment is one visual row of a logical line: runes [Start, End).
type Segment struct{ Start, End int }

type Layout struct {
	Width     int
	Mode      Mode
	Tab       int       // cells per tab stop; zero is DefaultTab
	WrapStyle WrapStyle // where Wrap ends a row
}

// Line is text laid out once: its cell columns and visual rows. Lay a
// line out once and keep it; every method is then cheap.
type Line struct {
	xs   []int
	segs []Segment
}

// Line lays text out. NoWrap, or a width of zero, gives one row. Wrap
// moves a rune that does not fit to the next row; with WrapStyleWord, the
// word it is in, when that leaves something on the row. An empty line
// is one empty row.
func (l Layout) Line(text []rune) Line {
	xs := l.Cells(text)
	n := len(text)
	if l.Mode == NoWrap || l.Width <= 0 {
		return Line{xs, []Segment{{0, n}}}
	}
	if l.WrapStyle == WrapStyleWord {
		return Line{xs, l.wordRows(text, xs)}
	}
	var segs []Segment
	start, x0 := 0, 0
	for i := range n {
		w := xs[i+1] - xs[i]
		if xs[i]-x0+w > l.Width && i > start {
			segs = append(segs, Segment{start, i})
			start, x0 = i, xs[i]
		}
	}
	return Line{xs, append(segs, Segment{start, n})}
}

// wordRows breaks text into rows after the last space or tab that fits,
// so a row begins with a word. A word wider than the row breaks by
// rune, as a run of spaces does.
func (l Layout) wordRows(text []rune, xs []int) []Segment {
	var segs []Segment
	start, x0 := 0, 0
	brk, space := 0, false // where the row may break; the glyph before is a space
	for i := range text {
		w := xs[i+1] - xs[i]
		if w == 0 {
			continue // the rest of a cluster
		}
		if space {
			brk = i
		}
		space = text[i] == ' ' || text[i] == '\t'
		// Twice when a wide rune still does not fit after its word moved.
		for xs[i]-x0+w > l.Width && i > start {
			at := i
			if brk > start {
				at = brk
			}
			segs = append(segs, Segment{start, at})
			start, x0 = at, xs[at]
		}
	}
	return append(segs, Segment{start, len(text)})
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
	segs := append(slices.Clip(ln.segs), Segment{n, n})
	return Line{ln.xs, segs}
}

// Cells is the line's cell columns, as the Cells function gives them.
func (ln Line) Cells() []int { return ln.xs }

// Glyph returns the runes [i, j) drawn as one glyph with rune col: it
// and the rest of its cluster, which take no cells of their own. The
// newline after the last rune is a glyph of its own.
func (ln Line) Glyph(col int) (i, j int) {
	n := len(ln.xs) - 1
	i = col
	for i > 0 && i < n && ln.xs[i+1] == ln.xs[i] {
		i--
	}
	j = i + 1
	for j < n && ln.xs[j+1] == ln.xs[j] {
		j++
	}
	return i, j
}

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
