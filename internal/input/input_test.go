package input

import (
	"os"
	"strings"
	"testing"

	"github.com/gdamore/tcell/v2"
)

func TestDecode(t *testing.T) {
	k := func(key tcell.Key, r rune, mod tcell.ModMask) *tcell.EventKey {
		return tcell.NewEventKey(key, r, mod)
	}
	tests := []struct {
		name string
		ev   *tcell.EventKey
		want Command
	}{
		{"up", k(tcell.KeyUp, 0, 0), Command{Up, false}},
		{"shift up", k(tcell.KeyUp, 0, tcell.ModShift), Command{Up, true}},
		{"shift down", k(tcell.KeyDown, 0, tcell.ModShift), Command{Down, true}},
		{"left", k(tcell.KeyLeft, 0, 0), Command{Left, false}},
		{"ctrl left", k(tcell.KeyLeft, 0, tcell.ModCtrl), Command{WordLeft, false}},
		{"ctrl shift right", k(tcell.KeyRight, 0, tcell.ModCtrl|tcell.ModShift), Command{WordRight, true}},
		{"alt left", k(tcell.KeyLeft, 0, tcell.ModAlt), Command{WordLeft, false}},
		{"alt shift right", k(tcell.KeyRight, 0, tcell.ModAlt|tcell.ModShift), Command{WordRight, true}},
		{"alt b", k(tcell.KeyRune, 'b', tcell.ModAlt), Command{WordLeft, false}},
		{"alt f", k(tcell.KeyRune, 'f', tcell.ModAlt), Command{WordRight, false}},
		{"shift home", k(tcell.KeyHome, 0, tcell.ModShift), Command{Home, true}},
		{"end", k(tcell.KeyEnd, 0, 0), Command{End, false}},
		{"shift pgup", k(tcell.KeyPgUp, 0, tcell.ModShift), Command{PageUp, true}},
		{"pgdn", k(tcell.KeyPgDn, 0, 0), Command{PageDown, false}},
		{"space", k(tcell.KeyRune, ' ', 0), Command{PageDown, false}},
		{"b", k(tcell.KeyRune, 'b', 0), Command{PageUp, false}},
		{"j", k(tcell.KeyRune, 'j', 0), Command{Down, false}},
		{"k", k(tcell.KeyRune, 'k', 0), Command{Up, false}},
		{"f", k(tcell.KeyRune, 'f', 0), Command{PageDown, false}},
		{"ctrl f", k(tcell.KeyCtrlF, 0, tcell.ModCtrl), Command{PageDown, false}},
		{"ctrl b", k(tcell.KeyCtrlB, 0, tcell.ModCtrl), Command{PageUp, false}},
		{"ctrl v", k(tcell.KeyCtrlV, 0, tcell.ModCtrl), Command{PageDown, false}},
		{"alt v", k(tcell.KeyRune, 'v', tcell.ModAlt), Command{PageUp, false}},
		{"d", k(tcell.KeyRune, 'd', 0), Command{HalfPageDown, false}},
		{"ctrl d", k(tcell.KeyCtrlD, 0, tcell.ModCtrl), Command{HalfPageDown, false}},
		{"u", k(tcell.KeyRune, 'u', 0), Command{HalfPageUp, false}},
		{"ctrl u", k(tcell.KeyCtrlU, 0, tcell.ModCtrl), Command{HalfPageUp, false}},
		{"g", k(tcell.KeyRune, 'g', 0), Command{First, false}},
		{"<", k(tcell.KeyRune, '<', tcell.ModShift), Command{First, false}},
		{">", k(tcell.KeyRune, '>', tcell.ModShift), Command{Last, false}},
		{"G never extends", k(tcell.KeyRune, 'G', tcell.ModShift), Command{Last, false}},
		{"ctrl a", k(tcell.KeyCtrlA, 0, tcell.ModCtrl), Command{SelectAll, false}},
		{"esc", k(tcell.KeyEscape, 0, 0), Command{ClearSelection, false}},
		{"ctrl c", k(tcell.KeyCtrlC, 0, tcell.ModCtrl), Command{Copy, false}},
		{"y", k(tcell.KeyRune, 'y', 0), Command{Copy, false}},
		{"enter", k(tcell.KeyEnter, 0, 0), Command{Enter, false}},
		{"slash", k(tcell.KeyRune, '/', 0), Command{Search, false}},
		{"question", k(tcell.KeyRune, '?', tcell.ModShift), Command{SearchBack, false}},
		{"n", k(tcell.KeyRune, 'n', 0), Command{SearchNext, false}},
		{"N", k(tcell.KeyRune, 'N', 0), Command{SearchPrev, false}},
		{"colon", k(tcell.KeyRune, ':', 0), Command{GoToLine, false}},
		{"w", k(tcell.KeyRune, 'w', 0), Command{ToggleWrap, false}},
		{"q", k(tcell.KeyRune, 'q', 0), Command{Quit, false}},
		{"h", k(tcell.KeyRune, 'h', 0), Command{Help, false}},
		{"unbound", k(tcell.KeyRune, 'z', 0), Command{}},
		{"unbound key", k(tcell.KeyF1, 0, 0), Command{}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			if got := Decode(tc.ev, false); got != tc.want {
				t.Fatalf("Decode = %+v, want %+v", got, tc.want)
			}
		})
	}
	mac := []struct {
		name string
		ev   *tcell.EventKey
		want Command
	}{
		{"cmd c", k(tcell.KeyRune, 'c', tcell.ModMeta), Command{Copy, false}},
		{"cmd a", k(tcell.KeyRune, 'a', tcell.ModMeta), Command{SelectAll, false}},
		{"cmd g", k(tcell.KeyRune, 'g', tcell.ModMeta), Command{SearchNext, false}},
		{"cmd shift g", k(tcell.KeyRune, 'g', tcell.ModMeta|tcell.ModShift), Command{SearchPrev, false}},
		{"cmd shift G as a capital", k(tcell.KeyRune, 'G', tcell.ModMeta|tcell.ModShift), Command{SearchPrev, false}},
		{"cmd up", k(tcell.KeyUp, 0, tcell.ModMeta), Command{First, false}},
		{"cmd shift up", k(tcell.KeyUp, 0, tcell.ModMeta|tcell.ModShift), Command{First, true}},
		{"cmd down", k(tcell.KeyDown, 0, tcell.ModMeta), Command{Last, false}},
		{"cmd left", k(tcell.KeyLeft, 0, tcell.ModMeta), Command{Home, false}},
		{"cmd shift right", k(tcell.KeyRight, 0, tcell.ModMeta|tcell.ModShift), Command{End, true}},
		{"cmd q is unbound", k(tcell.KeyRune, 'q', tcell.ModMeta), Command{}},
	}
	for _, tc := range mac {
		t.Run("macOS "+tc.name, func(t *testing.T) {
			if got := Decode(tc.ev, true); got != tc.want {
				t.Fatalf("Decode = %+v, want %+v", got, tc.want)
			}
		})
	}
	if got := Decode(k(tcell.KeyRune, 'c', tcell.ModMeta), false); got != (Command{}) {
		t.Errorf("cmd c elsewhere = %+v, want unbound", got)
	}
}

