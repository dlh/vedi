package ansi

import (
	"testing"

	"github.com/gdamore/tcell/v3"
)

func TestParseText(t *testing.T) {
	tests := []struct {
		name string
		in   string
		want string
	}{
		{"plain", "héllo", "héllo"},
		{"csi skipped", "a\x1b[2Kb", "ab"},
		{"osc bel", "a\x1b]0;title\x07b", "ab"},
		{"osc st", "a\x1b]8;;\x1b\\b", "ab"},
		{"charset", "a\x1b(Bb", "ab"},
		{"dcs st", "a\x1bPq#0;2;0;0;0#0~~@@\x1b\\b", "ab"},
		{"apc st", "a\x1b_Gf=100,a=T;AAAA\x1b\\b", "ab"},
		{"pm st", "a\x1b^x\x1b\\b", "ab"},
		{"sos st", "a\x1bXx\x1b\\b", "ab"},
		{"truncated apc", "abc\x1b_Gf=100", "abc"},
		{"esc single", "a\x1b7b", "ab"},
		{"c0 dropped", "a\x07b\x08c", "abc"},
		{"del dropped", "a\x7fb", "ab"},
		{"tab and cr kept", "a\tb\rc", "a\tb\rc"},
		{"truncated csi", "abc\x1b[3", "abc"},
		{"truncated esc", "abc\x1b", "abc"},
		{"truncated osc", "abc\x1b]8;;foo", "abc"},
		{"truncated charset", "abc\x1b(", "abc"},
		{"invalid utf8", "a\xffb", "a�b"},
		{"empty", "", ""},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			text, runs := NewParser().Parse([]byte(tc.in))
			if got := string(text); got != tc.want {
				t.Fatalf("text = %q, want %q", got, tc.want)
			}
			if len(text) == 0 {
				if len(runs) != 0 {
					t.Fatalf("runs = %v, want none", runs)
				}
				return
			}
			want := []Run{{0, len(text), tcell.StyleDefault, "", ""}}
			if len(runs) != 1 || runs[0] != want[0] {
				t.Fatalf("runs = %v, want %v", runs, want)
			}
		})
	}
}

