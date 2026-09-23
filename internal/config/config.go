// Package config reads vedi.conf: one directive per line, kitty style.
package config

import (
	_ "embed"
	"fmt"
	"strings"

	"github.com/gdamore/tcell/v2"
	"go.dlh.dev/vedi/internal/input"
)

// Config is what a file sets.
type Config struct {
	Keys []input.Binding // map lines, in order
}

// Parse reads a config: "map <key> <action>" lines, blank lines and #
// comments. Errors read name:line: message.
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
		default:
			return c, fail("unknown verb %q", f[0])
		}
	}
	return c, nil
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
