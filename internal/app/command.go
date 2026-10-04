package app

import (
	"fmt"
	"slices"
	"strconv"
	"strings"

	"github.com/gdamore/tcell/v3"
	"go.dlh.dev/vedi/internal/ansi"
	"go.dlh.dev/vedi/internal/buffer"
	"go.dlh.dev/vedi/internal/clipboard"
	"go.dlh.dev/vedi/internal/config"
	"go.dlh.dev/vedi/internal/input"
	"go.dlh.dev/vedi/internal/layout"
)

// handleCommandKey edits the : prompt. Enter runs what was typed and
// closes the prompt, reporting whether it quit, unless Tab's list of commands is showing and the
// prompt holds one of them: then it adds the space after the command
// and keeps the prompt open, for the argument. Esc, Ctrl+g and Ctrl+c
// just close it.
func (a *App) handleCommandKey(ev *tcell.EventKey) bool {
	_, before := lastWord(a.stem)
	listingCommands := a.matches != nil && string(a.command.text) == a.filled && len(before) == 0
	if ev.Key() != tcell.KeyTab {
		a.matches = nil
	}
	switch {
	case cancels(ev):
		a.commanding = false
	case ev.Key() == tcell.KeyEnter && listingCommands && slices.Contains(commands, strings.TrimSuffix(a.filled, " ")):
		a.setCommand(strings.TrimSuffix(a.filled, " ") + " ")
	case ev.Key() == tcell.KeyEnter:
		a.commanding = false
		if len(a.command.text) > 0 {
			a.command.remember()
		}
		return a.runCommand(string(a.command.text))
	case ev.Key() == tcell.KeyTab:
		a.complete()
	default:
		a.command.edit(ev)
	}
	return false
}

// complete fills in the word being typed at the end of the : prompt
// from the candidates for its place: with one, the word and a space;
// with several, what they share, listing them sorted; with none,
// nothing. Tab again, the line as the last left it, fills in each match in turn
// and then what was typed.
func (a *App) complete() {
	if a.matches != nil && string(a.command.text) == a.filled {
		a.matchPos++
		if a.matchPos == len(a.matches) {
			a.matchPos = -1
			a.setCommand(a.stem)
			return
		}
		word, _ := lastWord(a.stem)
		a.setCommand(a.stem[:len(a.stem)-len(word)] + a.matches[a.matchPos] + " ")
		return
	}
	a.stem = string(a.command.text)
	word, before := lastWord(a.stem)
	base := a.stem[:len(a.stem)-len(word)]
	var found []string
	for _, c := range candidates(before) {
		if strings.HasPrefix(c, word) {
			found = append(found, c)
		}
	}
	slices.Sort(found)
	switch len(found) {
	case 0:
	case 1:
		a.setCommand(base + found[0] + " ")
	default:
		shared := found[0]
		for _, c := range found[1:] {
			shared = commonPrefix(shared, c)
		}
		a.matches, a.matchPos = found, -1
		a.setCommand(base + shared)
	}
}

// setCommand is what a Tab leaves on the prompt.
func (a *App) setCommand(text string) {
	a.command.text = append(a.command.text[:0], []rune(text)...)
	a.filled = text
}

// lastWord splits a prompt into the word being typed, "" after a
// space, and the words before it.
func lastWord(text string) (word string, before []string) {
	before = strings.Fields(text)
	if len(before) > 0 && !strings.HasSuffix(text, " ") {
		word, before = before[len(before)-1], before[:len(before)-1]
	}
	return word, before
}

// candidates are the words that may come after before, the words
// before the one being completed: a command first, then its argument.
func candidates(before []string) []string {
	switch {
	case len(before) == 0:
		return commands
	case len(before) == 1 && before[0] == "cycle":
		return input.SettingNames()
	case len(before) == 1:
		if s, ok := input.LookupSetting(before[0]); ok {
			return s.Values
		}
	case len(before) == 2 && before[0] == "map":
		return input.CommandNames()
	case len(before) == 3 && before[0] == "map" && before[2] == "cycle":
		return input.SettingNames()
	}
	return nil
}

func commonPrefix(a, b string) string {
	i := 0
	for i < len(a) && i < len(b) && a[i] == b[i] {
		i++
	}
	return a[:i]
}

