package input

import (
	"os"
	"strings"
	"testing"
)

func TestIsMovement(t *testing.T) {
	for _, a := range []Action{Up, Down, Left, Right, Home, End, PageUp, PageDown, HalfPageUp, HalfPageDown, WordLeft, WordRight, First, Last} {
		if !IsMovement(a) {
			t.Errorf("IsMovement(%d) = false", a)
		}
	}
	for _, a := range []Action{None, SelectAll, ClearSelection, Copy, CopyAndQuit, Search, SearchBack, SearchNext, SearchPrev, GoToLine, ToggleWrap, Quit, Help} {
		if IsMovement(a) {
			t.Errorf("IsMovement(%d) = true", a)
		}
	}
}

// TestBindings checks that the OSes differ only by ⌘ keys: none
// elsewhere, and on macOS the same rows once the ⌘ keys are dropped.
func TestBindings(t *testing.T) {
	var stripped []HelpRow
	for _, b := range Bindings(true) {
		var keys []string
		for _, k := range strings.Split(b.Keys, ", ") {
			if !strings.Contains(k, "⌘") {
				keys = append(keys, k)
			}
		}
		if len(keys) > 0 {
			stripped = append(stripped, HelpRow{strings.Join(keys, ", "), b.Doc})
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
// table the same list: each row of the table is a HelpRow, in order,
// with the macOS keys.
func TestBindingsMatchREADME(t *testing.T) {
	bindings := Bindings(true)
	data, err := os.ReadFile("../../README.md")
	if err != nil {
		t.Fatal(err)
	}
	var rows []HelpRow
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
		rows = append(rows, HelpRow{key, doc})
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
