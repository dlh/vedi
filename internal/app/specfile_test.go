package app_test

import (
	"fmt"
	"reflect"
	"strings"
	"testing"
	"unicode/utf8"

	"github.com/gdamore/tcell/v2"
)

// A section of a scenario file: "-- name --" then its body, ending in
// a newline unless empty. line is the header's 1-based line.
type section struct {
	name, body string
	line       int
}

type archive struct {
	comment  string
	sections []section
}

// parseArchive splits txtar-style text: everything before the first
// "-- name --" line is the comment.
func parseArchive(data string) archive {
	if data != "" && !strings.HasSuffix(data, "\n") {
		data += "\n"
	}
	var a archive
	var cur *section
	var body strings.Builder
	flush := func() {
		if cur != nil {
			cur.body = body.String()
			a.sections = append(a.sections, *cur)
		} else {
			a.comment = body.String()
		}
		body.Reset()
	}
	for i, line := range strings.SplitAfter(data, "\n") {
		if line == "" {
			continue
		}
		trimmed := strings.TrimSuffix(line, "\n")
		if strings.HasPrefix(trimmed, "-- ") && strings.HasSuffix(trimmed, " --") {
			flush()
			cur = &section{name: strings.TrimSuffix(strings.TrimPrefix(trimmed, "-- "), " --"), line: i + 1}
			continue
		}
		body.WriteString(line)
	}
	flush()
	return a
}

var namedKeys = map[string]tcell.Key{
	"Up": tcell.KeyUp, "Down": tcell.KeyDown, "Left": tcell.KeyLeft, "Right": tcell.KeyRight,
	"Home": tcell.KeyHome, "End": tcell.KeyEnd, "PgUp": tcell.KeyPgUp, "PgDn": tcell.KeyPgDn,
	"Enter": tcell.KeyEnter, "Esc": tcell.KeyEscape, "Backspace": tcell.KeyBackspace2,
}

// parseKeys turns a "-- keys --" body into key events: named keys with
// optional Shift+/Ctrl+/Alt+ prefixes, single characters as themselves,
// "quoted text" typed rune by rune.
func parseKeys(s string) ([]*tcell.EventKey, error) {
	var keys []*tcell.EventKey
	for s = strings.TrimSpace(s); s != ""; s = strings.TrimSpace(s) {
		if s[0] == '"' {
			end := strings.IndexByte(s[1:], '"')
			if end < 0 {
				return nil, fmt.Errorf("unterminated quote: %s", s)
			}
			for _, r := range s[1 : 1+end] {
				keys = append(keys, tcell.NewEventKey(tcell.KeyRune, r, 0))
			}
			s = s[2+end:]
			continue
		}
		tok := s
		if i := strings.IndexAny(s, " \t\n"); i >= 0 {
			tok, s = s[:i], s[i:]
		} else {
			s = ""
		}
		ev, err := parseKey(tok)
		if err != nil {
			return nil, err
		}
		keys = append(keys, ev)
	}
	return keys, nil
}

func parseKey(tok string) (*tcell.EventKey, error) {
	var mod tcell.ModMask
	prefixes := []struct {
		name string
		mod  tcell.ModMask
	}{{"Shift+", tcell.ModShift}, {"Ctrl+", tcell.ModCtrl}, {"Alt+", tcell.ModAlt}}
	for again := true; again; {
		again = false
		for _, p := range prefixes {
			if strings.HasPrefix(tok, p.name) {
				mod |= p.mod
				tok = tok[len(p.name):]
				again = true
			}
		}
	}
	if tok == "Space" {
		return tcell.NewEventKey(tcell.KeyRune, ' ', mod), nil
	}
	if k, ok := namedKeys[tok]; ok {
		return tcell.NewEventKey(k, 0, mod), nil
	}
	if r, size := utf8.DecodeRuneInString(tok); size == len(tok) && size > 0 {
		if mod&tcell.ModCtrl != 0 && r >= 'a' && r <= 'z' {
			return tcell.NewEventKey(tcell.KeyCtrlA+tcell.Key(r-'a'), 0, mod), nil
		}
		if mod&tcell.ModCtrl != 0 && r >= 'A' && r <= 'Z' {
			return tcell.NewEventKey(tcell.KeyCtrlA+tcell.Key(r-'A'), 0, mod), nil
		}
		if mod&^tcell.ModAlt != 0 {
			return nil, fmt.Errorf("%q: modifiers on a character key", tok)
		}
		return tcell.NewEventKey(tcell.KeyRune, r, mod), nil
	}
	return nil, fmt.Errorf("unknown key %q", tok)
}

// A mouse action from a "-- mouse --" body. For click and dblclick
// only the first cell is used; drag presses at the first and releases
// at the second; wheel uses n and up.
type mouseAction struct {
	kind   string
	y, x   int // row, col of the first cell
	y2, x2 int
	up     bool
	n      int
}

