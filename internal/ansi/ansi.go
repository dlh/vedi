// Package ansi turns raw terminal output into text plus style runs.
package ansi

import (
	"bytes"
	"strconv"
	"strings"
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
	// A rune is at least one byte; a run starts at a line's start or an
	// escape.
	text := make([]rune, 0, len(line))
	runs := make([]Run, 0, bytes.Count(line, []byte{0x1b})+1)
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
	return trim(text), trim(runs)
}

// trim copies s to a slice of its own length when most of its capacity
// is unused, so a line of mostly escapes does not keep it.
func trim[T any](s []T) []T {
	if cap(s) > 2*len(s)+8 {
		return append(make([]T, 0, len(s)), s...)
	}
	return s
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
	case ']', 'P', '_', '^', 'X': // OSC, DCS, APC, PM, SOS: a string ending at ESC \, or BEL for OSC
		for i := 2; i < len(b); i++ {
			if b[i] == 0x07 && b[1] == ']' {
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

// applySGR applies SGR parameters to s. params is the text between
// "ESC [" and "m": "1;38;5;208", "38:2::255:0:0" (T.416 colon form), or "" (reset).
func applySGR(s tcell.Style, params []byte) tcell.Style {
	if len(params) == 0 {
		return tcell.StyleDefault
	}
	groups := strings.Split(string(params), ";")
	for gi := 0; gi < len(groups); gi++ {
		sub := strings.Split(groups[gi], ":")
		n := atoi(sub[0])
		switch {
		case n == 0:
			s = tcell.StyleDefault
		case n == 1:
			s = s.Bold(true)
		case n == 2:
			s = s.Dim(true)
		case n == 3:
			s = s.Italic(true)
		case n == 4:
			s = s.Underline(len(sub) < 2 || sub[1] != "0")
		case n == 7:
			s = s.Reverse(true)
		case n == 9:
			s = s.StrikeThrough(true)
		case n == 22:
			s = s.Bold(false).Dim(false)
		case n == 23:
			s = s.Italic(false)
		case n == 24:
			s = s.Underline(false)
		case n == 27:
			s = s.Reverse(false)
		case n == 29:
			s = s.StrikeThrough(false)
		case n >= 30 && n <= 37:
			s = s.Foreground(tcell.PaletteColor(n - 30))
		case n == 39:
			s = s.Foreground(tcell.ColorDefault)
		case n >= 40 && n <= 47:
			s = s.Background(tcell.PaletteColor(n - 40))
		case n == 49:
			s = s.Background(tcell.ColorDefault)
		case n >= 90 && n <= 97:
			s = s.Foreground(tcell.PaletteColor(n - 90 + 8))
		case n >= 100 && n <= 107:
			s = s.Background(tcell.PaletteColor(n - 100 + 8))
		case n == 38 || n == 48 || n == 58:
			var args []string
			if len(sub) > 1 {
				args = sub[1:]
			} else {
				args, gi = takeColorArgs(groups, gi)
			}
			c, ok := extendedColor(args)
			if !ok {
				continue
			}
			switch n {
			case 38:
				s = s.Foreground(c)
			case 48:
				s = s.Background(c)
			}
		}
	}
	return s
}

// takeColorArgs reads the semicolon-form arguments of the 38/48/58 at
// groups[gi] ("5;n" or "2;r;g;b") and the index of the last group
// taken. A malformed tail takes the rest.
func takeColorArgs(groups []string, gi int) ([]string, int) {
	if gi+1 < len(groups) {
		switch groups[gi+1] {
		case "5":
			if gi+2 < len(groups) {
				return groups[gi+1 : gi+3], gi + 2
			}
		case "2":
			if gi+4 < len(groups) {
				return groups[gi+1 : gi+5], gi + 4
			}
		}
	}
	return nil, len(groups) - 1
}

// extendedColor decodes "5 n" or "2 [colorspace] r g b" arguments.
func extendedColor(args []string) (tcell.Color, bool) {
	if len(args) == 0 {
		return 0, false
	}
	switch args[0] {
	case "5":
		if len(args) >= 2 {
			return tcell.PaletteColor(clamp(atoi(args[1]))), true
		}
	case "2":
		if len(args) >= 4 {
			rgb := args[len(args)-3:]
			return tcell.NewRGBColor(int32(clamp(atoi(rgb[0]))), int32(clamp(atoi(rgb[1]))), int32(clamp(atoi(rgb[2])))), true
		}
	}
	return 0, false
}

func atoi(s string) int {
	n, _ := strconv.Atoi(s)
	return n
}

func clamp(n int) int {
	if n < 0 {
		return 0
	}
	if n > 255 {
		return 255
	}
	return n
}
