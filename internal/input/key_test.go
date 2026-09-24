package input

import (
	"testing"

	"github.com/gdamore/tcell/v2"
)

func ev(k tcell.Key, r rune, m tcell.ModMask) *tcell.EventKey { return tcell.NewEventKey(k, r, m) }

func TestNormalize(t *testing.T) {
	tests := []struct {
		name string
		ev   *tcell.EventKey
		want Key
	}{
		{"up", ev(tcell.KeyUp, 0, 0), Key{tcell.KeyUp, 0, 0}},
		{"shift up keeps shift", ev(tcell.KeyUp, 0, tcell.ModShift), Key{tcell.KeyUp, 0, tcell.ModShift}},
		{"shift on a character is dropped", ev(tcell.KeyRune, '<', tcell.ModShift), Key{tcell.KeyRune, '<', 0}},
		{"capital stays capital", ev(tcell.KeyRune, 'G', tcell.ModShift), Key{tcell.KeyRune, 'G', 0}},
		{"ctrl f", ev(tcell.KeyCtrlF, 0, tcell.ModCtrl), Key{tcell.KeyCtrlF, 0, 0}},
		{"ctrl f without mod", ev(tcell.KeyCtrlF, 0, 0), Key{tcell.KeyCtrlF, 0, 0}},
		{"ctrl f as a rune", ev(tcell.KeyRune, 0x06, 0), Key{tcell.KeyCtrlF, 0, 0}},
		{"enter", ev(tcell.KeyEnter, 0, 0), Key{tcell.KeyEnter, 0, 0}},
		{"esc", ev(tcell.KeyEscape, 0, 0), Key{tcell.KeyEscape, 0, 0}},
		{"alt v", ev(tcell.KeyRune, 'v', tcell.ModAlt), Key{tcell.KeyRune, 'v', tcell.ModAlt}},
		{"cmd c", ev(tcell.KeyRune, 'c', tcell.ModMeta), Key{tcell.KeyRune, 'c', tcell.ModMeta}},
		{"cmd shift g", ev(tcell.KeyRune, 'g', tcell.ModMeta|tcell.ModShift), Key{tcell.KeyRune, 'g', tcell.ModMeta | tcell.ModShift}},
		{"cmd G is cmd shift g", ev(tcell.KeyRune, 'G', tcell.ModMeta), Key{tcell.KeyRune, 'g', tcell.ModMeta | tcell.ModShift}},
		{"ctrl shift left", ev(tcell.KeyLeft, 0, tcell.ModCtrl|tcell.ModShift), Key{tcell.KeyLeft, 0, tcell.ModCtrl | tcell.ModShift}},
		{"kitty alt shift comma is alt <", ev(tcell.KeyRune, ',', tcell.ModShift|tcell.ModAlt), Key{tcell.KeyRune, '<', tcell.ModAlt}},
		{"kitty alt shift period is alt >", ev(tcell.KeyRune, '.', tcell.ModShift|tcell.ModAlt), Key{tcell.KeyRune, '>', tcell.ModAlt}},
		{"kitty cmd shift slash is cmd ?", ev(tcell.KeyRune, '/', tcell.ModShift|tcell.ModMeta), Key{tcell.KeyRune, '?', tcell.ModMeta}},
		{"legacy alt < is alt <", ev(tcell.KeyRune, '<', tcell.ModAlt), Key{tcell.KeyRune, '<', tcell.ModAlt}},
		{"ctrl space", ev(tcell.KeyCtrlSpace, 0, tcell.ModCtrl), Key{tcell.KeyCtrlSpace, 0, 0}},
		{"nul is ctrl space", ev(tcell.KeyNUL, 0, tcell.ModCtrl), Key{tcell.KeyCtrlSpace, 0, 0}},
		{"kitty ctrl space", ev(tcell.KeyRune, ' ', tcell.ModCtrl), Key{tcell.KeyCtrlSpace, 0, 0}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := Normalize(tc.ev)
			if got != tc.want {
				t.Fatalf("Normalize = %+v, want %+v", got, tc.want)
			}
			if again := Normalize(got.Event()); again != got {
				t.Fatalf("Normalize(Event()) = %+v, want %+v", again, got)
			}
		})
	}
}

