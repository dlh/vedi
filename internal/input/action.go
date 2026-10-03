package input

import (
	"fmt"
	"slices"
	"strings"
)

var actionNames = [...]string{
	None: "none", Up: "up", Down: "down", Left: "left", Right: "right",
	Home: "home", End: "end", PageUp: "page_up", PageDown: "page_down",
	HalfPageUp: "half_page_up", HalfPageDown: "half_page_down",
	WordLeft: "word_left", WordRight: "word_right", First: "first", Last: "last",
	PrevFile: "prev_file", NextFile: "next_file",
	SelectAll: "select_all", ClearSelection: "clear_selection", SetMark: "set_mark", Copy: "copy",
	CopyAndQuit: "copy_and_quit", Search: "search", SearchBack: "search_back",
	SearchNext: "search_next", SearchPrev: "search_prev", CommandPrompt: "command",
	Cycle: "cycle", Quit: "quit", Help: "help", Reload: "reload",
}

// String is the config name.
func (a Action) String() string { return actionNames[a] }

// String is the config name: select_ before a movement that extends,
// cycle and its setting.
func (c Command) String() string {
	switch {
	case c.Extend:
		return "select_" + c.Action.String()
	case c.Action == Cycle:
		return "cycle " + c.Arg
	}
	return c.Action.String()
}

func action(name string) (Action, bool) {
	for a, n := range actionNames {
		if n == name {
			return Action(a), true
		}
	}
	return None, false
}

// CommandNames is every name ParseCommand takes, sorted.
func CommandNames() []string {
	var names []string
	for a, n := range actionNames {
		names = append(names, n)
		if IsMovement(Action(a)) {
			names = append(names, "select_"+n)
		}
	}
	slices.Sort(names)
	return names
}

// ParseCommand reads an action, cycle with its setting; none is the
// zero Command. go_to_line is command's old name.
func ParseCommand(text string) (Command, error) {
	f := strings.Fields(text)
	if len(f) > 0 && f[0] == "cycle" {
		if len(f) != 2 {
			return Command{}, errCycle()
		}
		if _, ok := LookupSetting(f[1]); !ok {
			return Command{}, errCycle()
		}
		return Command{Action: Cycle, Arg: f[1]}, nil
	}
	name := strings.Join(f, " ")
	if name == "go_to_line" {
		name = "command"
	}
	if a, ok := action(name); ok {
		return Command{Action: a}, nil
	}
	if base, ok := strings.CutPrefix(name, "select_"); ok {
		if a, ok := action(base); ok && IsMovement(a) {
			return Command{Action: a, Extend: true}, nil
		}
	}
	return Command{}, fmt.Errorf("unknown action %q", name)
}