// commands is what the : prompt takes: goto, cycle, help, reload,
// quit and the config verbs but clear_all_shortcuts, which would
// unbind : and q.
var commands = []string{"goto", "cycle", "help", "reload", "quit", "wrap", "wrap_style", "tab_width", "edge_markers", "file_separators", "auto_reload", "status_line", "view_style", "clipboard_cmd", "map"}

// runCommand runs a : line. A number alone, or goto and a number, goes
// to that 1-based line, clamped to the buffer; cycle steps a setting;
// help, reload and quit do what their keys do; a config verb sets
// what it would in a file, for the rest of the run. Nothing typed does
// nothing; an error is reported on the status line. It reports
// whether the line quit.
func (a *App) runCommand(line string) bool {
	f := strings.Fields(line)
	if len(f) == 0 {
		return false
	}
	if len(f) == 1 && isNumber(f[0]) {
		f = []string{"goto", f[0]}
	}
	switch f[0] {
	case "goto":
		if len(f) != 2 || !isNumber(f[1]) {
			a.status = "goto takes a line number"
			return false
		}
		n, _ := strconv.Atoi(f[1])
		_, start, end := a.input()
		a.cur = buffer.Pos{Line: max(start, min(start+n-1, end-1))}
		a.drop()
		a.scrollToCursor()
		return false
	case "cycle":
		c, err := input.ParseCommand(line)
		if err != nil {
			a.status = err.Error()
			return false
		}
		a.cycle(c.Arg)
		return false
	case "help", "reload", "quit":
		if len(f) != 1 {
			a.status = f[0] + " takes no argument"
			return false
		}
		switch f[0] {
		case "help":
			a.showHelp()
		case "reload":
			a.reload(true)
		}
		return f[0] == "quit"
	}
	var c config.Config
	verb, err := config.ParseLine(line, &c)
	if !slices.Contains(commands, verb) {
		a.status = fmt.Sprintf("unknown command %q", verb)
		return false
	}
	if err != nil {
		a.status = err.Error()
		return false
	}
	switch verb {
	case "wrap":
		mode := layout.Wrap
		if c.NoWrap {
			mode = layout.NoWrap
		}
		a.setMode(mode)
	case "wrap_style":
		a.style = c.WrapStyle
	case "tab_width":
		a.tab = c.TabWidth
	case "edge_markers":
		a.marks = c.EdgeMarkers
	case "file_separators":
		a.seps = !c.NoFileSeparators
	case "auto_reload":
		a.auto = !c.NoAutoReload
		if !a.auto {
			a.dirty = false // a change during a reload asked for another
		}
	case "status_line":
		a.hide = c.NoStatusLine
	case "view_style":
		a.view = c.ViewStyle
	case "clipboard_cmd":
		a.copier = clipboard.Command{Cmd: c.ClipboardCmd}
	case "map":
		a.keys = a.keys.Apply(c.Keys)
	}
	a.scrollToCursor()
	return false
}

// cycle sets a setting to the value after its current one, the first
// after the last, and reports the new value on the status line unless
// the line shows it anyway, as it does wrap, wrap_style and
// view_style, or is the setting.
func (a *App) cycle(name string) {
	s, _ := input.LookupSetting(name)
	line := name + " " + s.Next(a.setting(name))
	a.runCommand(line)
	if name != "wrap" && name != "wrap_style" && name != "status_line" && name != "view_style" {
		a.status = line
	}
}

// setting is the current value of a setting cycle takes, as a config
// file spells it.
func (a *App) setting(name string) string {
	switch name {
	case "wrap":
		return yesNo(a.mode == layout.Wrap)
	case "wrap_style":
		if a.style == layout.WrapStyleWord {
			return "word"
		}
		return "char"
	case "edge_markers":
		return yesNo(a.marks)
	case "file_separators":
		return yesNo(a.seps)
	case "auto_reload":
		return yesNo(a.auto)
	case "status_line":
		return yesNo(!a.hide)
	case "view_style":
		if a.view == ansi.ViewPlain {
			return "plain"
		}
		return "color"
	}
	return ""
}

func yesNo(b bool) string {
	if b {
		return "yes"
	}
	return "no"
}

func isNumber(s string) bool {
	for _, r := range s {
		if r < '0' || r > '9' {
			return false
		}
	}
	return s != ""
}