func TestParseKey(t *testing.T) {
	tests := []struct {
		name string
		want Key
	}{
		{"Up", Key{tcell.KeyUp, 0, 0}},
		{"Shift+Down", Key{tcell.KeyDown, 0, tcell.ModShift}},
		{"Ctrl+Shift+Left", Key{tcell.KeyLeft, 0, tcell.ModCtrl | tcell.ModShift}},
		{"Shift+Ctrl+Left", Key{tcell.KeyLeft, 0, tcell.ModCtrl | tcell.ModShift}},
		{"Alt+Right", Key{tcell.KeyRight, 0, tcell.ModAlt}},
		{"Home", Key{tcell.KeyHome, 0, 0}},
		{"End", Key{tcell.KeyEnd, 0, 0}},
		{"PgUp", Key{tcell.KeyPgUp, 0, 0}},
		{"Shift+PgDn", Key{tcell.KeyPgDn, 0, tcell.ModShift}},
		{"Enter", Key{tcell.KeyEnter, 0, 0}},
		{"Esc", Key{tcell.KeyEscape, 0, 0}},
		{"Backspace", Key{tcell.KeyBackspace, 0, 0}},
		{"Space", Key{tcell.KeyRune, ' ', 0}},
		{"Ctrl+Space", Key{tcell.KeyCtrlSpace, 0, 0}},
		{"j", Key{tcell.KeyRune, 'j', 0}},
		{"G", Key{tcell.KeyRune, 'G', 0}},
		{"<", Key{tcell.KeyRune, '<', 0}},
		{"é", Key{tcell.KeyRune, 'é', 0}},
		{"Ctrl+f", Key{tcell.KeyCtrlF, 0, 0}},
		{"Ctrl+F", Key{tcell.KeyCtrlF, 0, 0}},
		{"Alt+v", Key{tcell.KeyRune, 'v', tcell.ModAlt}},
		{"Alt+<", Key{tcell.KeyRune, '<', tcell.ModAlt}},
		{"Cmd+c", Key{tcell.KeyRune, 'c', tcell.ModMeta}},
		{"Cmd+Shift+g", Key{tcell.KeyRune, 'g', tcell.ModMeta | tcell.ModShift}},
		{"Cmd+G", Key{tcell.KeyRune, 'g', tcell.ModMeta | tcell.ModShift}},
		{"Cmd+Up", Key{tcell.KeyUp, 0, tcell.ModMeta}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got, err := ParseKey(tc.name)
			if err != nil || got != tc.want {
				t.Fatalf("ParseKey = %+v, %v; want %+v", got, err, tc.want)
			}
		})
	}
	bad := []struct{ name, err string }{
		{"F1", `unknown key "F1"`},
		{"Spacex", `unknown key "Spacex"`},
		{"Ctrl+", `unknown key "Ctrl+"`},
		{"", `unknown key ""`},
		{"Shift+g", `"Shift+g": modifiers on a character key`},
		{"Shift+Space", `"Shift+Space": modifiers on a character key`},
		{"Ctrl+<", `"Ctrl+<": modifiers on a character key`},
	}
	for _, tc := range bad {
		t.Run("bad "+tc.name, func(t *testing.T) {
			if _, err := ParseKey(tc.name); err == nil || err.Error() != tc.err {
				t.Fatalf("ParseKey(%q) err = %v, want %s", tc.name, err, tc.err)
			}
		})
	}
}

// TestKeyString: the help name is the config spelling.
func TestKeyString(t *testing.T) {
	tests := []struct{ name, want string }{
		{"Up", "Up"}, {"Down", "Down"}, {"Left", "Left"}, {"Right", "Right"},
		{"Home", "Home"}, {"End", "End"}, {"PgUp", "PgUp"}, {"PgDn", "PgDn"},
		{"Enter", "Enter"}, {"Esc", "Esc"}, {"Backspace", "Backspace"}, {"Space", "Space"}, {"Ctrl+Space", "Ctrl+Space"},
		{"j", "j"}, {"G", "G"}, {"<", "<"},
		{"Ctrl+f", "Ctrl+f"}, {"Ctrl+F", "Ctrl+f"}, {"Alt+v", "Alt+v"}, {"Alt+V", "Alt+V"},
		{"Shift+Up", "Shift+Up"}, {"Ctrl+Shift+Left", "Ctrl+Shift+Left"}, {"Shift+Alt+Right", "Alt+Shift+Right"},
		{"Cmd+c", "Cmd+c"}, {"Cmd+Shift+g", "Cmd+Shift+g"}, {"Cmd+G", "Cmd+Shift+g"}, {"Cmd+Left", "Cmd+Left"},
	}
	for _, tc := range tests {
		k, err := ParseKey(tc.name)
		if err != nil {
			t.Fatal(err)
		}
		if got := k.String(); got != tc.want {
			t.Errorf("%s: String = %q, want %q", tc.name, got, tc.want)
		}
		if back, err := ParseKey(k.String()); err != nil || back != k {
			t.Errorf("%s: ParseKey(%q) = %+v, %v; want %+v", tc.name, k.String(), back, err, k)
		}
	}
}