// parseMouse turns a "-- mouse --" body into actions: "click R C",
// "dblclick R C", "drag R C R C" and "wheel up|down [N]", rows and
// columns 0-based.
func parseMouse(s string) ([]mouseAction, error) {
	f := strings.Fields(s)
	var acts []mouseAction
	ints := func(i, n int) ([]int, error) {
		if i+n > len(f) {
			return nil, fmt.Errorf("%s: want %d numbers", f[i-1], n)
		}
		out := make([]int, n)
		for k := range out {
			if _, err := fmt.Sscanf(f[i+k], "%d", &out[k]); err != nil {
				return nil, fmt.Errorf("%s: %q is not a number", f[i-1], f[i+k])
			}
		}
		return out, nil
	}
	for i := 0; i < len(f); {
		a := mouseAction{kind: f[i]}
		i++
		switch a.kind {
		case "click", "dblclick":
			v, err := ints(i, 2)
			if err != nil {
				return nil, err
			}
			a.y, a.x = v[0], v[1]
			i += 2
		case "drag":
			v, err := ints(i, 4)
			if err != nil {
				return nil, err
			}
			a.y, a.x, a.y2, a.x2 = v[0], v[1], v[2], v[3]
			i += 4
		case "wheel":
			if i >= len(f) || f[i] != "up" && f[i] != "down" {
				return nil, fmt.Errorf("wheel: want up or down")
			}
			a.up = f[i] == "up"
			i++
			a.n = 1
			if i < len(f) {
				if v, err := ints(i, 1); err == nil {
					a.n = v[0]
					i++
				}
			}
		default:
			return nil, fmt.Errorf("unknown mouse action %q", a.kind)
		}
		acts = append(acts, a)
	}
	return acts, nil
}

func TestParseArchive(t *testing.T) {
	a := parseArchive("prose\nmore\n-- size --\n10x4\n-- input --\nline 1\n\n-- empty --\n-- last --\nx\n")
	if a.comment != "prose\nmore\n" {
		t.Errorf("comment = %q", a.comment)
	}
	want := []section{
		{"size", "10x4\n", 3},
		{"input", "line 1\n\n", 5},
		{"empty", "", 8},
		{"last", "x\n", 9},
	}
	if !reflect.DeepEqual(a.sections, want) {
		t.Errorf("sections = %+v\nwant       %+v", a.sections, want)
	}
	a = parseArchive("prose\n-- last --\nx")
	if got := a.sections[len(a.sections)-1].body; got != "x\n" {
		t.Errorf("without a final newline, last body = %q, want %q", got, "x\n")
	}
}

func TestParseKeys(t *testing.T) {
	keys, err := parseKeys(`Down Shift+Right Ctrl+Shift+Left Alt+Shift+Right Alt+b Ctrl+C y "a b" Esc Backspace Space`)
	if err != nil {
		t.Fatal(err)
	}
	want := []*tcell.EventKey{
		tcell.NewEventKey(tcell.KeyDown, 0, 0),
		tcell.NewEventKey(tcell.KeyRight, 0, tcell.ModShift),
		tcell.NewEventKey(tcell.KeyLeft, 0, tcell.ModCtrl|tcell.ModShift),
		tcell.NewEventKey(tcell.KeyRight, 0, tcell.ModAlt|tcell.ModShift),
		tcell.NewEventKey(tcell.KeyRune, 'b', tcell.ModAlt),
		tcell.NewEventKey(tcell.KeyCtrlC, 0, tcell.ModCtrl),
		tcell.NewEventKey(tcell.KeyRune, 'y', 0),
		tcell.NewEventKey(tcell.KeyRune, 'a', 0),
		tcell.NewEventKey(tcell.KeyRune, ' ', 0),
		tcell.NewEventKey(tcell.KeyRune, 'b', 0),
		tcell.NewEventKey(tcell.KeyEscape, 0, 0),
		tcell.NewEventKey(tcell.KeyBackspace2, 0, 0),
		tcell.NewEventKey(tcell.KeyRune, ' ', 0),
	}
	if len(keys) != len(want) {
		t.Fatalf("got %d keys, want %d", len(keys), len(want))
	}
	for i := range want {
		if keys[i].Key() != want[i].Key() || keys[i].Rune() != want[i].Rune() || keys[i].Modifiers() != want[i].Modifiers() {
			t.Errorf("key %d = (%v %q %v), want (%v %q %v)", i, keys[i].Key(), keys[i].Rune(), keys[i].Modifiers(), want[i].Key(), want[i].Rune(), want[i].Modifiers())
		}
	}
	for _, bad := range []string{"Bogus", "Shift+y", `"unterminated`} {
		if _, err := parseKeys(bad); err == nil {
			t.Errorf("parseKeys(%q) should fail", bad)
		}
	}
}

func TestParseMouse(t *testing.T) {
	acts, err := parseMouse("click 1 2 dblclick 3 4 drag 0 1 2 3 wheel up wheel down 5 click 0 0")
	if err != nil {
		t.Fatal(err)
	}
	want := []mouseAction{
		{kind: "click", y: 1, x: 2},
		{kind: "dblclick", y: 3, x: 4},
		{kind: "drag", y: 0, x: 1, y2: 2, x2: 3},
		{kind: "wheel", up: true, n: 1},
		{kind: "wheel", up: false, n: 5},
		{kind: "click", y: 0, x: 0},
	}
	if !reflect.DeepEqual(acts, want) {
		t.Errorf("actions = %+v\nwant      %+v", acts, want)
	}
	for _, bad := range []string{"click 1", "click a b", "drag 1 2 3", "wheel sideways", "tap 1 1"} {
		if _, err := parseMouse(bad); err == nil {
			t.Errorf("parseMouse(%q) should fail", bad)
		}
	}
}
