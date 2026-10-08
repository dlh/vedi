// Package config reads vedi.conf: one directive per line, kitty style.
package config

import (
	_ "embed"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strconv"
	"strings"

	"github.com/gdamore/tcell/v3"
	"go.dlh.dev/vedi/internal/ansi"
	"go.dlh.dev/vedi/internal/input"
	"go.dlh.dev/vedi/internal/layout"
	"go.dlh.dev/vedi/internal/words"
)

// Config is what a file sets.
type Config struct {
	Keys             []input.Binding  // map lines, in order
	Clear            bool             // clear_all_shortcuts: the map starts empty
	NoAutoReload     bool             // auto_reload no: a changed file is not read again
	NoWrap           bool             // wrap no: start in nowrap mode
	WrapStyle        layout.WrapStyle // wrap_style word: wrap mode breaks rows at words
	ViewStyle        ansi.ViewStyle   // view_style plain: the text is drawn without its colors; raw: with its escapes as text
	ClipboardCmd     string           // clipboard_cmd: copy pipes to this, not OSC 52
	OpenCmds         []OpenCmd        // open_cmd lines, in order
	TabWidth         int              // tab_width: cells per tab stop; 0 for the default
	EdgeMarkers      bool             // edge_markers yes: mark text off the sides, and wrapped rows
	NoFileSeparators bool             // file_separators no: no row naming each input
	NoStatusLine     bool             // status_line no: the status line shows only when it has something to say
	OnePageRowsBelow int              // one_page_rows_below: rows -F leaves below the text; 0 for the default
}

// An OpenCmd is an open_cmd line: files whose base name matches
// Pattern are read by running Argv, %s the name; Pattern "" is for
// the files no pattern matches.
type OpenCmd struct {
	Pattern string
	Argv    []string
}

// OpenCmdFor is the command name is read through: the last pattern
// line matching its base name, ignoring case, else the last plain
// line, else nil to read it as it is.
func (c Config) OpenCmdFor(name string) []string {
	var plain []string
	base := strings.ToLower(filepath.Base(name))
	for _, o := range slices.Backward(c.OpenCmds) {
		if o.Pattern == "" {
			if plain == nil {
				plain = o.Argv
			}
			continue
		}
		if ok, _ := filepath.Match(o.Pattern, base); ok {
			return o.Argv
		}
	}
	return plain
}

// Parse applies src line by line. Errors read name:line: message.
func Parse(name string, src []byte) (Config, error) {
	var c Config
	for i, line := range strings.Split(string(src), "\n") {
		if _, err := ParseLine(line, &c); err != nil {
			return c, fmt.Errorf("%s:%d: %v", name, i+1, err)
		}
	}
	return c, nil
}

