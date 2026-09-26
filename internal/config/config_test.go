package config

import (
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"github.com/gdamore/tcell/v2"
	"go.dlh.dev/vedi/internal/input"
)

func key(name string) input.Key {
	k, err := input.ParseKey(name)
	if err != nil {
		panic(err)
	}
	return k
}

func TestParse(t *testing.T) {
	src := "# vim\n\nmap h left\n\tmap  L\tselect_right \nmap ? none\r\n"
	c, err := Parse("vedi.conf", []byte(src))
	if err != nil {
		t.Fatal(err)
	}
	got := c.Keys
	want := []input.Binding{
		{Key: input.Key{Key: tcell.KeyRune, Rune: 'h', Mod: 0}, Cmd: input.Command{Action: input.Left}},
		{Key: input.Key{Key: tcell.KeyRune, Rune: 'L', Mod: 0}, Cmd: input.Command{Action: input.Right, Extend: true}},
		{Key: input.Key{Key: tcell.KeyRune, Rune: '?', Mod: 0}, Cmd: input.Command{}},
	}
	if len(got) != len(want) {
		t.Fatalf("Parse = %+v, want %+v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Errorf("binding %d = %+v, want %+v", i, got[i], want[i])
		}
	}
	if c, err := Parse("empty", nil); err != nil || len(c.Keys) != 0 {
		t.Errorf("Parse(empty) = %v, %v", c, err)
	}
	bad := []struct{ src, err string }{
		{"map h left\nbind j down\n", `vedi.conf:2: unknown verb "bind"`},
		{"clear_all_shortcuts yes\n", "vedi.conf:1: clear_all_shortcuts takes nothing"},
		{"map h\n", "vedi.conf:1: map takes a key and an action"},
		{"map h left now\n", "vedi.conf:1: map takes a key and an action"},
		{"\n\nmap F1 help\n", `vedi.conf:3: unknown key "F1"`},
		{"map h foo\n", `vedi.conf:1: unknown action "foo"`},
		{"map Shift+h left\n", `vedi.conf:1: "Shift+h": modifiers on a character key`},
		{"map h select_copy\n", `vedi.conf:1: unknown action "select_copy"`},
	}
	for _, tc := range bad {
		if _, err := Parse("vedi.conf", []byte(tc.src)); err == nil || err.Error() != tc.err {
			t.Errorf("Parse(%q) err = %v, want %s", tc.src, err, tc.err)
		}
	}
}

// TestClearAllShortcuts: clear_all_shortcuts drops what was bound
// before it, the defaults included; map lines after it build the map
// from nothing.
func TestClearAllShortcuts(t *testing.T) {
	c, err := Parse("vedi.conf", []byte("map j quit\nclear_all_shortcuts\nmap x quit\n"))
	if err != nil {
		t.Fatal(err)
	}
	want := []input.Binding{{Key: key("x"), Cmd: input.Command{Action: input.Quit}}}
	if !c.Clear || !reflect.DeepEqual(c.Keys, want) {
		t.Errorf("Parse = %+v, want Clear and %+v", c, want)
	}
	if got := c.Keymap(true); !reflect.DeepEqual(got, input.Keymap(want)) {
		t.Errorf("Keymap = %+v, want %+v", got, want)
	}
	c, err = Parse("vedi.conf", []byte("map x quit\n"))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := c.Keymap(false), Default(false).Apply(c.Keys); !reflect.DeepEqual(got, want) {
		t.Errorf("Keymap without clear = %+v, want %+v", got, want)
	}
}

func TestDefaultOS(t *testing.T) {
	mac, other := Default(true), Default(false)
	var stripped input.Keymap
	for _, b := range mac {
		if b.Key.Mod&tcell.ModMeta == 0 {
			stripped = append(stripped, b)
		}
	}
	if len(stripped) == len(mac) {
		t.Fatal("macOS map has no ⌘ keys")
	}
	if len(stripped) != len(other) {
		t.Fatalf("macOS has %d bindings without ⌘, elsewhere %d", len(stripped), len(other))
	}
	for i := range other {
		if stripped[i] != other[i] {
			t.Errorf("binding %d: %+v vs %+v", i, stripped[i], other[i])
		}
	}
	for i, b := range mac {
		for _, o := range mac[i+1:] {
			if b.Key == o.Key {
				t.Errorf("%s bound twice", b.Key)
			}
		}
	}
}

