package main

// stripper drops escape sequences from a byte stream: CSI (ESC [ ...
// final byte), OSC (ESC ] ... BEL or ESC \), three-byte ESC ( x and
// two-byte ESC x. State carries across calls: a sequence may straddle
// reads.
type stripper struct{ state int }

const (
	text = iota
	esc
	esc2
	csi
	osc
	oscEsc
)

// strip appends p's text to dst.
func (s *stripper) strip(dst, p []byte) []byte {
	for _, c := range p {
		switch s.state {
		case text:
			if c == 0x1b {
				s.state = esc
			} else {
				dst = append(dst, c)
			}
		case esc:
			switch c {
			case '[':
				s.state = csi
			case ']':
				s.state = osc
			case '(', ')', '#':
				s.state = esc2
			default:
				s.state = text
			}
		case esc2:
			s.state = text
		case csi:
			if c >= 0x40 && c <= 0x7e {
				s.state = text
			}
		case osc:
			if c == 7 {
				s.state = text
			} else if c == 0x1b {
				s.state = oscEsc
			}
		case oscEsc:
			s.state = text
		}
	}
	return dst
}
