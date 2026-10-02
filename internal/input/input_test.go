package input

import "testing"

func TestIsMovement(t *testing.T) {
	for _, a := range []Action{Up, Down, Left, Right, Home, End, PageUp, PageDown, HalfPageUp, HalfPageDown, WordLeft, WordRight, First, Last} {
		if !IsMovement(a) {
			t.Errorf("IsMovement(%d) = false", a)
		}
	}
	for _, a := range []Action{None, SelectAll, ClearSelection, Copy, CopyAndQuit, Search, SearchBack, SearchNext, SearchPrev, CommandPrompt, ToggleWrap, Quit, Help} {
		if IsMovement(a) {
			t.Errorf("IsMovement(%d) = true", a)
		}
	}
}
