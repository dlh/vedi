package main

import (
	"strings"
	"unicode/utf8"
)

// screen is what a terminal would show: rows of cells, a cursor, and
// the escape-sequence parser that moves them. A pager that draws with
// tcell sends only the cells that changed, so a wait must look at the
// screen, not the byte stream. State carries across writes: a
// sequence, or a rune, may straddle reads.
type screen struct {
	rows, cols int
	cell       [][]rune
	r, c       int // the cursor
	top, bot   int // the scroll region, inclusive
	sr, sc     int // the saved cursor
	state      int
	seq        []byte // the CSI parameters and intermediates so far
	pend       []byte // an incomplete rune
	da1        bool   // the pager asked for the terminal's identity
}

const (
	sText = iota
	sEsc
	sEsc2
	sCSI
	sOSC
	sOSCEsc
)

func newScreen(rows, cols int) *screen {
	s := &screen{rows: rows, cols: cols, bot: rows - 1}
	s.cell = make([][]rune, rows)
	for i := range s.cell {
		s.cell[i] = s.blank()
	}
	return s
}

func (s *screen) blank() []rune {
	row := make([]rune, s.cols)
	for i := range row {
		row[i] = ' '
	}
	return row
}

// contains reports whether a row shows text.
func (s *screen) contains(text string) bool {
	for _, row := range s.cell {
		if strings.Contains(string(row), text) {
			return true
		}
	}
	return false
}

// String is the screen, a line per row, for a failure message.
func (s *screen) String() string {
	var b strings.Builder
	for _, row := range s.cell {
		b.WriteString(strings.TrimRight(string(row), " "))
		b.WriteByte('\n')
	}
	return b.String()
}

// askedDA1 reports, once, that the pager sent a primary device
// attributes request and is waiting for the reply.
func (s *screen) askedDA1() bool {
	asked := s.da1
	s.da1 = false
	return asked
}

// write takes p as a terminal would.
func (s *screen) write(p []byte) {
	for _, b := range p {
		switch s.state {
		case sText:
			s.text(b)
		case sEsc:
			s.esc(b)
		case sEsc2:
			s.state = sText
		case sCSI:
			if b >= 0x40 && b <= 0x7e {
				s.csi(b)
				s.state = sText
			} else {
				s.seq = append(s.seq, b)
			}
		case sOSC:
			if b == 7 {
				s.state = sText
			} else if b == 0x1b {
				s.state = sOSCEsc
			}
		case sOSCEsc:
			s.state = sText
		}
	}
}

func (s *screen) text(b byte) {
	if len(s.pend) > 0 || b >= 0x80 {
		s.pend = append(s.pend, b)
		if !utf8.FullRune(s.pend) {
			return
		}
		r, _ := utf8.DecodeRune(s.pend)
		s.pend = s.pend[:0]
		s.put(r)
		return
	}
	switch b {
	case 0x1b:
		s.state = sEsc
	case '\r':
		s.c = 0
	case '\n', 0x0b, 0x0c:
		s.lineFeed()
	case '\b':
		if s.c > 0 {
			s.c--
		}
	case '\t':
		s.c = min((s.c/8+1)*8, s.cols-1)
	default:
		if b >= 0x20 {
			s.put(rune(b))
		}
	}
}

func (s *screen) put(r rune) {
	if s.c >= s.cols {
		s.c = 0
		s.lineFeed()
	}
	s.cell[s.r][s.c] = r
	s.c++
}

func (s *screen) lineFeed() {
	if s.r == s.bot {
		s.scrollUp(s.top, s.bot, 1)
	} else if s.r < s.rows-1 {
		s.r++
	}
}

// scrollUp moves rows top+n..bot up by n within the region, blanking
// the rows at the bottom; n < 0 scrolls down.
func (s *screen) scrollUp(top, bot, n int) {
	if n > 0 {
		for i := top; i <= bot; i++ {
			if i+n <= bot {
				s.cell[i] = s.cell[i+n]
			} else {
				s.cell[i] = s.blank()
			}
		}
	} else if n < 0 {
		for i := bot; i >= top; i-- {
			if i+n >= top {
				s.cell[i] = s.cell[i+n]
			} else {
				s.cell[i] = s.blank()
			}
		}
	}
}

