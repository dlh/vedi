// Package input maps key events to pager actions.
package input

import "github.com/gdamore/tcell/v2"

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
	WordLeft
	WordRight
	First
	Last
	SelectAll
	ClearSelection
	Copy
	Enter
	Search
	SearchNext
	SearchPrev
	ToggleWrap
	Quit
	Help
)

// Binding is one row of the help screen: the keys and what they do.
type Binding struct {
	Keys, Doc string
}

// Bindings lists every key, in the order the help screen shows them.
// The README's Keys table is the same list.
var Bindings = []Binding{
	{"↑ ↓ ← →, Home, End", "Move the cursor"},
	{"⇞ ⇟, Space, b", "Move by a page"},
	{"⌃←, ⌃→", "Move by word"},
	{"g, G", "First line, last line"},
	{"⇧ + ↑ ↓ ← →, Home, End", "Extend the selection"},
	{"⇧⇞, ⇧⇟", "Extend by a page"},
	{"⌃⇧←, ⌃⇧→", "Extend by word"},
	{"⌃A", "Select all"},
	{"⎋", "Clear the selection, then the search highlight"},
	{"⌃C, y", "Copy the selection as plain text"},
	{"⏎", "Copy the selection and quit; with none, down a line"},
	{"/", "Search (smartcase); n and N for next and previous"},
	{"w", "Toggle wrap / nowrap"},
	{"q", "Quit"},
	{"?", "Show the key bindings"},
}

// IsMovement reports whether a moves the cursor.
func IsMovement(a Action) bool { return a >= Up && a <= Last }

// Command is a decoded key. Extend is set when Shift was held on a
// movement key, which extends the selection instead of clearing it.
type Command struct {
	Action Action
	Extend bool
}

// Decode maps a key event to a Command. Unbound keys give Command{}.
// Only arrows, Home/End, PgUp/PgDn and Ctrl+arrows extend: Shift on a
// letter is a different letter, and Shift+Space is not distinguishable.
func Decode(ev *tcell.EventKey) Command {
	shift := ev.Modifiers()&tcell.ModShift != 0
	ctrl := ev.Modifiers()&tcell.ModCtrl != 0
	switch ev.Key() {
	case tcell.KeyUp:
		return Command{Up, shift}
	case tcell.KeyDown:
		return Command{Down, shift}
	case tcell.KeyLeft:
		if ctrl {
			return Command{WordLeft, shift}
		}
		return Command{Left, shift}
	case tcell.KeyRight:
		if ctrl {
			return Command{WordRight, shift}
		}
		return Command{Right, shift}
	case tcell.KeyHome:
		return Command{Home, shift}
	case tcell.KeyEnd:
		return Command{End, shift}
	case tcell.KeyPgUp:
		return Command{PageUp, shift}
	case tcell.KeyPgDn:
		return Command{PageDown, shift}
	case tcell.KeyCtrlA:
		return Command{Action: SelectAll}
	case tcell.KeyEscape:
		return Command{Action: ClearSelection}
	case tcell.KeyCtrlC:
		return Command{Action: Copy}
	case tcell.KeyEnter:
		return Command{Action: Enter}
	case tcell.KeyRune:
		switch ev.Rune() {
		case ' ':
			return Command{Action: PageDown}
		case 'b':
			return Command{Action: PageUp}
		case 'g':
			return Command{Action: First}
		case 'G':
			return Command{Action: Last}
		case 'y':
			return Command{Action: Copy}
		case '/':
			return Command{Action: Search}
		case 'n':
			return Command{Action: SearchNext}
		case 'N':
			return Command{Action: SearchPrev}
		case 'w':
			return Command{Action: ToggleWrap}
		case 'q':
			return Command{Action: Quit}
		case '?':
			return Command{Action: Help}
		}
	}
	return Command{}
}
