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
	SelectAll: "select_all", ClearSelection: "clear_selection", SetMark: "set_mark", Copy: "copy",
	CopyAndQuit: "copy_and_quit", Search: "search", SearchBack: "search_back",
	SearchNext: "search_next", SearchPrev: "search_prev", CommandPrompt: "command",
	ToggleWrap: "toggle_wrap", Quit: "quit", Help: "help", Reload: "reload",
}

// String is the config name.
func (a Action) String() string { return actionNames[a] }

// String is the config name: select_ before a movement that extends.
func (c Command) String() string {
	if c.Extend {
		return "select_" + c.Action.String()
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

// ParseCommand reads an action name; none is the zero Command.
// go_to_line is command's old name.
func ParseCommand(name string) (Command, error) {
	if name == "go_to_line" {
		name = "command"
	}
	if a, ok := action(name); ok {
		return Command{Action: a}, nil
	}
	if base, ok := strings.CutPrefix(name, "select_"); ok {
		if a, ok := action(base); ok && IsMovement(a) {
			return Command{a, true}, nil
		}
	}
	return Command{}, fmt.Errorf("unknown action %q", name)
}
