package config

import (
	"os"
	"slices"
	"strings"
	"testing"

	"go.dlh.dev/vedi/internal/input"
)

func rows(secs []input.Section) []input.Row {
	var all []input.Row
	for _, s := range secs {
		all = append(all, s.Rows...)
	}
	return all
}

func TestHelpDefault(t *testing.T) {
	secs := Default(false).Help()
	top := secs[0]
	if top.Name != "" || len(top.Rows) != 4 || top.Rows[0].Doc != "Show the key bindings" || !slices.Equal(top.Rows[0].Keys, []string{"h"}) {
		t.Errorf("top section = %+v", top)
	}
	moving := secs[1]
	if moving.Name != "Moving" {
		t.Errorf("second section = %q", moving.Name)
	}
	want := []input.Row{
		{Keys: []string{"Up", "Down", "Left", "Right", "Home", "End", "j", "k"}, Doc: "Move the cursor"},
		{Keys: []string{"PgDn", "Space", "f", "Ctrl+f", "Ctrl+v"}, Doc: "Down a page"},
		{Keys: []string{"PgUp", "b", "Ctrl+b", "Alt+v"}, Doc: "Up a page"},
	}
	for i, w := range want {
		if !slices.Equal(moving.Rows[i].Keys, w.Keys) || moving.Rows[i].Doc != w.Doc {
			t.Errorf("row %d = %+v, want %+v", i, moving.Rows[i], w)
		}
	}
	if last := moving.Rows[len(moving.Rows)-1]; last.Doc != "Command prompt; N goes to line N" {
		t.Errorf("last moving row = %+v", last)
	}
	var names []string
	for _, s := range secs {
		names = append(names, s.Name)
	}
	if want := []string{"", "Moving", "Selecting", "Mouse", "Copying", "Searching", "Prompts"}; !slices.Equal(names, want) {
		t.Errorf("sections = %v", names)
	}
	all := rows(secs)
	for _, r := range all {
		if r.Doc == "Extend by half a page" {
			t.Error("a row with no keys is shown")
		}
	}
	last := all[len(all)-1]
	if !slices.Equal(last.Keys, []string{"Up", "Down"}) || last.Doc != "Recall earlier searches, or commands" {
		t.Errorf("last row = %+v", last)
	}
	mac := Default(true).Help()
	if got := mac[1].Rows[0].Keys; !slices.Equal(got, []string{"Up", "Down", "Left", "Right", "Home", "End", "j", "k", "Cmd+Left", "Cmd+Right"}) {
		t.Errorf("macOS cursor row = %v", got)
	}
}

func TestHelpApplied(t *testing.T) {
	m := Default(false).Apply([]input.Binding{
		{Key: key("h"), Cmd: input.Command{Action: input.Left}},
		{Key: key("Alt+h"), Cmd: input.Command{Action: input.Help}},
		{Key: key("w"), Cmd: input.Command{}},
	})
	secs := m.Help()
	if got := secs[1].Rows[0].Keys; !slices.Equal(got, []string{"Up", "Down", "Left", "Right", "Home", "End", "j", "k", "h"}) {
		t.Errorf("cursor row = %v", got)
	}
	for _, r := range rows(secs) {
		if r.Doc == "Toggle wrap / nowrap" {
			t.Error("unbound action still has a row")
		}
	}
	if got := secs[0].Rows[0].Keys; !slices.Equal(got, []string{"Alt+h"}) {
		t.Errorf("help row = %v", got)
	}
}

// TestHelpEmptySection: a section none of whose rows has a key is
// left out.
func TestHelpEmptySection(t *testing.T) {
	m := Default(false).Apply([]input.Binding{
		{Key: key("Ctrl+c"), Cmd: input.Command{}},
		{Key: key("y"), Cmd: input.Command{}},
		{Key: key("Enter"), Cmd: input.Command{}},
	})
	for _, s := range m.Help() {
		if s.Name == "Copying" {
			t.Error("Copying is shown with nothing in it")
		}
	}
}

// TestHelpMatchesKeysDoc keeps the table in docs/keys.md and the help
// screen the same list: each row of the table is a input.Row, in order,
// with the macOS keys.
func TestHelpMatchesKeysDoc(t *testing.T) {
	all := rows(Default(true).Help())
	data, err := os.ReadFile("../../docs/keys.md")
	if err != nil {
		t.Fatal(err)
	}
	var got []input.Row
	inKeys := false
	for line := range strings.SplitSeq(string(data), "\n") {
		if strings.HasPrefix(line, "#") {
			inKeys = line == "# Keys"
			continue
		}
		if !inKeys || !strings.HasPrefix(line, "|") {
			continue
		}
		cells := strings.Split(strings.Trim(line, "|"), "|")
		if len(cells) != 2 {
			t.Fatalf("docs/keys.md row %q: want two cells", line)
		}
		keys, doc := strings.TrimSpace(cells[0]), strings.TrimSpace(cells[1])
		if keys == "Key" || keys == "---" {
			continue
		}
		got = append(got, input.Row{Keys: []string{keys}, Doc: doc})
	}
	if len(got) != len(all) {
		t.Fatalf("docs/keys.md has %d rows, input.Help has %d", len(got), len(all))
	}
	for i := range all {
		// Fixed rows hold a comma themselves, so compare the joined text.
		if keys := strings.Join(all[i].Keys, ", "); got[i].Keys[0] != keys || got[i].Doc != all[i].Doc {
			t.Errorf("row %d: docs/keys.md %q %q, input.Help %q %q", i, got[i].Keys[0], got[i].Doc, keys, all[i].Doc)
		}
	}
}
