package input

import "testing"

func TestActionNames(t *testing.T) {
	for a := None; a <= Help; a++ {
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
		if err != nil || got != (Command{a, true}) {
			t.Errorf("ParseCommand(%q) = %+v, %v; want %+v", sel, got, err, Command{a, true})
		}
		if got.String() != sel {
			t.Errorf("Command.String = %q, want %q", got.String(), sel)
		}
	}
	for _, name := range []string{"select_all_x", "select_copy", "select_none", "Up", "page-down", ""} {
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
