package input

import "slices"

// Row is one line of the help screen: the keys bound to its actions,
// in map order, and what they do. Fixed rows, for the mouse and the
// prompt, name their keys themselves.
type Row struct {
	Keys []string
	Doc  string
}

type helpRow struct {
	cmds  []Command
	fixed string
	doc   string
}

func act(a Action) Command { return Command{Action: a} }
func sel(a Action) Command { return Command{a, true} }

// helpRows is the help screen in order. The README's Keys table is
// the same list with the macOS keys.
var helpRows = []helpRow{
	{cmds: []Command{act(Up), act(Down), act(Left), act(Right), act(Home), act(End)}, doc: "Move the cursor"},
	{cmds: []Command{act(PageDown)}, doc: "Down a page"},
	{cmds: []Command{act(PageUp)}, doc: "Up a page"},
	{cmds: []Command{act(HalfPageDown)}, doc: "Down half a page"},
	{cmds: []Command{act(HalfPageUp)}, doc: "Up half a page"},
	{cmds: []Command{act(WordLeft), act(WordRight)}, doc: "Move by word"},
	{cmds: []Command{act(First), act(Last)}, doc: "First line, last line"},
	{cmds: []Command{sel(Up), sel(Down), sel(Left), sel(Right), sel(Home), sel(End), sel(First), sel(Last)}, doc: "Extend the selection"},
	{cmds: []Command{sel(PageDown), sel(PageUp)}, doc: "Extend by a page"},
	{cmds: []Command{sel(HalfPageDown), sel(HalfPageUp)}, doc: "Extend by half a page"},
	{cmds: []Command{sel(WordLeft), sel(WordRight)}, doc: "Extend by word"},
	{fixed: "Click, drag", doc: "Move the cursor, select; held at an edge, scroll"},
	{fixed: "Double-, triple-click", doc: "Select the word, the line"},
	{fixed: "Wheel", doc: "Scroll"},
	{cmds: []Command{act(SelectAll)}, doc: "Select all"},
	{cmds: []Command{act(ClearSelection)}, doc: "Clear the selection, then the search highlight"},
	{cmds: []Command{act(Copy)}, doc: "Copy the selection as plain text"},
	{cmds: []Command{act(CopyAndQuit)}, doc: "Copy the selection and quit; with none, down a line"},
	{cmds: []Command{act(Search)}, doc: "Search; ignores case if lowercase; empty repeats"},
	{cmds: []Command{act(SearchBack)}, doc: "Search backward"},
	{fixed: "↑ ↓ at the prompt", doc: "Recall earlier searches"},
	{cmds: []Command{act(SearchNext), act(SearchPrev)}, doc: "Next and previous match; ? swaps them"},
	{cmds: []Command{act(GoToLine)}, doc: "Go to a line number"},
	{cmds: []Command{act(ToggleWrap)}, doc: "Toggle wrap / nowrap"},
	{cmds: []Command{act(Quit)}, doc: "Quit"},
	{cmds: []Command{act(Help)}, doc: "Show the key bindings"},
}

// Help is the help screen for the map; a row none of whose actions
// has a key is left out.
func (m Keymap) Help() []Row {
	var rows []Row
	for _, h := range helpRows {
		if h.fixed != "" {
			rows = append(rows, Row{[]string{h.fixed}, h.doc})
			continue
		}
		var keys []string
		for _, b := range m {
			if slices.Contains(h.cmds, b.Cmd) {
				keys = append(keys, b.Key.String())
			}
		}
		if len(keys) > 0 {
			rows = append(rows, Row{keys, h.doc})
		}
	}
	return rows
}
