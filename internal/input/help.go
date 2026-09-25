package input

import "slices"

// Row is one line of the help screen: the keys bound to its actions,
// in map order, and what they do. Fixed rows, for the mouse and the
// prompt, name their keys themselves.
type Row struct {
	Keys []string
	Doc  string
}

// Section is a heading and the rows under it.
type Section struct {
	Name string
	Rows []Row
}

type helpRow struct {
	cmds  []Command
	fixed string
	doc   string
}

type helpSection struct {
	name string
	rows []helpRow
}

func act(a Action) Command { return Command{Action: a} }
func sel(a Action) Command { return Command{a, true} }

// helpSections is the help screen in order: the first, unnamed, is
// shown at the top as less shows h and q. The README's Keys table is
// the same rows with the macOS keys.
var helpSections = []helpSection{
	{"", []helpRow{
		{cmds: []Command{act(Help)}, doc: "Show the key bindings"},
		{cmds: []Command{act(Quit)}, doc: "Quit"},
		{cmds: []Command{act(ToggleWrap)}, doc: "Toggle wrap / nowrap"},
		{cmds: []Command{act(Reload)}, doc: "Reload the file"},
	}},
	{"Moving", []helpRow{
		{cmds: []Command{act(Up), act(Down), act(Left), act(Right), act(Home), act(End)}, doc: "Move the cursor"},
		{cmds: []Command{act(PageDown)}, doc: "Down a page"},
		{cmds: []Command{act(PageUp)}, doc: "Up a page"},
		{cmds: []Command{act(HalfPageDown)}, doc: "Down half a page"},
		{cmds: []Command{act(HalfPageUp)}, doc: "Up half a page"},
		{cmds: []Command{act(WordLeft), act(WordRight)}, doc: "Move by word"},
		{cmds: []Command{act(First), act(Last)}, doc: "First line, last line"},
		{cmds: []Command{act(GoToLine)}, doc: "Go to a line number"},
	}},
	{"Selecting", []helpRow{
		{cmds: []Command{sel(Up), sel(Down), sel(Left), sel(Right), sel(Home), sel(End), sel(First), sel(Last)}, doc: "Extend the selection"},
		{cmds: []Command{sel(PageDown), sel(PageUp)}, doc: "Extend by a page"},
		{cmds: []Command{sel(HalfPageDown), sel(HalfPageUp)}, doc: "Extend by half a page"},
		{cmds: []Command{sel(WordLeft), sel(WordRight)}, doc: "Extend by word"},
		{cmds: []Command{act(SetMark)}, doc: "Start a selection that motions extend; again, end it"},
		{cmds: []Command{act(SelectAll)}, doc: "Select all"},
		{cmds: []Command{act(ClearSelection)}, doc: "Clear the selection, then the search highlight"},
	}},
	{"Mouse", []helpRow{
		{fixed: "Click, drag", doc: "Move the cursor, select; held at an edge, scroll"},
		{fixed: "Double-, triple-click", doc: "Select the word, the line"},
		{fixed: "Wheel", doc: "Scroll"},
	}},
	{"Copying", []helpRow{
		{cmds: []Command{act(Copy)}, doc: "Copy the selection as plain text"},
		{cmds: []Command{act(CopyAndQuit)}, doc: "Copy the selection and quit; with none, down a line"},
	}},
	{"Searching", []helpRow{
		{cmds: []Command{act(Search)}, doc: "Search; ignores case if lowercase; empty repeats"},
		{cmds: []Command{act(SearchBack)}, doc: "Search backward"},
		{fixed: "Up, Down at the prompt", doc: "Recall earlier searches"},
		{cmds: []Command{act(SearchNext), act(SearchPrev)}, doc: "Next and previous match; ? swaps them"},
	}},
}

// Help is the help screen for the map: a row none of whose actions
// has a key is left out, and a section with no rows left.
func (m Keymap) Help() []Section {
	var secs []Section
	for _, s := range helpSections {
		var rows []Row
		for _, h := range s.rows {
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
		if len(rows) > 0 {
			secs = append(secs, Section{s.name, rows})
		}
	}
	return secs
}
