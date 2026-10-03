package input

import (
	"slices"
	"testing"
)

// TestHelpCoversActions: every action a user can map has a row, or
// its binding would vanish from the help screen.
func TestHelpCoversActions(t *testing.T) {
	var covered []Command
	for _, s := range helpSections {
		for _, h := range s.rows {
			covered = append(covered, h.cmds...)
		}
	}
	for _, name := range SettingNames() {
		if !slices.Contains(covered, Command{Action: Cycle, Arg: name}) {
			t.Errorf("cycle %s has no help row", name)
		}
	}
	for a := Up; a <= Reload; a++ {
		if a == Cycle {
			continue
		}
		if !slices.Contains(covered, Command{Action: a}) {
			t.Errorf("%s has no help row", a)
		}
		if IsMovement(a) && !slices.Contains(covered, Command{Action: a, Extend: true}) {
			t.Errorf("select_%s has no help row", a)
		}
	}
}