func TestIsMovement(t *testing.T) {
	for _, a := range []Action{Up, Down, Left, Right, Home, End, PageUp, PageDown, HalfPageUp, HalfPageDown, WordLeft, WordRight, First, Last} {
		if !IsMovement(a) {
			t.Errorf("IsMovement(%d) = false", a)
		}
	}
	for _, a := range []Action{None, SelectAll, ClearSelection, Copy, Enter, Search, SearchBack, SearchNext, SearchPrev, GoToLine, ToggleWrap, Quit, Help} {
		if IsMovement(a) {
			t.Errorf("IsMovement(%d) = true", a)
		}
	}
}

// TestBindings checks that the OSes differ only by ⌘ keys: none
// elsewhere, and on macOS the same rows once the ⌘ keys are dropped.
func TestBindings(t *testing.T) {
	var stripped []Binding
	for _, b := range Bindings(true) {
		var keys []string
		for _, k := range strings.Split(b.Keys, ", ") {
			if !strings.Contains(k, "⌘") {
				keys = append(keys, k)
			}
		}
		if len(keys) > 0 {
			stripped = append(stripped, Binding{strings.Join(keys, ", "), b.Doc})
		}
	}
	other := Bindings(false)
	for _, b := range other {
		if strings.Contains(b.Keys, "⌘") {
			t.Errorf("row %+v has ⌘ off macOS", b)
		}
	}
	if len(stripped) != len(other) {
		t.Fatalf("macOS has %d rows without ⌘, want %d", len(stripped), len(other))
	}
	for i := range other {
		if stripped[i] != other[i] {
			t.Errorf("row %d: macOS without ⌘ %+v, want %+v", i, stripped[i], other[i])
		}
	}
}

// TestBindingsMatchREADME keeps the help screen and the README's Keys
// table the same list: each row of the table is a Binding, in order,
// with the macOS keys.
func TestBindingsMatchREADME(t *testing.T) {
	bindings := Bindings(true)
	data, err := os.ReadFile("../../README.md")
	if err != nil {
		t.Fatal(err)
	}
	var rows []Binding
	inKeys := false
	for _, line := range strings.Split(string(data), "\n") {
		if strings.HasPrefix(line, "## ") {
			inKeys = line == "## Keys"
			continue
		}
		if !inKeys || !strings.HasPrefix(line, "|") {
			continue
		}
		cells := strings.Split(strings.Trim(line, "|"), "|")
		if len(cells) != 2 {
			t.Fatalf("README row %q: want two cells", line)
		}
		key, doc := strings.TrimSpace(cells[0]), strings.TrimSpace(cells[1])
		if key == "Key" || key == "---" {
			continue
		}
		rows = append(rows, Binding{key, doc})
	}
	if len(rows) != len(bindings) {
		t.Fatalf("README has %d bindings, Bindings has %d", len(rows), len(bindings))
	}
	for i := range rows {
		if rows[i] != bindings[i] {
			t.Errorf("row %d: README %+v, Bindings %+v", i, rows[i], bindings[i])
		}
	}
}