func TestParseStyles(t *testing.T) {
	d := tcell.StyleDefault
	red := d.Foreground(tcell.PaletteColor(1))
	tests := []struct {
		name string
		in   string
		want []Run
	}{
		{"fg then reset", "\x1b[31mred\x1b[0m plain", []Run{{0, 3, red, "", ""}, {3, 9, d, "", ""}}},
		{"bold underline", "\x1b[1;4mx", []Run{{0, 1, d.Bold(true).Underline(true), "", ""}}},
		{"256 semicolon", "\x1b[38;5;208mx", []Run{{0, 1, d.Foreground(tcell.PaletteColor(208)), "", ""}}},
		{"256 colon", "\x1b[38:5:208mx", []Run{{0, 1, d.Foreground(tcell.PaletteColor(208)), "", ""}}},
		{"rgb semicolon", "\x1b[38;2;1;2;3mx", []Run{{0, 1, d.Foreground(tcell.NewRGBColor(1, 2, 3)), "", ""}}},
		{"rgb colon colorspace", "\x1b[38:2::1:2:3mx", []Run{{0, 1, d.Foreground(tcell.NewRGBColor(1, 2, 3)), "", ""}}},
		{"rgb bg colon", "\x1b[48:2:1:2:3mx", []Run{{0, 1, d.Background(tcell.NewRGBColor(1, 2, 3)), "", ""}}},
		{"bright fg", "\x1b[93mx", []Run{{0, 1, d.Foreground(tcell.PaletteColor(11)), "", ""}}},
		{"bright bg", "\x1b[103mx", []Run{{0, 1, d.Background(tcell.PaletteColor(11)), "", ""}}},
		{"basic bg", "\x1b[44mx", []Run{{0, 1, d.Background(tcell.PaletteColor(4)), "", ""}}},
		{"empty sgr resets", "\x1b[31ma\x1b[mb", []Run{{0, 1, red, "", ""}, {1, 2, d, "", ""}}},
		{"bold off", "\x1b[1mbold\x1b[22mnot", []Run{{0, 4, d.Bold(true), "", ""}, {4, 7, d, "", ""}}},
		{"two sgrs one run", "\x1b[31m\x1b[1mx", []Run{{0, 1, red.Bold(true), "", ""}}},
		{"same style no split", "a\x1b[mb", []Run{{0, 2, d, "", ""}}},
		{"all attrs", "\x1b[2;3;7;9mx", []Run{{0, 1, d.Dim(true).Italic(true).Reverse(true).StrikeThrough(true), "", ""}}},
		{"attrs off", "\x1b[3;7;9m\x1b[23;27;29mx", []Run{{0, 1, d, "", ""}}},
		{"fg default", "\x1b[31m\x1b[39mx", []Run{{0, 1, d, "", ""}}},
		{"bg default", "\x1b[44m\x1b[49mx", []Run{{0, 1, d, "", ""}}},
		{"underline style on", "\x1b[4:3mx", []Run{{0, 1, d.Underline(true), "", ""}}},
		{"underline style off", "\x1b[4m\x1b[4:0mx", []Run{{0, 1, d, "", ""}}},
		{"underline color ignored", "\x1b[58:2:1:2:3mx", []Run{{0, 1, d, "", ""}}},
		{"malformed 256", "\x1b[38;5mx", []Run{{0, 1, d, "", ""}}},
		{"malformed rgb takes the tail", "\x1b[38;2;1;1mx", []Run{{0, 1, d, "", ""}}},
		{"rgb then bold", "\x1b[38;2;1;2;3;1mx", []Run{{0, 1, d.Foreground(tcell.NewRGBColor(1, 2, 3)).Bold(true), "", ""}}},
		{"huge code ignored", "\x1b[31m\x1b[99999999999999999999mx", []Run{{0, 1, red, "", ""}}},
		{"unknown code ignored", "\x1b[99mx", []Run{{0, 1, d, "", ""}}},
		{"kitty line prefix", "\x1b[m\x1b[31mx", []Run{{0, 1, red, "", ""}}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			_, runs := NewParser().Parse([]byte(tc.in))
			if len(runs) != len(tc.want) {
				t.Fatalf("runs = %v, want %v", runs, tc.want)
			}
			for i := range runs {
				if runs[i] != tc.want[i] {
					t.Fatalf("run %d = %v, want %v", i, runs[i], tc.want[i])
				}
			}
		})
	}
}

func TestStyleCarriesAcrossLines(t *testing.T) {
	d := tcell.StyleDefault
	red := d.Foreground(tcell.PaletteColor(1))
	p := NewParser()
	p.Parse([]byte("\x1b[31mline one"))
	_, runs := p.Parse([]byte("two"))
	if len(runs) != 1 || runs[0] != (Run{0, 3, red, "", ""}) {
		t.Fatalf("line two runs = %v, want red", runs)
	}
	_, runs = p.Parse([]byte("\x1b[mthree"))
	if len(runs) != 1 || runs[0] != (Run{0, 5, d, "", ""}) {
		t.Fatalf("line three runs = %v, want default", runs)
	}
}

