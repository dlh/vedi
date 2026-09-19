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
		{"shift home", k(tcell.KeyHome, 0, tcell.ModShift), Command{Home, true}},
		{"end", k(tcell.KeyEnd, 0, 0), Command{End, false}},
		{"shift pgup", k(tcell.KeyPgUp, 0, tcell.ModShift), Command{PageUp, true}},
		{"pgdn", k(tcell.KeyPgDn, 0, 0), Command{PageDown, false}},
		{"space", k(tcell.KeyRune, ' ', 0), Command{PageDown, false}},
		{"b", k(tcell.KeyRune, 'b', 0), Command{PageUp, false}},
		{"g", k(tcell.KeyRune, 'g', 0), Command{First, false}},
		{"G never extends", k(tcell.KeyRune, 'G', tcell.ModShift), Command{Last, false}},
		{"ctrl a", k(tcell.KeyCtrlA, 0, tcell.ModCtrl), Command{SelectAll, false}},
		{"esc", k(tcell.KeyEscape, 0, 0), Command{ClearSelection, false}},
		{"ctrl c", k(tcell.KeyCtrlC, 0, tcell.ModCtrl), Command{Copy, false}},
		{"y", k(tcell.KeyRune, 'y', 0), Command{Copy, false}},
		{"enter", k(tcell.KeyEnter, 0, 0), Command{Enter, false}},
		{"slash", k(tcell.KeyRune, '/', 0), Command{Search, false}},
		{"n", k(tcell.KeyRune, 'n', 0), Command{SearchNext, false}},
		{"N", k(tcell.KeyRune, 'N', 0), Command{SearchPrev, false}},
		{"w", k(tcell.KeyRune, 'w', 0), Command{ToggleWrap, false}},
		{"q", k(tcell.KeyRune, 'q', 0), Command{Quit, false}},
		{"?", k(tcell.KeyRune, '?', 0), Command{Help, false}},
		{"unbound", k(tcell.KeyRune, 'z', 0), Command{}},
		{"unbound key", k(tcell.KeyF1, 0, 0), Command{}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			if got := Decode(tc.ev); got != tc.want {
				t.Fatalf("Decode = %+v, want %+v", got, tc.want)
			}
		})
	}
}

func TestIsMovement(t *testing.T) {
	for _, a := range []Action{Up, Down, Left, Right, Home, End, PageUp, PageDown, WordLeft, WordRight, First, Last} {
		if !IsMovement(a) {
			t.Errorf("IsMovement(%d) = false", a)
		}
	}
	for _, a := range []Action{None, SelectAll, ClearSelection, Copy, Enter, Search, SearchNext, SearchPrev, ToggleWrap, Quit, Help} {
		if IsMovement(a) {
			t.Errorf("IsMovement(%d) = true", a)
		}
	}
}

// TestBindingsMatchREADME keeps the help screen and the README's Keys
// table the same list: each row of the table is a Binding, in order.
func TestBindingsMatchREADME(t *testing.T) {
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
	if len(rows) != len(Bindings) {
		t.Fatalf("README has %d bindings, Bindings has %d", len(rows), len(Bindings))
	}
	for i := range rows {
		if rows[i] != Bindings[i] {
			t.Errorf("row %d: README %+v, Bindings %+v", i, rows[i], Bindings[i])
		}
	}
}
