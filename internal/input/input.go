// Package input maps key events to pager actions.
package input

type Action int

// Movement actions come first so IsMovement can range-check.
const (
	None Action = iota
	Up
	Down
	Left
	Right
	Home
	End
	PageUp
	PageDown
	HalfPageUp
	HalfPageDown
	WordLeft
	WordRight
	First
	Last
	SelectAll
	ClearSelection
	SetMark
	Copy
	CopyAndQuit
	Search
	SearchBack
	SearchNext
	SearchPrev
	GoToLine
	ToggleWrap
	Quit
	Help
)

// IsMovement reports whether a moves the cursor.
func IsMovement(a Action) bool { return a >= Up && a <= Last }

// Command is a decoded key. Extend is Shift on a movement key: extend
// the selection instead of clearing it.
type Command struct {
	Action Action
	Extend bool
}
