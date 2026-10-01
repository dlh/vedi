package input

import (
	"slices"

	"github.com/gdamore/tcell/v3"
)

// Binding is a key and what it does; a zero Cmd unbinds.
type Binding struct {
	Key Key
	Cmd Command
}

// Keymap is the bindings in effect, in order: the help screen lists
// keys in this order.
type Keymap []Binding

// Apply lays bindings over a copy of the map: a bound key is replaced
// in place, a new one appended, and none removes it.
func (m Keymap) Apply(bs []Binding) Keymap {
	m = slices.Clone(m)
	for _, b := range bs {
		i := slices.IndexFunc(m, func(x Binding) bool { return x.Key == b.Key })
		switch {
		case b.Cmd == (Command{}) && i >= 0:
			m = slices.Delete(m, i, i+1)
		case b.Cmd == (Command{}):
		case i >= 0:
			m[i] = b
		default:
			m = append(m, b)
		}
	}
	return m
}

// Lookup is what the key does; zero when unbound.
func (m Keymap) Lookup(ev *tcell.EventKey) Command {
	k := Normalize(ev)
	for _, b := range m {
		if b.Key == k {
			return b.Cmd
		}
	}
	return Command{}
}

// Find is the first key bound to c.
func (m Keymap) Find(c Command) (Key, bool) {
	for _, b := range m {
		if b.Cmd == c {
			return b.Key, true
		}
	}
	return Key{}, false
}
