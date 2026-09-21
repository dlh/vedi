// Package search finds plain substrings in the buffer, with smartcase.
package search

import (
	"math"
	"unicode"

	"go.dlh.dev/vedi/internal/buffer"
)

// Matcher matches a fixed pattern; one without upper-case letters
// ignores case.
type Matcher struct {
	src  string // the pattern as given
	pat  []rune
	fold bool
}

func New(pattern string) Matcher {
	pat := []rune(pattern)
	fold := true
	for _, r := range pat {
		if unicode.IsUpper(r) {
			fold = false
			break
		}
	}
	if fold {
		for i, r := range pat {
			pat[i] = unicode.ToLower(r)
		}
	}
	return Matcher{src: pattern, pat: pat, fold: fold}
}

// Pattern is the pattern as given to New.
func (m Matcher) Pattern() string { return m.src }

func (m Matcher) Empty() bool { return len(m.pat) == 0 }
func (m Matcher) Len() int    { return len(m.pat) }

func (m Matcher) matchAt(text []rune, i int) bool {
	if m.Empty() || i < 0 || i+len(m.pat) > len(text) {
		return false
	}
	for k, p := range m.pat {
		r := text[i+k]
		if m.fold {
			r = unicode.ToLower(r)
		}
		if r != p {
			return false
		}
	}
	return true
}

// Find returns the first match starting at or after from, or -1.
func (m Matcher) Find(text []rune, from int) int {
	for i := max(from, 0); i+len(m.pat) <= len(text); i++ {
		if m.matchAt(text, i) {
			return i
		}
	}
	return -1
}

// FindLast returns the last match starting before `before`, or -1.
func (m Matcher) FindLast(text []rune, before int) int {
	for i := min(before-1, len(text)-len(m.pat)); i >= 0; i-- {
		if m.matchAt(text, i) {
			return i
		}
	}
	return -1
}

// All returns the start of every match in text; matches may overlap.
func (m Matcher) All(text []rune) []int {
	var out []int
	for i := m.Find(text, 0); i >= 0; i = m.Find(text, i+1) {
		out = append(out, i)
	}
	return out
}

// Next returns the first match at or after from (strictly after when
// after is set), wrapping around the end; wrapped reports that it did.
func Next(buf *buffer.Buffer, m Matcher, from buffer.Pos, after bool) (pos buffer.Pos, wrapped, found bool) {
	n := buf.Len()
	if n == 0 || m.Empty() {
		return
	}
	col := from.Col
	if after {
		col++
	}
	var text []rune
	for k := 0; k <= n; k++ {
		li := (from.Line + k) % n
		if k > 0 {
			col = 0
		}
		text = buf.Text(li, text)
		if i := m.Find(text, col); i >= 0 {
			return buffer.Pos{Line: li, Col: i}, from.Line+k >= n, true
		}
	}
	return
}

// Prev returns the last match before from, wrapping around the start;
// wrapped reports that it did.
func Prev(buf *buffer.Buffer, m Matcher, from buffer.Pos) (pos buffer.Pos, wrapped, found bool) {
	n := buf.Len()
	if n == 0 || m.Empty() {
		return
	}
	var text []rune
	for k := 0; k <= n; k++ {
		li := ((from.Line-k)%n + n) % n
		before := math.MaxInt
		if k == 0 {
			before = from.Col
		}
		text = buf.Text(li, text)
		if i := m.FindLast(text, before); i >= 0 {
			return buffer.Pos{Line: li, Col: i}, from.Line-k < 0, true
		}
	}
	return
}
