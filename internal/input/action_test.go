package input

import (
	"slices"
	"strings"
	"testing"
)

func TestActionNames(t *testing.T) {
	for a := None; a <= Help; a++ {
		if a == Cycle {
			continue // takes a setting; TestCycle
		}
		name := a.String()
		if name == "" {
			t.Fatalf("action %d has no name", a)
		}
		got, err := ParseCommand(name)
		if err != nil || got != (Command{Action: a}) {
			t.Errorf("ParseCommand(%q) = %+v, %v; want %+v", name, got, err, Command{Action: a})
		}
		if !IsMovement(a) {
			continue
		}
		sel := "select_" + name
		got, err = ParseCommand(sel)
		if err != nil || got != (Command{Action: a, Extend: true}) {
			t.Errorf("ParseCommand(%q) = %+v, %v; want %+v", sel, got, err, Command{Action: a, Extend: true})
		}
		if got.String() != sel {
			t.Errorf("Command.String = %q, want %q", got.String(), sel)
		}
	}
	for _, name := range []string{"select_all_x", "select_copy", "select_none", "Up", "page-down", "", "toggle_wrap", "left now"} {
		if _, err := ParseCommand(name); err == nil {
			t.Errorf("ParseCommand(%q) = nil error", name)
		}
	}
	if got := (Command{Action: CopyAndQuit}).String(); got != "copy_and_quit" {
		t.Errorf("CopyAndQuit = %q", got)
	}
	if got := (Command{Action: SetMark}).String(); got != "set_mark" {
		t.Errorf("SetMark = %q", got)
	}
	if got := (Command{Action: SelectAll}).String(); got != "select_all" {
		t.Errorf("SelectAll = %q", got)
	}
}

// TestCommandNames: every name parses back to itself, the select_
// forms included, and none is unparseable; cycle, which needs a
// setting, is listed and parses with each.
func TestCommandNames(t *testing.T) {
	names := CommandNames()
	if !slices.Contains(names, "none") || !slices.Contains(names, "select_page_down") || slices.Contains(names, "select_copy") || !slices.Contains(names, "cycle") {
		t.Errorf("CommandNames = %v", names)
	}
	if !slices.IsSorted(names) {
		t.Errorf("CommandNames not sorted: %v", names)
	}
	for _, n := range names {
		if n == "cycle" {
			continue
		}
		c, err := ParseCommand(n)
		if err != nil || c.String() != n {
			t.Errorf("ParseCommand(%q) = %v, %v", n, c, err)
		}
	}
}

// TestCycle: cycle takes one of the settings, in any spacing, and
// prints back as cycle and the setting; alone or with anything else
// it is an error naming the settings.
func TestCycle(t *testing.T) {
	want := []string{"wrap", "wrap_style", "edge_markers", "auto_reload"}
	if got := SettingNames(); !slices.Equal(got, want) {
		t.Errorf("SettingNames = %v, want %v", got, want)
	}
	for _, name := range want {
		c, err := ParseCommand("cycle  " + name)
		if err != nil || c != (Command{Action: Cycle, Arg: name}) || c.String() != "cycle "+name {
			t.Errorf("ParseCommand(%q) = %+v, %v", "cycle "+name, c, err)
		}
	}
	for _, text := range []string{"cycle", "cycle tab_width", "cycle wrap yes"} {
		_, err := ParseCommand(text)
		if err == nil || !strings.Contains(err.Error(), "cycle takes wrap, wrap_style, edge_markers or auto_reload") {
			t.Errorf("ParseCommand(%q) = %v", text, err)
		}
	}
	if _, err := ParseCommand("left wrap"); err == nil || err.Error() != `unknown action "left wrap"` {
		t.Errorf("ParseCommand(\"left wrap\") = %v", err)
	}
}

// TestSettingNext: Next steps through a setting's values and comes
// back round; an unknown value starts over.
func TestSettingNext(t *testing.T) {
	s, ok := LookupSetting("wrap_style")
	if !ok || !slices.Equal(s.Values, []string{"char", "word"}) {
		t.Fatalf("LookupSetting(wrap_style) = %+v, %v", s, ok)
	}
	for _, tc := range []struct{ cur, next string }{{"char", "word"}, {"word", "char"}, {"", "char"}} {
		if got := s.Next(tc.cur); got != tc.next {
			t.Errorf("Next(%q) = %q, want %q", tc.cur, got, tc.next)
		}
	}
	if _, ok := LookupSetting("tab_width"); ok {
		t.Error("LookupSetting(tab_width) found")
	}
}

// TestGoToLineAlias: go_to_line, the command prompt's old name, still
// parses; command is the name it prints.
func TestGoToLineAlias(t *testing.T) {
	for _, name := range []string{"command", "go_to_line"} {
		c, err := ParseCommand(name)
		if err != nil || c != (Command{Action: CommandPrompt}) || c.String() != "command" {
			t.Errorf("ParseCommand(%q) = %+v, %v", name, c, err)
		}
	}
	if slices.Contains(CommandNames(), "go_to_line") {
		t.Error("CommandNames lists the alias")
	}
}
