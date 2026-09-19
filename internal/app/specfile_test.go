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
// optional Shift+/Ctrl+ prefixes, single characters as themselves,
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
	for strings.HasPrefix(tok, "Shift+") || strings.HasPrefix(tok, "Ctrl+") {
		if strings.HasPrefix(tok, "Shift+") {
			mod |= tcell.ModShift
			tok = tok[len("Shift+"):]
		} else {
			mod |= tcell.ModCtrl
			tok = tok[len("Ctrl+"):]
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
		if mod != 0 {
			return nil, fmt.Errorf("%q: modifiers on a character key", tok)
		}
		return tcell.NewEventKey(tcell.KeyRune, r, 0), nil
	}
	return nil, fmt.Errorf("unknown key %q", tok)
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
	keys, err := parseKeys(`Down Shift+Right Ctrl+Shift+Left Ctrl+C y "a b" Esc Backspace Space`)
	if err != nil {
		t.Fatal(err)
	}
	want := []*tcell.EventKey{
		tcell.NewEventKey(tcell.KeyDown, 0, 0),
		tcell.NewEventKey(tcell.KeyRight, 0, tcell.ModShift),
		tcell.NewEventKey(tcell.KeyLeft, 0, tcell.ModCtrl|tcell.ModShift),
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
