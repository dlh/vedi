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
	HalfPageUp
	HalfPageDown
	WordLeft
	WordRight
	First
	Last
	SelectAll
	ClearSelection
	Copy
	Enter
	Search
	SearchBack
	SearchNext
	SearchPrev
	GoToLine
	ToggleWrap
	Quit
	Help
)

// Binding is one row of the help screen: the keys and what they do.
type Binding struct {
	Keys, Doc string
}

// Bindings is the help screen, in order. The README's Keys table is the
// same list.
var Bindings = []Binding{
	{"↑ ↓ ← →, Home, End", "Move the cursor"},
	{"j, k", "Down a line, up a line"},
	{"⇞ ⇟, Space, f, ⌃F, b, ⌃B", "Move by a page"},
	{"⌃V, ⌥V", "Down a page, up a page"},
	{"d, ⌃D, u, ⌃U", "Move by half a page"},
	{"⌃←, ⌃→, ⌥←, ⌥→, ⌥B, ⌥F", "Move by word"},
	{"g, G, <, >", "First line, last line"},
	{"⇧ + ↑ ↓ ← →, Home, End", "Extend the selection"},
	{"⇧⇞, ⇧⇟", "Extend by a page"},
	{"⌃⇧←, ⌃⇧→, ⌥⇧←, ⌥⇧→", "Extend by word"},
	{"Click, drag", "Move the cursor, select"},
	{"Double-click", "Select the word"},
	{"Wheel", "Scroll"},
	{"⌃A", "Select all"},
	{"⎋", "Clear the selection, then the search highlight"},
	{"⌃C, y", "Copy the selection as plain text"},
	{"⏎", "Copy the selection and quit; with none, down a line"},
	{"/", "Search; ignores case unless capitalized; empty repeats"},
	{"?", "Search backward"},
	{"n, N", "Next and previous match; ? swaps them"},
	{":", "Go to a line number"},
	{"w", "Toggle wrap / nowrap"},
	{"q", "Quit"},
	{"h", "Show the key bindings"},
}

// IsMovement reports whether a moves the cursor.
func IsMovement(a Action) bool { return a >= Up && a <= Last }

// Command is a decoded key. Extend is Shift on a movement key: extend
// the selection instead of clearing it.
type Command struct {
	Action Action
	Extend bool
}

// Decode maps a key event to a Command; unbound keys give Command{}.
// Only arrows, Home/End, PgUp/PgDn and Ctrl/Alt+arrows extend: Shift on
// a letter is another letter, and Shift+Space is indistinguishable.
// Alt+arrow is Ctrl+arrow, and Alt+b/Alt+f are the emacs word motions,
// which Terminal.app sends for Option+arrow. Ctrl+V/Alt+v page as in
// emacs; less has Alt+v too.
func Decode(ev *tcell.EventKey) Command {
	shift := ev.Modifiers()&tcell.ModShift != 0
	word := ev.Modifiers()&(tcell.ModCtrl|tcell.ModAlt) != 0
	alt := ev.Modifiers()&tcell.ModAlt != 0
	switch ev.Key() {
	case tcell.KeyUp:
		return Command{Up, shift}
	case tcell.KeyDown:
		return Command{Down, shift}
	case tcell.KeyLeft:
		if word {
			return Command{WordLeft, shift}
		}
		return Command{Left, shift}
	case tcell.KeyRight:
		if word {
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
	case tcell.KeyCtrlF:
		return Command{Action: PageDown}
	case tcell.KeyCtrlB:
		return Command{Action: PageUp}
	case tcell.KeyCtrlV:
		return Command{Action: PageDown}
	case tcell.KeyCtrlD:
		return Command{Action: HalfPageDown}
	case tcell.KeyCtrlU:
		return Command{Action: HalfPageUp}
	case tcell.KeyCtrlA:
		return Command{Action: SelectAll}
	case tcell.KeyEscape:
		return Command{Action: ClearSelection}
	case tcell.KeyCtrlC:
		return Command{Action: Copy}
	case tcell.KeyEnter:
		return Command{Action: Enter}
	case tcell.KeyRune:
		if alt {
			switch ev.Rune() {
			case 'b':
				return Command{Action: WordLeft}
			case 'f':
				return Command{Action: WordRight}
			case 'v':
				return Command{Action: PageUp}
			}
			return Command{}
		}
		switch ev.Rune() {
		case 'j':
			return Command{Action: Down}
		case 'k':
			return Command{Action: Up}
		case ' ', 'f':
			return Command{Action: PageDown}
		case 'b':
			return Command{Action: PageUp}
		case 'd':
			return Command{Action: HalfPageDown}
		case 'u':
			return Command{Action: HalfPageUp}
		case 'g', '<':
			return Command{Action: First}
		case 'G', '>':
			return Command{Action: Last}
		case 'y':
			return Command{Action: Copy}
		case '/':
			return Command{Action: Search}
		case '?':
			return Command{Action: SearchBack}
		case 'n':
			return Command{Action: SearchNext}
		case 'N':
			return Command{Action: SearchPrev}
		case ':':
			return Command{Action: GoToLine}
		case 'w':
			return Command{Action: ToggleWrap}
		case 'q':
			return Command{Action: Quit}
		case 'h':
			return Command{Action: Help}
		}
	}
	return Command{}
}
