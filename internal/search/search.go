// Package search finds plain substrings in the buffer, with smartcase.
package search

import (
	"bytes"
	"math"
	"slices"
	"strings"
	"unicode"
	"unicode/utf8"

	"go.dlh.dev/vedi/internal/ansi"
	"go.dlh.dev/vedi/internal/buffer"
)

// Matcher matches a fixed pattern; one without upper-case letters
// ignores case.
type Matcher struct {
	src  string // the pattern as given
	pat  []rune
	fold bool

	// For ruling lines out by their bytes, undecoded. raw is pat in
	// UTF-8, or nil when bytes cannot tell: folding past ASCII, or
	// U+FFFD, which stands for any invalid byte. Every matching line
	// holds a byte of anchor, pat's rarest in both cases. loose is a
	// folding pat with k or i, which K and İ match.
	raw    []byte
	anchor []byte
	loose  bool
}

func New(pattern string) Matcher {
	pat := []rune(pattern)
	fold := !slices.ContainsFunc(pat, unicode.IsUpper)
	if fold {
		for i, r := range pat {
			pat[i] = unicode.ToLower(r)
		}
	}
	m := Matcher{src: pattern, pat: pat, fold: fold}
	if slices.ContainsFunc(pat, func(r rune) bool {
		return r == utf8.RuneError || fold && r >= utf8.RuneSelf
	}) {
		return m
	}
	raw := []byte(string(pat))
	anchor, rarest := byte(0), -1
	for _, c := range raw {
		// K and İ hold neither byte, so they are no anchor.
		if fold && (c == 'k' || c == 'i') {
			m.loose = true
			continue
		}
		if r := rarity(c); r >= rarest {
			anchor, rarest = c, r
		}
	}
	if rarest < 0 {
		return m
	}
	m.raw, m.anchor = raw, []byte{anchor}
	if fold && anchor >= 'a' && anchor <= 'z' {
		m.anchor = append(m.anchor, anchor-'a'+'A')
	}
	return m
}

// common is the usual bytes of text, from the most frequent.
const common = " etaoinsr0123456789hld.-_/cumfpgwybvkxjqz"

// rarity ranks c by how seldom text holds it; a byte not in common is
// rarest.
func rarity(c byte) int {
	if i := strings.IndexByte(common, c); i >= 0 {
		return i
	}
	return len(common)
}

// anchored reports whether b holds a byte of the anchor.
func (m Matcher) anchored(b []byte) bool {
	for _, c := range m.anchor {
		if bytes.IndexByte(b, c) >= 0 {
			return true
		}
	}
	return false
}

// mayMatch reports whether line, undecoded and without its "\n", may
// hold a match: false only when its text cannot. literal is the
// buffer's: the text is then every byte. scratch is reused.
func (m Matcher) mayMatch(line []byte, scratch *[]byte, literal bool) bool {
	if m.raw == nil {
		return true
	}
	if !m.anchored(line) {
		return false
	}
	if literal {
		return m.mayMatchLiteral(line, scratch)
	}
	plain := ansi.Strip(*scratch, line)
	*scratch = plain
	for i, c := range plain {
		switch {
		case c >= utf8.RuneSelf:
			if m.loose {
				return true
			}
		case c < 0x20 && c != '\t' && c != '\r', c == 0x7f:
			// Dropped from the text, so a match may span it.
			return true
		case m.fold && c >= 'A' && c <= 'Z':
			plain[i] = c - 'A' + 'a'
		}
	}
	return bytes.Contains(plain, m.raw)
}

// mayMatchLiteral is mayMatch for a literal buffer's line, whose text
// is every byte: none is dropped for a match to span.
func (m Matcher) mayMatchLiteral(line []byte, scratch *[]byte) bool {
	if !m.fold {
		return bytes.Contains(line, m.raw)
	}
	plain := append((*scratch)[:0], line...)
	*scratch = plain
	for i, c := range plain {
		switch {
		case c >= utf8.RuneSelf:
			if m.loose {
				return true
			}
		case c >= 'A' && c <= 'Z':
			plain[i] = c - 'A' + 'a'
		}
	}
	return bytes.Contains(plain, m.raw)
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

// walk searches lines whole, decoding only those whose bytes may match.
type walk struct {
	buf   *buffer.Buffer
	m     Matcher
	text  []rune
	raw   []byte
	plain []byte

	literal bool // buf's, when the walk began
}

func (w *walk) line(i int) []rune {
	w.text = w.buf.Text(i, w.text)
	return w.text
}

// find is the first match on the first line of [lo, hi) to hold one,
// or with back set the last match on the last.
func (w *walk) find(lo, hi int, back bool) (buffer.Pos, bool) {
	lo = max(lo, 0)
	if lo >= hi {
		return buffer.Pos{}, false
	}
	first, last, step := lo/buffer.BlockLines, (hi-1)/buffer.BlockLines, 1
	if back {
		first, last, step = last, first, -1
	}
	for k := first; k != last+step; k += step {
		if pos, ok := w.block(k, lo, hi, back); ok {
			return pos, true
		}
	}
	return buffer.Pos{}, false
}

// block is find within block k.
func (w *walk) block(k, lo, hi int, back bool) (buffer.Pos, bool) {
	base := k * buffer.BlockLines
	var starts [buffer.BlockLines + 1]int
	n := buffer.BlockLines
	if w.m.raw != nil {
		w.raw = w.buf.Raw(k, w.raw)
		if !w.m.anchored(w.raw) {
			return buffer.Pos{}, false
		}
		n = 0
		for at := 0; at < len(w.raw) && n < buffer.BlockLines; n++ {
			nl := bytes.IndexByte(w.raw[at:], '\n')
			if nl < 0 {
				nl = len(w.raw) - at
			}
			at += nl + 1
			starts[n+1] = at
		}
	}
	j, end, step := 0, n, 1
	if back {
		j, end, step = n-1, -1, -1
	}
	for ; j != end; j += step {
		li := base + j
		if li < lo || li >= hi {
			continue
		}
		if w.m.raw != nil && !w.m.mayMatch(w.raw[starts[j]:min(starts[j+1]-1, len(w.raw))], &w.plain, w.literal) {
			continue
		}
		i := -1
		if back {
			i = w.m.FindLast(w.line(li), math.MaxInt)
		} else {
			i = w.m.Find(w.line(li), 0)
		}
		if i >= 0 {
			return buffer.Pos{Line: li, Col: i}, true
		}
	}
	return buffer.Pos{}, false
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
	w := walk{buf: buf, m: m, literal: buf.Literal()}
	if i := m.Find(w.line(from.Line), col); i >= 0 {
		return buffer.Pos{Line: from.Line, Col: i}, false, true
	}
	if pos, ok := w.find(from.Line+1, n, false); ok {
		return pos, false, true
	}
	if pos, ok := w.find(0, from.Line, false); ok {
		return pos, true, true
	}
	if i := m.Find(w.line(from.Line), 0); i >= 0 {
		return buffer.Pos{Line: from.Line, Col: i}, true, true
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
	w := walk{buf: buf, m: m, literal: buf.Literal()}
	if i := m.FindLast(w.line(from.Line), from.Col); i >= 0 {
		return buffer.Pos{Line: from.Line, Col: i}, false, true
	}
	if pos, ok := w.find(0, from.Line, true); ok {
		return pos, false, true
	}
	if pos, ok := w.find(from.Line+1, n, true); ok {
		return pos, true, true
	}
	if i := m.FindLast(w.line(from.Line), math.MaxInt); i >= 0 {
		return buffer.Pos{Line: from.Line, Col: i}, true, true
	}
	return
}