// TestLookup covers what Lookup adds to Parse: the event side of
// Normalize. The defaults themselves are covered by TestDefaultOS and
// the specs.
func TestLookup(t *testing.T) {
	k := func(key tcell.Key, r rune, mod tcell.ModMask) *tcell.EventKey {
		return tcell.NewEventKey(key, r, mod)
	}
	tests := []struct {
		name  string
		macOS bool
		ev    *tcell.EventKey
		want  input.Command
	}{
		{"up", false, k(tcell.KeyUp, 0, 0), input.Command{Action: input.Up}},
		{"shift up extends", false, k(tcell.KeyUp, 0, tcell.ModShift), input.Command{Action: input.Up, Extend: true}},
		{"ctrl on a named key", false, k(tcell.KeyLeft, 0, tcell.ModCtrl), input.Command{Action: input.WordLeft}},
		{"alt on a rune", false, k(tcell.KeyRune, 'b', tcell.ModAlt), input.Command{Action: input.WordLeft}},
		{"ctrl f", false, k(tcell.KeyCtrlF, 0, tcell.ModCtrl), input.Command{Action: input.PageDown}},
		{"ctrl f without mod", false, k(tcell.KeyCtrlF, 0, 0), input.Command{Action: input.PageDown}},
		{"shift on a character", false, k(tcell.KeyRune, '<', tcell.ModShift), input.Command{Action: input.First}},
		{"G never extends", false, k(tcell.KeyRune, 'G', tcell.ModShift), input.Command{Action: input.Last}},
		{"enter", false, k(tcell.KeyEnter, 0, 0), input.Command{Action: input.CopyAndQuit}},
		{"unbound rune", false, k(tcell.KeyRune, 'z', 0), input.Command{}},
		{"unbound key", false, k(tcell.KeyF1, 0, 0), input.Command{}},
		{"unlisted modifier is unbound", false, k(tcell.KeyHome, 0, tcell.ModCtrl), input.Command{}},
		{"cmd c", true, k(tcell.KeyRune, 'c', tcell.ModMeta), input.Command{Action: input.Copy}},
		{"cmd shift G as a capital", true, k(tcell.KeyRune, 'G', tcell.ModMeta|tcell.ModShift), input.Command{Action: input.SearchPrev}},
		{"cmd shift up extends", true, k(tcell.KeyUp, 0, tcell.ModMeta|tcell.ModShift), input.Command{Action: input.First, Extend: true}},
		{"cmd q is unbound", true, k(tcell.KeyRune, 'q', tcell.ModMeta), input.Command{}},
		{"cmd c elsewhere", false, k(tcell.KeyRune, 'c', tcell.ModMeta), input.Command{}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			if got := Default(tc.macOS).Lookup(tc.ev); got != tc.want {
				t.Fatalf("Lookup = %+v, want %+v", got, tc.want)
			}
		})
	}
}

func TestLookupApplied(t *testing.T) {
	m := Default(false).Apply([]input.Binding{
		{Key: key("h"), Cmd: input.Command{Action: input.Left}},
		{Key: key("L"), Cmd: input.Command{Action: input.Right, Extend: true}},
		{Key: key("Alt+<"), Cmd: input.Command{Action: input.First}},
	})
	// The kitty protocol reports Alt+< as , with Shift and Alt.
	if got := m.Lookup(tcell.NewEventKey(tcell.KeyRune, ',', tcell.ModShift|tcell.ModAlt)); got != (input.Command{Action: input.First}) {
		t.Errorf("kitty Alt+< = %+v", got)
	}
	if got := m.Lookup(key("h").Event()); got != (input.Command{Action: input.Left}) {
		t.Errorf("h = %+v", got)
	}
	if got := m.Lookup(tcell.NewEventKey(tcell.KeyRune, 'L', tcell.ModShift)); got != (input.Command{Action: input.Right, Extend: true}) {
		t.Errorf("L = %+v", got)
	}
}

func BenchmarkDefault(b *testing.B) {
	for b.Loop() {
		Default(true)
	}
}

func BenchmarkLookup(b *testing.B) {
	m := Default(true)
	ev := tcell.NewEventKey(tcell.KeyRune, 'h', 0)
	b.ReportAllocs()
	for b.Loop() {
		m.Lookup(ev)
	}
}

func TestPath(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", "/xdg")
	if got := Path(); got != "/xdg/vedi/vedi.conf" {
		t.Errorf("Path = %q", got)
	}
	t.Setenv("XDG_CONFIG_HOME", "")
	t.Setenv("HOME", "/home/u")
	if got := Path(); got != "/home/u/.config/vedi/vedi.conf" {
		t.Errorf("Path = %q", got)
	}
}

func TestLoad(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "vedi.conf")
	c, err := Load(path)
	if !errors.Is(err, fs.ErrNotExist) || !reflect.DeepEqual(c, Config{}) {
		t.Errorf("missing file: %+v, %v", c, err)
	}
	if c, err := Load(""); err != nil || !reflect.DeepEqual(c, Config{}) {
		t.Errorf("no path: %+v, %v", c, err)
	}
	os.WriteFile(path, []byte("map h left\n"), 0o644)
	c, err = Load(path)
	if err != nil {
		t.Fatal(err)
	}
	if got := Default(false).Apply(c.Keys).Lookup(tcell.NewEventKey(tcell.KeyRune, 'h', 0)); got != (input.Command{Action: input.Left}) {
		t.Errorf("h = %+v", got)
	}
	os.WriteFile(path, []byte("map h left\n\nmap x foo\n"), 0o644)
	if _, err := Load(path); err == nil || err.Error() != path+`:3: unknown action "foo"` {
		t.Errorf("bad file err = %v", err)
	}
	if _, err := Load(dir); err == nil {
		t.Error("a directory reads without error")
	}
}

// TestAutoReload: auto_reload no turns off reading a changed file
// again; yes, the default, turns it back on; anything else is an
// error.
func TestAutoReload(t *testing.T) {
	for _, tc := range []struct {
		src  string
		want bool
	}{
		{"", false},
		{"auto_reload no\n", true},
		{"auto_reload yes\n", false},
		{"auto_reload no\nauto_reload yes\n", false},
		{"auto_reload yes\nmap x quit\nauto_reload no\n", true},
	} {
		c, err := Parse("vedi.conf", []byte(tc.src))
		if err != nil || c.NoAutoReload != tc.want {
			t.Errorf("Parse(%q) = %+v, %v; want NoAutoReload %v", tc.src, c, err, tc.want)
		}
	}
	bad := []struct{ src, err string }{
		{"auto_reload\n", "vedi.conf:1: auto_reload takes yes or no"},
		{"auto_reload maybe\n", "vedi.conf:1: auto_reload takes yes or no"},
		{"auto_reload no yes\n", "vedi.conf:1: auto_reload takes yes or no"},
	}
	for _, tc := range bad {
		if _, err := Parse("vedi.conf", []byte(tc.src)); err == nil || err.Error() != tc.err {
			t.Errorf("Parse(%q) err = %v, want %s", tc.src, err, tc.err)
		}
	}
}
