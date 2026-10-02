package app

import (
	"fmt"
	"slices"
	"strconv"
	"strings"

	"github.com/gdamore/tcell/v3"
	"go.dlh.dev/vedi/internal/buffer"
	"go.dlh.dev/vedi/internal/clipboard"
	"go.dlh.dev/vedi/internal/config"
	"go.dlh.dev/vedi/internal/layout"
)

// handleCommandKey edits the : prompt. Enter runs what was typed and
// closes the prompt; Esc just closes it.
func (a *App) handleCommandKey(ev *tcell.EventKey) {
	switch ev.Key() {
	case tcell.KeyEscape:
		a.commanding = false
	case tcell.KeyEnter:
		a.commanding = false
		if len(a.command.text) > 0 {
			a.command.remember()
		}
		a.runCommand(string(a.command.text))
	default:
		a.command.edit(ev)
	}
}

// commands is what the : prompt takes: goto and the config verbs but
// clear_all_shortcuts, which would unbind : and q.
var commands = []string{"goto", "wrap", "wrap_style", "tab_width", "edge_markers", "auto_reload", "clipboard_cmd", "map"}

// runCommand runs a : line. A number alone, or goto and a number, goes
// to that 1-based line, clamped to the buffer; a config verb sets
// what it would in a file, for the rest of the run. Nothing typed does
// nothing; an error is reported on the status line.
func (a *App) runCommand(line string) {
	f := strings.Fields(line)
	if len(f) == 0 {
		return
	}
	if len(f) == 1 && isNumber(f[0]) {
		f = []string{"goto", f[0]}
	}
	if f[0] == "goto" {
		if len(f) != 2 || !isNumber(f[1]) {
			a.status = "goto takes a line number"
			return
		}
		n, _ := strconv.Atoi(f[1])
		a.cur = buffer.Pos{Line: n - 1}
		a.drop()
		a.scrollToCursor()
		return
	}
	var c config.Config
	verb, err := config.ParseLine(line, &c)
	if !slices.Contains(commands, verb) {
		a.status = fmt.Sprintf("unknown command %q", verb)
		return
	}
	if err != nil {
		a.status = err.Error()
		return
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
	case "auto_reload":
		a.auto = !c.NoAutoReload
	case "clipboard_cmd":
		a.copier = clipboard.Command{Cmd: c.ClipboardCmd}
	case "map":
		a.keys = a.keys.Apply(c.Keys)
	}
	a.scrollToCursor()
}

func isNumber(s string) bool {
	for _, r := range s {
		if r < '0' || r > '9' {
			return false
		}
	}
	return s != ""
}
