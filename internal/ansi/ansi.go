// Package ansi turns raw terminal output into text plus style runs.
package ansi

import (
	"unicode/utf8"

	"github.com/gdamore/tcell/v2"
)

// Run is a maximal range of runes [Start, End) drawn in one style.
type Run struct {
	Start, End int
	Style      tcell.Style
}

// Parser converts bytes to text and style runs. The style carries
// across calls: a color set on one line stays until a reset.
type Parser struct {
	style tcell.Style
}

func NewParser() *Parser { return &Parser{style: tcell.StyleDefault} }

// Parse takes one line without its ending and returns the visible runes
// and runs covering them. SGR changes the style; every other escape is
// skipped whole. C0 controls other than \t and \r are dropped, as is a
// sequence cut off by the end of line.
func (p *Parser) Parse(line []byte) ([]rune, []Run) {
	var text []rune
	var runs []Run
	runStart := 0
	closeRun := func() {
		if len(text) > runStart {
			runs = append(runs, Run{runStart, len(text), p.style})
		}
		runStart = len(text)
	}
	for i := 0; i < len(line); {
		c := line[i]
		switch {
		case c == 0x1b:
			n, sgr, ok := escape(line[i:])
			if !ok {
				i = len(line)
				continue
			}
			if sgr != nil {
				if next := applySGR(p.style, sgr); next != p.style {
					closeRun()
					p.style = next
				}
			}
			i += n
		case c == '\t' || c == '\r':
			text = append(text, rune(c))
			i++
		case c < 0x20 || c == 0x7f:
			i++
		default:
			r, size := utf8.DecodeRune(line[i:])
			text = append(text, r)
			i += size
		}
	}
	closeRun()
	return text, runs
}

// escape scans the sequence at b[0] == ESC: its length, its parameters
// when it is SGR (non-nil, possibly empty), and ok=false when b ends
// first.
func escape(b []byte) (n int, sgr []byte, ok bool) {
	if len(b) < 2 {
		return len(b), nil, false
	}
	switch b[1] {
	case '[': // CSI: parameters and intermediates, then a final byte 0x40-0x7E
		for i := 2; i < len(b); i++ {
			if b[i] >= 0x40 && b[i] <= 0x7e {
				if b[i] == 'm' {
					return i + 1, b[2:i], true
				}
				return i + 1, nil, true
			}
		}
		return len(b), nil, false
	case ']': // OSC: ends at BEL or ESC \
		for i := 2; i < len(b); i++ {
			if b[i] == 0x07 {
				return i + 1, nil, true
			}
			if b[i] == 0x1b && i+1 < len(b) && b[i+1] == '\\' {
				return i + 2, nil, true
			}
		}
		return len(b), nil, false
	case '(', ')', '*', '+': // charset select: one more byte
		if len(b) < 3 {
			return len(b), nil, false
		}
		return 3, nil, true
	default: // ESC plus one byte: ESC 7, ESC =, ...
		return 2, nil, true
	}
}

// applySGR is completed in Task 2. Until then every SGR is a no-op.
func applySGR(s tcell.Style, params []byte) tcell.Style { return s }
