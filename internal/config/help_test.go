package config

import (
	"os"
	"slices"
	"strings"
	"testing"

	"go.dlh.dev/vedi/internal/input"
)

func TestHelpDefault(t *testing.T) {
	rows := Default(false).Help()
	want := []input.Row{
		{Keys: []string{"↑", "↓", "←", "→", "Home", "End", "j", "k"}, Doc: "Move the cursor"},
		{Keys: []string{"⇟", "Space", "f", "⌃F", "⌃V"}, Doc: "Down a page"},
		{Keys: []string{"⇞", "b", "⌃B", "⌥V"}, Doc: "Up a page"},
	}
	for i, w := range want {
		if !slices.Equal(rows[i].Keys, w.Keys) || rows[i].Doc != w.Doc {
			t.Errorf("row %d = %+v, want %+v", i, rows[i], w)
		}
	}
	for _, r := range rows {
		if r.Doc == "Extend by half a page" {
			t.Error("a row with no keys is shown")
		}
	}
	last := rows[len(rows)-1]
	if !slices.Equal(last.Keys, []string{"h"}) || last.Doc != "Show the key bindings" {
		t.Errorf("last row = %+v", last)
	}
	mac := Default(true).Help()
	if got := mac[0].Keys; !slices.Equal(got, []string{"↑", "↓", "←", "→", "Home", "End", "j", "k", "⌘←", "⌘→"}) {
		t.Errorf("macOS cursor row = %v", got)
	}
}

func TestHelpApplied(t *testing.T) {
	m := Default(false).Apply([]input.Binding{
		{Key: key("h"), Cmd: input.Command{Action: input.Left}},
		{Key: key("Alt+h"), Cmd: input.Command{Action: input.Help}},
		{Key: key("w"), Cmd: input.Command{}},
	})
	rows := m.Help()
	if got := rows[0].Keys; !slices.Equal(got, []string{"↑", "↓", "←", "→", "Home", "End", "j", "k", "h"}) {
		t.Errorf("cursor row = %v", got)
	}
	for _, r := range rows {
		if r.Doc == "Toggle wrap / nowrap" {
			t.Error("unbound action still has a row")
		}
	}
	if last := rows[len(rows)-1]; !slices.Equal(last.Keys, []string{"⌥H"}) {
		t.Errorf("help row = %v", last.Keys)
	}
}

// TestHelpMatchesREADME keeps the README's Keys table and the help
// screen the same list: each row of the table is a input.Row, in order,
// with the macOS keys.
func TestHelpMatchesREADME(t *testing.T) {
	rows := Default(true).Help()
	data, err := os.ReadFile("../../README.md")
	if err != nil {
		t.Fatal(err)
	}
	var got []input.Row
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
		keys, doc := strings.TrimSpace(cells[0]), strings.TrimSpace(cells[1])
		if keys == "Key" || keys == "---" {
			continue
		}
		got = append(got, input.Row{Keys: []string{keys}, Doc: doc})
	}
	if len(got) != len(rows) {
		t.Fatalf("README has %d rows, input.Help has %d", len(got), len(rows))
	}
	for i := range rows {
		// Fixed rows hold a comma themselves, so compare the joined text.
		if keys := strings.Join(rows[i].Keys, ", "); got[i].Keys[0] != keys || got[i].Doc != rows[i].Doc {
			t.Errorf("row %d: README %q %q, input.Help %q %q", i, got[i].Keys[0], got[i].Doc, keys, rows[i].Doc)
		}
	}
}
