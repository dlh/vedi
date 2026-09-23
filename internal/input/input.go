// Package input maps key events to pager actions.
package input

import "slices"

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

// HelpRow is one row of the help screen: the keys and what they do.
type HelpRow struct {
	Keys, Doc string
}

// Bindings is the help screen, in order; on macOS with the ⌘ keys. The
// README's Keys table is the macOS list.
func Bindings(macOS bool) []HelpRow {
	b := slices.Clone(bindings)
	if !macOS {
		return b
	}
	row := func(keys string) int {
		return slices.IndexFunc(b, func(b HelpRow) bool { return b.Keys == keys })
	}
	b[row("⌃A")].Keys = "⌃A, ⌘A"
	b[row("⌃C, y")].Keys = "⌃C, ⌘C, y"
	b[row("n, N")].Keys = "n, N, ⌘G, ⇧⌘G"
	return slices.Insert(b, row("g, G, <, >")+1, HelpRow{"⌘←, ⌘→, ⌘↑, ⌘↓", "Line start and end, first and last line"})
}

var bindings = []HelpRow{
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
	{"Click, drag", "Move the cursor, select; held at an edge, scroll"},
	{"Double-, triple-click", "Select the word, the line"},
	{"Wheel", "Scroll"},
	{"⌃A", "Select all"},
	{"⎋", "Clear the selection, then the search highlight"},
	{"⌃C, y", "Copy the selection as plain text"},
	{"⏎", "Copy the selection and quit; with none, down a line"},
	{"/", "Search; ignores case unless capitalized; empty repeats"},
	{"?", "Search backward"},
	{"↑ ↓ at the prompt", "Recall earlier searches"},
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