// ParseLine applies one line of a config to c and names its verb: ""
// for a blank line or a comment. The error has no file position.
func ParseLine(line string, c *Config) (verb string, err error) {
	f := strings.Fields(line)
	if len(f) == 0 || strings.HasPrefix(f[0], "#") {
		return "", nil
	}
	verb = f[0]
	switch verb {
	case "map":
		if len(f) != 3 && !(len(f) == 4 && f[2] == "cycle") {
			return verb, errors.New("map takes a key and an action")
		}
		k, err := input.ParseKey(f[1])
		if err != nil {
			return verb, err
		}
		cmd, err := input.ParseCommand(strings.Join(f[2:], " "))
		if err != nil {
			return verb, err
		}
		c.Keys = append(c.Keys, input.Binding{Key: k, Cmd: cmd})
	case "clear_all_shortcuts":
		if len(f) != 1 {
			return verb, errors.New("clear_all_shortcuts takes nothing")
		}
		c.Keys, c.Clear = nil, true
	case "auto_reload":
		if len(f) != 2 || (f[1] != "yes" && f[1] != "no") {
			return verb, errors.New("auto_reload takes yes or no")
		}
		c.NoAutoReload = f[1] == "no"
	case "wrap":
		if len(f) != 2 || (f[1] != "yes" && f[1] != "no") {
			return verb, errors.New("wrap takes yes or no")
		}
		c.NoWrap = f[1] == "no"
	case "wrap_style":
		if len(f) != 2 || (f[1] != "char" && f[1] != "word") {
			return verb, errors.New("wrap_style takes char or word")
		}
		c.WrapStyle = layout.WrapStyleChar
		if f[1] == "word" {
			c.WrapStyle = layout.WrapStyleWord
		}
	case "view_style":
		if len(f) != 2 {
			return verb, errors.New("view_style takes color, plain or raw")
		}
		switch f[1] {
		case "color":
			c.ViewStyle = ansi.ViewColor
		case "plain":
			c.ViewStyle = ansi.ViewPlain
		case "raw":
			c.ViewStyle = ansi.ViewRaw
		default:
			return verb, errors.New("view_style takes color, plain or raw")
		}
	case "clipboard_cmd":
		if len(f) < 2 {
			return verb, errors.New("clipboard_cmd takes a command")
		}
		c.ClipboardCmd = rest(line, f[0])
	case "open_cmd":
		o, err := parseOpenCmd(rest(line, f[0]))
		if err != nil {
			return verb, err
		}
		c.OpenCmds = append(c.OpenCmds, o)
	case "tab_width":
		if len(f) != 2 {
			return verb, errors.New("tab_width takes a positive number")
		}
		n, err := strconv.Atoi(f[1])
		if err != nil || n < 1 {
			return verb, errors.New("tab_width takes a positive number")
		}
		c.TabWidth = n
	case "one_page_rows_below":
		if len(f) != 2 {
			return verb, errors.New("one_page_rows_below takes a positive number")
		}
		n, err := strconv.Atoi(f[1])
		if err != nil || n < 1 {
			return verb, errors.New("one_page_rows_below takes a positive number")
		}
		c.OnePageRowsBelow = n
	case "edge_markers":
		if len(f) != 2 || (f[1] != "yes" && f[1] != "no") {
			return verb, errors.New("edge_markers takes yes or no")
		}
		c.EdgeMarkers = f[1] == "yes"
	case "file_separators":
		if len(f) != 2 || (f[1] != "yes" && f[1] != "no") {
			return verb, errors.New("file_separators takes yes or no")
		}
		c.NoFileSeparators = f[1] == "no"
	case "status_line":
		if len(f) != 2 || (f[1] != "yes" && f[1] != "no") {
			return verb, errors.New("status_line takes yes or no")
		}
		c.NoStatusLine = f[1] == "no"
	default:
		return verb, fmt.Errorf("unknown verb %q", verb)
	}
	return verb, nil
}

// parseOpenCmd reads what follows open_cmd: a pattern, when the
// first word has a glob character, then the command.
func parseOpenCmd(s string) (OpenCmd, error) {
	var o OpenCmd
	argv, err := words.Split(s)
	if err != nil {
		return o, fmt.Errorf("open_cmd: %v", err)
	}
	if len(argv) > 0 && strings.ContainsAny(argv[0], "*?[") {
		o.Pattern = strings.ToLower(argv[0])
		if _, err := filepath.Match(o.Pattern, ""); err != nil {
			return o, fmt.Errorf("open_cmd: bad pattern %s", argv[0])
		}
		argv = argv[1:]
	}
	if len(argv) == 0 {
		return o, errors.New("open_cmd takes a command")
	}
	o.Argv = argv
	return o, nil
}

func rest(line, verb string) string {
	return strings.TrimSpace(strings.TrimSpace(line)[len(verb):])
}

// Keymap is the bindings in effect: the defaults, or none after
// clear_all_shortcuts, with the map lines laid over.
func (c Config) Keymap(macOS bool) input.Keymap {
	if c.Clear {
		// Empty, not nil: nil asks the app for the defaults.
		return input.Keymap{}.Apply(c.Keys)
	}
	return Default(macOS).Apply(c.Keys)
}

//go:embed defaults.conf
var defaults []byte

// Default is the built-in key map; off macOS the ⌘ lines are left
// out.
func Default(macOS bool) input.Keymap {
	c, err := Parse("defaults.conf", defaults)
	if err != nil {
		panic(err)
	}
	m := make(input.Keymap, 0, len(c.Keys))
	for _, b := range c.Keys {
		if macOS || b.Key.Mod&tcell.ModMeta == 0 {
			m = append(m, b)
		}
	}
	return m
}

// Path is where the file is looked for: $XDG_CONFIG_HOME/vedi/vedi.conf,
// else ~/.config/vedi/vedi.conf. Empty when there is no home directory.
func Path() string {
	dir := os.Getenv("XDG_CONFIG_HOME")
	if dir == "" {
		home, err := os.UserHomeDir()
		if err != nil {
			return ""
		}
		dir = filepath.Join(home, ".config")
	}
	return filepath.Join(dir, "vedi", "vedi.conf")
}

// Load reads and parses the file at path; no path is an empty Config.
// A missing file is an fs.ErrNotExist, for the caller to forgive or
// not; a bad line names the file and line.
func Load(path string) (Config, error) {
	if path == "" {
		return Config{}, nil
	}
	src, err := os.ReadFile(path)
	if err != nil {
		return Config{}, err
	}
	return Parse(path, src)
}