func (s *screen) esc(b byte) {
	s.state = sText
	switch b {
	case '[':
		s.state = sCSI
		s.seq = s.seq[:0]
	case ']', 'P', '_', '^', 'X':
		s.state = sOSC
	case '(', ')', '*', '+', '#', '%':
		s.state = sEsc2
	case '7':
		s.sr, s.sc = s.r, s.c
	case '8':
		s.r, s.c = s.sr, s.sc
	case 'D':
		s.lineFeed()
	case 'E':
		s.c = 0
		s.lineFeed()
	case 'M':
		if s.r == s.top {
			s.scrollUp(s.top, s.bot, -1)
		} else if s.r > 0 {
			s.r--
		}
	case 'c':
		*s = *newScreen(s.rows, s.cols)
	}
}

// params are the CSI's numeric parameters, def where one is absent.
func (s *screen) params(def int) []int {
	var out []int
	n, have := 0, false
	for _, b := range s.seq {
		switch {
		case b >= '0' && b <= '9':
			n, have = n*10+int(b-'0'), true
		case b == ';':
			out = append(out, pick(n, have, def))
			n, have = 0, false
		}
	}
	return append(out, pick(n, have, def))
}

func pick(n int, have bool, def int) int {
	if !have || n == 0 {
		return def
	}
	return n
}

func (s *screen) csi(final byte) {
	private := len(s.seq) > 0 && s.seq[0] >= '<' && s.seq[0] <= '?'
	p := s.params(1)
	n := p[0]
	switch final {
	case 'H', 'f':
		col := 1
		if len(p) > 1 {
			col = p[1]
		}
		s.r, s.c = clamp(n-1, s.rows), clamp(col-1, s.cols)
	case 'A':
		s.r = max(s.r-n, 0)
	case 'B':
		s.r = min(s.r+n, s.rows-1)
	case 'C':
		s.c = min(s.c+n, s.cols-1)
	case 'D':
		s.c = max(s.c-n, 0)
	case 'E':
		s.r, s.c = min(s.r+n, s.rows-1), 0
	case 'F':
		s.r, s.c = max(s.r-n, 0), 0
	case 'G', '`':
		s.c = clamp(n-1, s.cols)
	case 'd':
		s.r = clamp(n-1, s.rows)
	case 'J':
		s.eraseDisplay(s.params(0)[0])
	case 'K':
		s.eraseLine(s.params(0)[0])
	case 'L':
		if s.r >= s.top && s.r <= s.bot {
			s.scrollUp(s.r, s.bot, -n)
		}
	case 'M':
		if s.r >= s.top && s.r <= s.bot {
			s.scrollUp(s.r, s.bot, n)
		}
	case 'S':
		s.scrollUp(s.top, s.bot, n)
	case 'T':
		s.scrollUp(s.top, s.bot, -n)
	case 'P':
		row := s.cell[s.r]
		copy(row[s.c:], row[min(s.c+n, s.cols):])
		s.fill(s.r, max(s.cols-n, s.c), s.cols)
	case '@':
		row := s.cell[s.r]
		copy(row[min(s.c+n, s.cols):], row[s.c:])
		s.fill(s.r, s.c, min(s.c+n, s.cols))
	case 'X':
		s.fill(s.r, s.c, min(s.c+n, s.cols))
	case 'r':
		s.top, s.bot = 0, s.rows-1
		if !private {
			if len(p) > 1 {
				s.top, s.bot = clamp(p[0]-1, s.rows), clamp(p[1]-1, s.rows)
			}
			if s.top >= s.bot {
				s.top, s.bot = 0, s.rows-1
			}
		}
		s.r, s.c = 0, 0
	case 's':
		s.sr, s.sc = s.r, s.c
	case 'u':
		s.r, s.c = s.sr, s.sc
	case 'c':
		if !private {
			s.da1 = true
		}
	}
}

func clamp(i, n int) int { return max(0, min(i, n-1)) }

func (s *screen) fill(r, from, to int) {
	for i := from; i < to; i++ {
		s.cell[r][i] = ' '
	}
}

func (s *screen) eraseLine(mode int) {
	switch mode {
	case 0:
		s.fill(s.r, s.c, s.cols)
	case 1:
		s.fill(s.r, 0, min(s.c+1, s.cols))
	case 2:
		s.fill(s.r, 0, s.cols)
	}
}

func (s *screen) eraseDisplay(mode int) {
	switch mode {
	case 0:
		s.eraseLine(0)
		for i := s.r + 1; i < s.rows; i++ {
			s.cell[i] = s.blank()
		}
	case 1:
		s.eraseLine(1)
		for i := 0; i < s.r; i++ {
			s.cell[i] = s.blank()
		}
	default:
		for i := range s.cell {
			s.cell[i] = s.blank()
		}
	}
}
