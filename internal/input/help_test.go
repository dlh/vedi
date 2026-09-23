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
	for a := Up; a <= Help; a++ {
		if !slices.Contains(covered, Command{Action: a}) {
			t.Errorf("%s has no help row", a)
		}
		if IsMovement(a) && !slices.Contains(covered, Command{a, true}) {
			t.Errorf("select_%s has no help row", a)
		}
	}
}
