// Package config reads vedi.conf: one directive per line, kitty style.
package config

import (
	_ "embed"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/gdamore/tcell/v3"
	"go.dlh.dev/vedi/internal/input"
	"go.dlh.dev/vedi/internal/layout"
)

// Config is what a file sets.
type Config struct {
	Keys         []input.Binding  // map lines, in order
	Clear        bool             // clear_all_shortcuts: the map starts empty
	NoAutoReload bool             // auto_reload no: a changed file is not read again
	NoWrap       bool             // wrap no: start in nowrap mode
	WrapStyle    layout.WrapStyle // wrap_style word: wrap mode breaks rows at words
	ClipboardCmd string           // clipboard_cmd: copy pipes to this, not OSC 52
	TabWidth     int              // tab_width: cells per tab stop; 0 for the default
	EdgeMarkers  bool             // edge_markers yes: nowrap marks text off the sides
}

// Parse reads a config: "map <key> <action>" lines, clear_all_shortcuts,
// "auto_reload yes|no", "wrap yes|no", "wrap_style char|word", "clipboard_cmd <command>",
// "tab_width <n>", "edge_markers yes|no", blank lines and # comments. Errors read name:line:
// message.
func Parse(name string, src []byte) (Config, error) {
	var c Config
	for i, line := range strings.Split(string(src), "\n") {
		f := strings.Fields(line)
		if len(f) == 0 || strings.HasPrefix(f[0], "#") {
			continue
		}
		fail := func(format string, args ...any) error {
			return fmt.Errorf("%s:%d: %s", name, i+1, fmt.Sprintf(format, args...))
		}
		switch f[0] {
		case "map":
			if len(f) != 3 {
				return c, fail("map takes a key and an action")
			}
			k, err := input.ParseKey(f[1])
			if err != nil {
				return c, fail("%v", err)
			}
			cmd, err := input.ParseCommand(f[2])
			if err != nil {
				return c, fail("%v", err)
			}
			c.Keys = append(c.Keys, input.Binding{Key: k, Cmd: cmd})
		case "clear_all_shortcuts":
			if len(f) != 1 {
				return c, fail("clear_all_shortcuts takes nothing")
			}
			c.Keys, c.Clear = nil, true
		case "auto_reload":
			if len(f) != 2 || (f[1] != "yes" && f[1] != "no") {
				return c, fail("auto_reload takes yes or no")
			}
			c.NoAutoReload = f[1] == "no"
		case "wrap":
			if len(f) != 2 || (f[1] != "yes" && f[1] != "no") {
				return c, fail("wrap takes yes or no")
			}
			c.NoWrap = f[1] == "no"
		case "wrap_style":
			if len(f) != 2 || (f[1] != "char" && f[1] != "word") {
				return c, fail("wrap_style takes char or word")
			}
			c.WrapStyle = layout.WrapStyleChar
			if f[1] == "word" {
				c.WrapStyle = layout.WrapStyleWord
			}
		case "clipboard_cmd":
			if len(f) < 2 {
				return c, fail("clipboard_cmd takes a command")
			}
			c.ClipboardCmd = strings.TrimSpace(strings.TrimSpace(line)[len(f[0]):])
		case "tab_width":
			if len(f) != 2 {
				return c, fail("tab_width takes a positive number")
			}
			n, err := strconv.Atoi(f[1])
			if err != nil || n < 1 {
				return c, fail("tab_width takes a positive number")
			}
			c.TabWidth = n
		case "edge_markers":
			if len(f) != 2 || (f[1] != "yes" && f[1] != "no") {
				return c, fail("edge_markers takes yes or no")
			}
			c.EdgeMarkers = f[1] == "yes"
		default:
			return c, fail("unknown verb %q", f[0])
		}
	}
	return c, nil
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