func TestParseLinks(t *testing.T) {
	d := tcell.StyleDefault
	red := d.Foreground(tcell.PaletteColor(1))
	tests := []struct {
		name string
		in   string
		want []Run
	}{
		{"link st", "\x1b]8;;http://x\x1b\\a\x1b]8;;\x1b\\b", []Run{{0, 1, d, "http://x", ""}, {1, 2, d, "", ""}}},
		{"link bel", "\x1b]8;;http://x\x07a\x1b]8;;\x07b", []Run{{0, 1, d, "http://x", ""}, {1, 2, d, "", ""}}},
		{"link id", "\x1b]8;id=k;http://x\x1b\\a", []Run{{0, 1, d, "http://x", "k"}}},
		{"other params ignored", "\x1b]8;foo=1:id=k;http://x\x1b\\a", []Run{{0, 1, d, "http://x", "k"}}},
		{"sgr inside link", "\x1b]8;;http://x\x1b\\\x1b[31ma\x1b[0mb\x1b]8;;\x1b\\", []Run{{0, 1, red, "http://x", ""}, {1, 2, d, "http://x", ""}}},
		{"new link drops id", "\x1b]8;id=k;http://x\x1b\\a\x1b]8;;http://y\x1b\\b", []Run{{0, 1, d, "http://x", "k"}, {1, 2, d, "http://y", ""}}},
		{"other osc ignored", "\x1b]0;title\x07a", []Run{{0, 1, d, "", ""}}},
		{"malformed osc 8", "\x1b]8;http://x\x07a", []Run{{0, 1, d, "", ""}}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			_, runs := NewParser().Parse([]byte(tc.in))
			if len(runs) != len(tc.want) {
				t.Fatalf("runs = %v, want %v", runs, tc.want)
			}
			for i := range runs {
				if runs[i] != tc.want[i] {
					t.Fatalf("run %d = %v, want %v", i, runs[i], tc.want[i])
				}
			}
		})
	}
}

func TestLinkCarriesAcrossLines(t *testing.T) {
	p := NewParser()
	p.Parse([]byte("\x1b]8;;http://x\x1b\\one"))
	_, runs := p.Parse([]byte("two"))
	if len(runs) != 1 || runs[0] != (Run{0, 3, tcell.StyleDefault, "http://x", ""}) {
		t.Fatalf("line two runs = %v, want link", runs)
	}
}

// TestSkipMatchesParse: Skip leaves the parser where Parse would, line
// after line, escapes cut off at a line end included.
func TestSkipMatchesParse(t *testing.T) {
	lines := []string{
		"plain", "\x1b[31mred", "still red",
		"\x1b]8;;http://x\x1b\\link", "\x1b[1mbold in link", "\x1b[0mreset keeps link",
		"\x1b]8;;\x07unlinked", "\x1b(Bcharset", "cut \x1b[3", "after cut",
		"\x1b[38;2;1;2;3mrgb ü \x1b[4:0mno underline", "\x1b]0;title\x07title", "\x1b]2;\x07cleared", "",
	}
	var byParse, bySkip Parser
	for _, l := range lines {
		byParse.Parse([]byte(l))
		bySkip.Skip([]byte(l))
		if byParse != bySkip {
			t.Fatalf("after %q: Skip left %+v, Parse %+v", l, bySkip, byParse)
		}
	}
}

// TestTitle: the window title OSC 0 or 2 sets is parser state, carried
// across lines like the link until set again; an empty title clears it.
func TestTitle(t *testing.T) {
	tests := []struct {
		name  string
		in    string
		title string
	}{
		{"osc 2 bel", "\x1b]2;vedi\x07text", "vedi"},
		{"osc 2 st", "\x1b]2;vedi\x1b\\text", "vedi"},
		{"osc 0", "\x1b]0;both\x07", "both"},
		{"osc 1 is the icon", "\x1b]1;icon\x07", "was"},
		{"empty clears", "\x1b]2;\x07", ""},
		{"last wins", "\x1b]2;one\x07\x1b[31m\x1b]0;two\x07", "two"},
		{"semicolons kept", "\x1b]2;a;b\x07", "a;b"},
		{"no osc keeps it", "plain \x1b[1mtext", "was"},
		{"link is not a title", "\x1b]8;;http://x\x07", "was"},
		{"cut off", "\x1b]2;ved", "was"},
		{"sgr reset keeps it", "\x1b[0m", "was"},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			p := NewParser()
			p.Parse([]byte("\x1b]2;was\x07"))
			p.Parse([]byte(tc.in))
			if got := p.Title(); got != tc.title {
				t.Fatalf("Title = %q, want %q", got, tc.title)
			}
		})
	}
}

