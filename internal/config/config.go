// Package config reads vedi.conf: one directive per line, kitty style.
package config

import (
	_ "embed"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/gdamore/tcell/v2"
	"go.dlh.dev/vedi/internal/input"
)

// Config is what a file sets.
type Config struct {
	Keys  []input.Binding // map lines, in order
	Clear bool            // clear_all_shortcuts: the map starts empty
}

// Parse reads a config: "map <key> <action>" lines, clear_all_shortcuts,
// blank lines and # comments. Errors read name:line: message.
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
		return input.Keymap(nil).Apply(c.Keys)
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
