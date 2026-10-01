package input

import (
	"fmt"
	"strings"
	"unicode"
	"unicode/utf8"

	"github.com/gdamore/tcell/v3"
)

// Key is a key as the map knows it: a tcell key, its rune when
// KeyRune, and modifiers, all as Normalize leaves them.
type Key struct {
	Key  tcell.Key
	Rune rune
	Mod  tcell.ModMask
}

// shifted is the US layout's Shift form of each character that has
// one. The kitty keyboard protocol reports a modified key by its
// unshifted character, so ⌥< arrives as , with Shift and Alt.
var shifted = map[rune]rune{
	'`': '~', '1': '!', '2': '@', '3': '#', '4': '$', '5': '%', '6': '^',
	'7': '&', '8': '*', '9': '(', '0': ')', '-': '_', '=': '+', '[': '{',
	']': '}', '\\': '|', ';': ':', '\'': '"', ',': '<', '.': '>', '/': '?',
}

// Normalize makes a key event comparable to a parsed name. A control
// key (Ctrl+letter, Enter, Esc) names itself, so Ctrl is dropped from
// it, and a raw control character becomes its letter's control key.
// Ctrl+Space is a space with Ctrl; NUL is the same key.
// Shift on a character that has a shifted form is that form, as a
// legacy terminal sends it: , with Shift is <. Otherwise Shift is
// dropped from a character: Shift+g is G. Under ⌘ the kitty protocol
// sends ⇧⌘G as G with Shift, so an uppercase letter with Meta becomes
// the lowercase one with Shift. Text of more than one character, a
// pasted cluster, is no key: its Rune is 0.
func Normalize(ev *tcell.EventKey) Key {
	k := Key{Key: ev.Key(), Mod: ev.Modifiers()}
	if str := ev.Str(); utf8.RuneCountInString(str) == 1 {
		k.Rune, _ = utf8.DecodeRuneInString(str)
	}
	switch {
	case k.Key == tcell.KeyNUL || k.Key == tcell.KeyRune && k.Rune == ' ' && k.Mod&tcell.ModCtrl != 0:
		k.Key, k.Rune, k.Mod = tcell.KeyRune, ' ', k.Mod|tcell.ModCtrl
	case k.Key == tcell.KeyRune && k.Mod&tcell.ModShift != 0 && shifted[k.Rune] != 0:
		k.Rune, k.Mod = shifted[k.Rune], k.Mod&^tcell.ModShift
	case k.Key == tcell.KeyRune && k.Mod&tcell.ModMeta == 0:
		k.Mod &^= tcell.ModShift
	case k.Key == tcell.KeyRune && unicode.IsUpper(k.Rune):
		k.Rune = unicode.ToLower(k.Rune)
		k.Mod |= tcell.ModShift
	case k.Key >= tcell.KeyCtrlA && k.Key <= tcell.KeyCtrlZ:
		k.Mod &^= tcell.ModCtrl
	case k.Key < ' ':
		k.Mod &^= tcell.ModCtrl
	}
	if k.Key != tcell.KeyRune {
		k.Rune = 0
	}
	return k
}

// Event is a key event that Normalize maps back to k.
func (k Key) Event() *tcell.EventKey {
	str := ""
	if k.Key == tcell.KeyRune {
		str = string(k.Rune)
	}
	return tcell.NewEventKey(k.Key, str, k.Mod)
}

var modPrefixes = []struct {
	name string
	mod  tcell.ModMask
}{{"Shift+", tcell.ModShift}, {"Ctrl+", tcell.ModCtrl}, {"Alt+", tcell.ModAlt}, {"Cmd+", tcell.ModMeta}}

var namedKeys = map[string]tcell.Key{
	"Up": tcell.KeyUp, "Down": tcell.KeyDown, "Left": tcell.KeyLeft, "Right": tcell.KeyRight,
	"Home": tcell.KeyHome, "End": tcell.KeyEnd, "PgUp": tcell.KeyPgUp, "PgDn": tcell.KeyPgDn,
	"Enter": tcell.KeyEnter, "Esc": tcell.KeyEscape, "Backspace": tcell.KeyBackspace,
}

var keyNames = map[tcell.Key]string{
	tcell.KeyUp: "Up", tcell.KeyDown: "Down", tcell.KeyLeft: "Left", tcell.KeyRight: "Right",
	tcell.KeyHome: "Home", tcell.KeyEnd: "End", tcell.KeyPgUp: "PgUp", tcell.KeyPgDn: "PgDn",
	tcell.KeyEnter: "Enter", tcell.KeyEscape: "Esc", tcell.KeyBackspace: "Backspace",
}

// ParseKey reads a key name: Up Down Left Right Home End PgUp PgDn
// Enter Esc Backspace Space or one character, with any of Shift+
// Ctrl+ Alt+ Cmd+ in front. Ctrl+letter is the control key, either
// case, and Ctrl+Space a space with Ctrl. Shift on a character needs Cmd:
// without it Shift is another character, or nothing, as Shift+Space.
func ParseKey(name string) (Key, error) {
	tok := name
	var mod tcell.ModMask
	for again := true; again; {
		again = false
		for _, p := range modPrefixes {
			if strings.HasPrefix(tok, p.name) {
				mod |= p.mod
				tok = tok[len(p.name):]
				again = true
			}
		}
	}
	if k, ok := namedKeys[tok]; ok {
		return Normalize(tcell.NewEventKey(k, "", mod)), nil
	}
	if tok == "Space" && mod&tcell.ModCtrl != 0 {
		return Normalize(tcell.NewEventKey(tcell.KeyRune, " ", mod)), nil
	}
	if tok == "Space" {
		tok = " "
	}
	r, size := utf8.DecodeRuneInString(tok)
	if size == 0 || size != len(tok) {
		return Key{}, fmt.Errorf("unknown key %q", name)
	}
	if mod&tcell.ModCtrl != 0 && r < utf8.RuneSelf && unicode.IsLetter(r) {
		return Normalize(tcell.NewEventKey(tcell.KeyCtrlA+tcell.Key(unicode.ToLower(r)-'a'), "", mod)), nil
	}
	if mod&tcell.ModCtrl != 0 || mod&(tcell.ModShift|tcell.ModMeta) == tcell.ModShift {
		return Key{}, fmt.Errorf("%q: modifiers on a character key", name)
	}
	return Normalize(tcell.NewEventKey(tcell.KeyRune, string(r), mod)), nil
}

// String is the help name, spelled as the config file spells the
// key: Ctrl+ Alt+ Cmd+ Shift+, then the key, so ParseKey reads it
// back.
func (k Key) String() string {
	ctrl := k.Mod&tcell.ModCtrl != 0
	var name string
	switch {
	case k.Key == tcell.KeyRune && k.Rune == ' ':
		name = "Space"
	case k.Key == tcell.KeyRune:
		name = string(k.Rune)
	case keyNames[k.Key] != "":
		name = keyNames[k.Key]
	case k.Key >= tcell.KeyCtrlA && k.Key <= tcell.KeyCtrlZ:
		ctrl = true
		name = string(rune('a' + k.Key - tcell.KeyCtrlA))
	default:
		name = tcell.KeyNames[k.Key]
	}
	var sb strings.Builder
	if ctrl {
		sb.WriteString("Ctrl+")
	}
	if k.Mod&tcell.ModAlt != 0 {
		sb.WriteString("Alt+")
	}
	if k.Mod&tcell.ModMeta != 0 {
		sb.WriteString("Cmd+")
	}
	if k.Mod&tcell.ModShift != 0 {
		sb.WriteString("Shift+")
	}
	sb.WriteString(name)
	return sb.String()
}