// TestSkipReportsTitle: Skip says whether the line set a title, the
// same one again and an empty one included.
func TestSkipReportsTitle(t *testing.T) {
	tests := []struct {
		in   string
		want bool
	}{
		{"\x1b]2;hunk\x07@@", true},
		{"\x1b[34m\x1b]0;hunk\x1b\\@@\x1b[0m", true},
		{"\x1b]2;\x07", true},
		{"plain \x1b[1mtext", false},
		{"\x1b]8;;http://x\x07link", false},
		{"\x1b]1;icon\x07", false},
		{"\x1b]2;cut", false},
	}
	p := NewParser()
	for _, tc := range tests {
		if got := p.Skip([]byte(tc.in)); got != tc.want {
			t.Errorf("Skip(%q) = %v, want %v", tc.in, got, tc.want)
		}
	}
}

// TestTextMatchesParse: Text gives Parse's runes, into a reused slice,
// and moves the state the same way.
func TestTextMatchesParse(t *testing.T) {
	lines := []string{"a\x1b[31mb", "c\td\x01e", "ü\x1b]8;;u\x07x", "cut \x1b[3", "after"}
	var p, q Parser
	var dst []rune
	for _, l := range lines {
		want, _ := p.Parse([]byte(l))
		dst = q.Text(dst, []byte(l))
		if string(dst) != string(want) || p != q {
			t.Fatalf("%q: Text = %q, Parse = %q", l, string(dst), string(want))
		}
	}
}

// TestParserIsAValue: a copy keeps the state at the copy.
func TestParserIsAValue(t *testing.T) {
	var p Parser
	p.Parse([]byte("\x1b[31m"))
	q := p
	p.Parse([]byte("\x1b[0m"))
	_, runs := q.Parse([]byte("x"))
	red := tcell.StyleDefault.Foreground(tcell.PaletteColor(1))
	if len(runs) != 1 || runs[0].Style != red {
		t.Fatalf("copy's runs = %v, want red", runs)
	}
}

// TestApplySGRAllocatesNothing: styling is per escape on every line,
// so it must not touch the heap.
func TestApplySGRAllocatesNothing(t *testing.T) {
	for _, params := range []string{"", "0", "1;4", "38;5;208", "38;2;1;2;3", "38:2::1:2:3", "4:0", "1;38;5;208;48;2;1;2;3;22"} {
		p := []byte(params)
		if n := testing.AllocsPerRun(100, func() { applySGR(tcell.StyleDefault, p) }); n > 0 {
			t.Errorf("applySGR(%q) allocates %v times", params, n)
		}
	}
}

// TestStripDropsEscapes: Strip keeps every byte outside an escape, in
// dst, and nothing from a cut-off escape on.
func TestStripDropsEscapes(t *testing.T) {
	for _, tc := range []struct{ in, want string }{
		{"plain", "plain"},
		{"", ""},
		{"fo\x1b[31mo\x1b[0m", "foo"},
		{"\x1b]0;title\x07a\x1b]8;;http://x\x1b\\b", "ab"},
		{"a\x1b(Bb\x1b=c", "abc"},
		{"a\x01b\tc\r", "a\x01b\tc\r"},
		{"caf\xc3\xa9 \xff", "caf\xc3\xa9 \xff"},
		{"ab\x1b[3", "ab"},
		{"ab\x1b]0;foo", "ab"},
	} {
		dst := make([]byte, 0, 32)
		got := Strip(dst, []byte(tc.in))
		if string(got) != tc.want {
			t.Errorf("Strip(%q) = %q, want %q", tc.in, got, tc.want)
		}
		if len(got) > 0 && &got[0] != &dst[:1][0] {
			t.Errorf("Strip(%q) is not in dst", tc.in)
		}
	}
}

