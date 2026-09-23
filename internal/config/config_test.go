package config

import (
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

func TestLookup(t *testing.T) {
	k := func(key tcell.Key, r rune, mod tcell.ModMask) *tcell.EventKey {
		return tcell.NewEventKey(key, r, mod)
	}
	tests := []struct {
		name string
		ev   *tcell.EventKey
		want input.Command
	}{
		{"up", k(tcell.KeyUp, 0, 0), input.Command{Action: input.Up, Extend: false}},
		{"shift up", k(tcell.KeyUp, 0, tcell.ModShift), input.Command{Action: input.Up, Extend: true}},
		{"shift down", k(tcell.KeyDown, 0, tcell.ModShift), input.Command{Action: input.Down, Extend: true}},
		{"left", k(tcell.KeyLeft, 0, 0), input.Command{Action: input.Left, Extend: false}},
		{"ctrl left", k(tcell.KeyLeft, 0, tcell.ModCtrl), input.Command{Action: input.WordLeft, Extend: false}},
		{"ctrl shift right", k(tcell.KeyRight, 0, tcell.ModCtrl|tcell.ModShift), input.Command{Action: input.WordRight, Extend: true}},
		{"alt left", k(tcell.KeyLeft, 0, tcell.ModAlt), input.Command{Action: input.WordLeft, Extend: false}},
		{"alt shift right", k(tcell.KeyRight, 0, tcell.ModAlt|tcell.ModShift), input.Command{Action: input.WordRight, Extend: true}},
		{"alt b", k(tcell.KeyRune, 'b', tcell.ModAlt), input.Command{Action: input.WordLeft, Extend: false}},
		{"alt f", k(tcell.KeyRune, 'f', tcell.ModAlt), input.Command{Action: input.WordRight, Extend: false}},
		{"shift home", k(tcell.KeyHome, 0, tcell.ModShift), input.Command{Action: input.Home, Extend: true}},
		{"end", k(tcell.KeyEnd, 0, 0), input.Command{Action: input.End, Extend: false}},
		{"shift pgup", k(tcell.KeyPgUp, 0, tcell.ModShift), input.Command{Action: input.PageUp, Extend: true}},
		{"pgdn", k(tcell.KeyPgDn, 0, 0), input.Command{Action: input.PageDown, Extend: false}},
		{"space", k(tcell.KeyRune, ' ', 0), input.Command{Action: input.PageDown, Extend: false}},
		{"b", k(tcell.KeyRune, 'b', 0), input.Command{Action: input.PageUp, Extend: false}},
		{"j", k(tcell.KeyRune, 'j', 0), input.Command{Action: input.Down, Extend: false}},
		{"k", k(tcell.KeyRune, 'k', 0), input.Command{Action: input.Up, Extend: false}},
		{"f", k(tcell.KeyRune, 'f', 0), input.Command{Action: input.PageDown, Extend: false}},
		{"ctrl f", k(tcell.KeyCtrlF, 0, tcell.ModCtrl), input.Command{Action: input.PageDown, Extend: false}},
		{"ctrl b", k(tcell.KeyCtrlB, 0, tcell.ModCtrl), input.Command{Action: input.PageUp, Extend: false}},
		{"ctrl v", k(tcell.KeyCtrlV, 0, tcell.ModCtrl), input.Command{Action: input.PageDown, Extend: false}},
		{"alt v", k(tcell.KeyRune, 'v', tcell.ModAlt), input.Command{Action: input.PageUp, Extend: false}},
		{"d", k(tcell.KeyRune, 'd', 0), input.Command{Action: input.HalfPageDown, Extend: false}},
		{"ctrl d", k(tcell.KeyCtrlD, 0, tcell.ModCtrl), input.Command{Action: input.HalfPageDown, Extend: false}},
		{"u", k(tcell.KeyRune, 'u', 0), input.Command{Action: input.HalfPageUp, Extend: false}},
		{"ctrl u", k(tcell.KeyCtrlU, 0, tcell.ModCtrl), input.Command{Action: input.HalfPageUp, Extend: false}},
		{"g", k(tcell.KeyRune, 'g', 0), input.Command{Action: input.First, Extend: false}},
		{"<", k(tcell.KeyRune, '<', tcell.ModShift), input.Command{Action: input.First, Extend: false}},
		{">", k(tcell.KeyRune, '>', tcell.ModShift), input.Command{Action: input.Last, Extend: false}},
		{"G never extends", k(tcell.KeyRune, 'G', tcell.ModShift), input.Command{Action: input.Last, Extend: false}},
		{"ctrl a", k(tcell.KeyCtrlA, 0, tcell.ModCtrl), input.Command{Action: input.SelectAll, Extend: false}},
		{"esc", k(tcell.KeyEscape, 0, 0), input.Command{Action: input.ClearSelection, Extend: false}},
		{"ctrl c", k(tcell.KeyCtrlC, 0, tcell.ModCtrl), input.Command{Action: input.Copy, Extend: false}},
		{"y", k(tcell.KeyRune, 'y', 0), input.Command{Action: input.Copy, Extend: false}},
		{"enter", k(tcell.KeyEnter, 0, 0), input.Command{Action: input.CopyAndQuit, Extend: false}},
		{"slash", k(tcell.KeyRune, '/', 0), input.Command{Action: input.Search, Extend: false}},
		{"question", k(tcell.KeyRune, '?', tcell.ModShift), input.Command{Action: input.SearchBack, Extend: false}},
		{"n", k(tcell.KeyRune, 'n', 0), input.Command{Action: input.SearchNext, Extend: false}},
		{"N", k(tcell.KeyRune, 'N', 0), input.Command{Action: input.SearchPrev, Extend: false}},
		{"colon", k(tcell.KeyRune, ':', 0), input.Command{Action: input.GoToLine, Extend: false}},
		{"w", k(tcell.KeyRune, 'w', 0), input.Command{Action: input.ToggleWrap, Extend: false}},
		{"q", k(tcell.KeyRune, 'q', 0), input.Command{Action: input.Quit, Extend: false}},
		{"h", k(tcell.KeyRune, 'h', 0), input.Command{Action: input.Help, Extend: false}},
		{"ctrl f without mod", k(tcell.KeyCtrlF, 0, 0), input.Command{Action: input.PageDown, Extend: false}},
		{"ctrl home is unbound", k(tcell.KeyHome, 0, tcell.ModCtrl), input.Command{}},
		{"unbound", k(tcell.KeyRune, 'z', 0), input.Command{}},
		{"unbound key", k(tcell.KeyF1, 0, 0), input.Command{}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			if got := Default(false).Lookup(tc.ev); got != tc.want {
				t.Fatalf("Lookup = %+v, want %+v", got, tc.want)
			}
		})
	}
	mac := []struct {
		name string
		ev   *tcell.EventKey
		want input.Command
	}{
		{"cmd c", k(tcell.KeyRune, 'c', tcell.ModMeta), input.Command{Action: input.Copy, Extend: false}},
		{"cmd a", k(tcell.KeyRune, 'a', tcell.ModMeta), input.Command{Action: input.SelectAll, Extend: false}},
		{"cmd g", k(tcell.KeyRune, 'g', tcell.ModMeta), input.Command{Action: input.SearchNext, Extend: false}},
		{"cmd shift g", k(tcell.KeyRune, 'g', tcell.ModMeta|tcell.ModShift), input.Command{Action: input.SearchPrev, Extend: false}},
		{"cmd shift G as a capital", k(tcell.KeyRune, 'G', tcell.ModMeta|tcell.ModShift), input.Command{Action: input.SearchPrev, Extend: false}},
		{"cmd up", k(tcell.KeyUp, 0, tcell.ModMeta), input.Command{Action: input.First, Extend: false}},
		{"cmd shift up", k(tcell.KeyUp, 0, tcell.ModMeta|tcell.ModShift), input.Command{Action: input.First, Extend: true}},
		{"cmd down", k(tcell.KeyDown, 0, tcell.ModMeta), input.Command{Action: input.Last, Extend: false}},
		{"cmd left", k(tcell.KeyLeft, 0, tcell.ModMeta), input.Command{Action: input.Home, Extend: false}},
		{"cmd shift right", k(tcell.KeyRight, 0, tcell.ModMeta|tcell.ModShift), input.Command{Action: input.End, Extend: true}},
		{"cmd q is unbound", k(tcell.KeyRune, 'q', tcell.ModMeta), input.Command{}},
	}
	for _, tc := range mac {
		t.Run("macOS "+tc.name, func(t *testing.T) {
			if got := Default(true).Lookup(tc.ev); got != tc.want {
				t.Fatalf("Lookup = %+v, want %+v", got, tc.want)
			}
		})
	}
	if got := Default(false).Lookup(k(tcell.KeyRune, 'c', tcell.ModMeta)); got != (input.Command{}) {
		t.Errorf("cmd c elsewhere = %+v, want unbound", got)
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