// TestPrinterKeepsColorsAndLinks: a Printer keeps text, SGR and OSC 8
// and drops what else could act on the terminal: other escapes, one
// cut off by the line's end, controls, and a CSI ending in m that is
// not SGR. A carriage return is spelled out.
func TestPrinterKeepsColorsAndLinks(t *testing.T) {
	for _, tc := range []struct{ in, want string }{
		{"plain", "plain"},
		{"", ""},
		{"a\tb", "a\tb"},
		{"\x1b[31mred\x1b[0m", "\x1b[31mred\x1b[0m"},
		{"\x1b[38:2::1:2:3;4:3mx\x1b[m", "\x1b[38:2::1:2:3;4:3mx\x1b[m"},
		{"\x1b]8;id=a;http://x\x07link\x1b]8;;\x1b\\", "\x1b]8;id=a;http://x\x07link\x1b]8;;\x1b\\"},
		{"\x1b]2;hunk\x07@@ one", "@@ one"},
		{"a\x1b]52;c;aGk=\x07b", "ab"},
		{"a\x1b]1337;File=x\x07b", "ab"},
		{"a\x1b[2J\x1b[H\x1b[3Ab", "ab"},
		{"a\x1b[>4;2mb", "ab"},
		{"a\x1b[?1049hb", "ab"},
		{"a\x1bP+q\x1b\\b\x1bcc", "abc"},
		{"ab\x1b]52;cut", "ab"},
		{"ab\x1b[31", "ab"},
		{"real\rfake\x08\x07\x7f\x0e", "real^Mfake"},
		{"a\u009b2Jb\u009d52;c\u009c", "a2Jb52;c"},
		{"a\x9b2Jb", "a\ufffd2Jb"},
		{"\x1b]8;;http://x\u009c\x07a", "a"},
		{"\x1b]8;;http://\xffx\x07a", "a"},
	} {
		var p Printer
		dst := make([]byte, 0, 64)
		got := p.Line(dst, []byte(tc.in))
		if string(got) != tc.want {
			t.Errorf("Line(%q) = %q, want %q", tc.in, got, tc.want)
		}
		if len(got) > 0 && &got[0] != &dst[:1][0] {
			t.Errorf("Line(%q) is not in dst", tc.in)
		}
	}
}

// TestPrinterEnd: End resets a style and closes a link the text left
// on, across lines, and adds nothing when it left neither.
func TestPrinterEnd(t *testing.T) {
	for _, tc := range []struct {
		lines []string
		want  string
	}{
		{[]string{"plain"}, ""},
		{[]string{"\x1b[31mred\x1b[0m", "\x1b[1mb\x1b[m"}, ""},
		{[]string{"\x1b[31mred", "more"}, "\x1b[m"},
		{[]string{"\x1b[0;1mbold"}, "\x1b[m"},
		{[]string{"\x1b]8;;http://x\x07link"}, "\x1b]8;;\x1b\\"},
		{[]string{"\x1b]8;;http://x\x07link\x1b]8;;\x07"}, ""},
		{[]string{"\x1b[4m\x1b]8;;http://x\x07link"}, "\x1b[m\x1b]8;;\x1b\\"},
	} {
		var p Printer
		for _, line := range tc.lines {
			p.Line(nil, []byte(line))
		}
		if got := p.End(nil); string(got) != tc.want {
			t.Errorf("End after %q = %q, want %q", tc.lines, got, tc.want)
		}
	}
}

// TestPrinterPlain: a plain Printer drops SGR and keeps links, and
// has no style to end.
func TestPrinterPlain(t *testing.T) {
	p := Printer{View: ViewPlain}
	in := "\x1b[1;31mred\x1b[0m \x1b]8;;http://x\x07link"
	if got, want := string(p.Line(nil, []byte(in))), "red \x1b]8;;http://x\x07link"; got != want {
		t.Errorf("Line(%q) = %q, want %q", in, got, want)
	}
	if got, want := string(p.End(nil)), "\x1b]8;;\x1b\\"; got != want {
		t.Errorf("End = %q, want %q", got, want)
	}
}
