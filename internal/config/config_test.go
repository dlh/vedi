package config

import (
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"github.com/gdamore/tcell/v3"
	"go.dlh.dev/vedi/internal/ansi"
	"go.dlh.dev/vedi/internal/input"
	"go.dlh.dev/vedi/internal/layout"
)

func key(name string) input.Key {
	k, err := input.ParseKey(name)
	if err != nil {
		panic(err)
	}
	return k
}

func TestParse(t *testing.T) {
	src := "# vim\n\nmap h left\n\tmap  L\tselect_right \nmap ? none\r\nmap Alt+w cycle wrap_style\n"
	c, err := Parse("vedi.conf", []byte(src))
	if err != nil {
		t.Fatal(err)
	}
	got := c.Keys
	want := []input.Binding{
		{Key: input.Key{Key: tcell.KeyRune, Rune: 'h', Mod: 0}, Cmd: input.Command{Action: input.Left}},
		{Key: input.Key{Key: tcell.KeyRune, Rune: 'L', Mod: 0}, Cmd: input.Command{Action: input.Right, Extend: true}},
		{Key: input.Key{Key: tcell.KeyRune, Rune: '?', Mod: 0}, Cmd: input.Command{}},
		{Key: input.Key{Key: tcell.KeyRune, Rune: 'w', Mod: tcell.ModAlt}, Cmd: input.Command{Action: input.Cycle, Arg: "wrap_style"}},
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
		{"map h cycle\n", "vedi.conf:1: cycle takes wrap, wrap_style, edge_markers, file_separators, auto_reload, status_line or view_style"},
		{"map h cycle tab_width\n", "vedi.conf:1: cycle takes wrap, wrap_style, edge_markers, file_separators, auto_reload, status_line or view_style"},
		{"map h cycle wrap yes\n", "vedi.conf:1: map takes a key and an action"},
		{"map h toggle_wrap\n", `vedi.conf:1: unknown action "toggle_wrap"`},
	}
	for _, tc := range bad {
		if _, err := Parse("vedi.conf", []byte(tc.src)); err == nil || err.Error() != tc.err {
			t.Errorf("Parse(%q) err = %v, want %s", tc.src, err, tc.err)
		}
	}
}

// TestParseLine: one line applies on its own, naming its verb, with the
// error bare of the file position that Parse adds.
func TestParseLine(t *testing.T) {
	var c Config
	verb, err := ParseLine("  wrap_style  word ", &c)
	if verb != "wrap_style" || c.WrapStyle != layout.WrapStyleWord || err != nil {
		t.Errorf("ParseLine(wrap_style word) = %q, %+v, %v", verb, c, err)
	}
	verb, err = ParseLine("clipboard_cmd sh -c false", &c)
	if verb != "clipboard_cmd" || c.ClipboardCmd != "sh -c false" || err != nil {
		t.Errorf("ParseLine(clipboard_cmd) = %q, %+v, %v", verb, c, err)
	}
	for _, blank := range []string{"", "   ", "# a comment"} {
		var c Config
		if verb, err := ParseLine(blank, &c); verb != "" || err != nil || !reflect.DeepEqual(c, Config{}) {
			t.Errorf("ParseLine(%q) = %q, %+v, %v", blank, verb, c, err)
		}
	}
	bad := []struct{ src, err string }{
		{"wrap_style wide", "wrap_style takes char or word"},
		{"bind j down", `unknown verb "bind"`},
	}
	for _, tc := range bad {
		var c Config
		if verb, err := ParseLine(tc.src, &c); err == nil || err.Error() != tc.err || verb != strings.Fields(tc.src)[0] {
			t.Errorf("ParseLine(%q) = %q, %v, want %s", tc.src, verb, err, tc.err)
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
	// Alone it leaves an empty map, not the nil that means the defaults.
	c, err = Parse("vedi.conf", []byte("clear_all_shortcuts\n"))
	if err != nil {
		t.Fatal(err)
	}
	if got := c.Keymap(true); got == nil || len(got) != 0 {
		t.Errorf("Keymap = %#v, want an empty non-nil map", got)
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
		str := ""
		if r != 0 {
			str = string(r)
		}
		return tcell.NewEventKey(key, str, mod)
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
	if got := m.Lookup(tcell.NewEventKey(tcell.KeyRune, ",", tcell.ModShift|tcell.ModAlt)); got != (input.Command{Action: input.First}) {
		t.Errorf("kitty Alt+< = %+v", got)
	}
	if got := m.Lookup(key("h").Event()); got != (input.Command{Action: input.Left}) {
		t.Errorf("h = %+v", got)
	}
	if got := m.Lookup(tcell.NewEventKey(tcell.KeyRune, "L", tcell.ModShift)); got != (input.Command{Action: input.Right, Extend: true}) {
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
	ev := tcell.NewEventKey(tcell.KeyRune, "h", 0)
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
	if got := Default(false).Apply(c.Keys).Lookup(tcell.NewEventKey(tcell.KeyRune, "h", 0)); got != (input.Command{Action: input.Left}) {
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

// TestWrap: wrap no starts in nowrap mode; yes, the default, in wrap;
// anything else is an error.
func TestWrap(t *testing.T) {
	for _, tc := range []struct {
		src  string
		want bool
	}{
		{"", false},
		{"wrap no\n", true},
		{"wrap yes\n", false},
		{"wrap no\nwrap yes\n", false},
		{"wrap yes\nmap x quit\nwrap no\n", true},
	} {
		c, err := Parse("vedi.conf", []byte(tc.src))
		if err != nil || c.NoWrap != tc.want {
			t.Errorf("Parse(%q) = %+v, %v; want NoWrap %v", tc.src, c, err, tc.want)
		}
	}
	for _, src := range []string{"wrap\n", "wrap maybe\n", "wrap no yes\n", "wrap word\n"} {
		if _, err := Parse("vedi.conf", []byte(src)); err == nil || err.Error() != "vedi.conf:1: wrap takes yes or no" {
			t.Errorf("Parse(%q) err = %v, want wrap takes yes or no", src, err)
		}
	}
}

// TestWrapStyle: wrap_style word breaks wrapped rows at words; char, the
// default, at the screen's edge; anything else is an error.
func TestWrapStyle(t *testing.T) {
	for _, tc := range []struct {
		src  string
		want layout.WrapStyle
	}{
		{"", layout.WrapStyleChar},
		{"wrap_style word\n", layout.WrapStyleWord},
		{"wrap_style char\n", layout.WrapStyleChar},
		{"wrap_style word\nwrap_style char\n", layout.WrapStyleChar},
		{"wrap_style word\nwrap no\n", layout.WrapStyleWord},
	} {
		c, err := Parse("vedi.conf", []byte(tc.src))
		if err != nil || c.WrapStyle != tc.want {
			t.Errorf("Parse(%q) = %+v, %v; want WrapStyle %v", tc.src, c, err, tc.want)
		}
	}
	for _, src := range []string{"wrap_style\n", "wrap_style yes\n", "wrap_style word char\n"} {
		if _, err := Parse("vedi.conf", []byte(src)); err == nil || err.Error() != "vedi.conf:1: wrap_style takes char or word" {
			t.Errorf("Parse(%q) err = %v, want wrap_style takes char or word", src, err)
		}
	}
}

// TestClipboardCmd: clipboard_cmd takes the rest of the line, spaces
// and all; the last line wins; a bare verb is an error.
func TestClipboardCmd(t *testing.T) {
	for _, tc := range []struct{ src, want string }{
		{"", ""},
		{"clipboard_cmd pbcopy\n", "pbcopy"},
		{"clipboard_cmd  xclip -selection clipboard  \n", "xclip -selection clipboard"},
		{"\tclipboard_cmd\twl-copy\n", "wl-copy"},
		{"clipboard_cmd pbcopy\nclipboard_cmd wl-copy\n", "wl-copy"},
	} {
		c, err := Parse("vedi.conf", []byte(tc.src))
		if err != nil || c.ClipboardCmd != tc.want {
			t.Errorf("Parse(%q) = %+v, %v; want ClipboardCmd %q", tc.src, c, err, tc.want)
		}
	}
	if _, err := Parse("vedi.conf", []byte("clipboard_cmd\n")); err == nil || err.Error() != "vedi.conf:1: clipboard_cmd takes a command" {
		t.Errorf("Parse(\"clipboard_cmd\") err = %v, want clipboard_cmd takes a command", err)
	}
}

// TestOpenCmd: open_cmd takes a command, split as a shell would, or
// a glob on the file's base name and then the command; the pattern
// may be quoted. A bare verb, a pattern alone and a bad pattern are
// errors.
func TestOpenCmd(t *testing.T) {
	for _, tc := range []struct {
		src  string
		want []OpenCmd
	}{
		{"", nil},
		{"open_cmd bat --color=always --paging=never %s\n", []OpenCmd{{"", []string{"bat", "--color=always", "--paging=never", "%s"}}}},
		{"\topen_cmd\t cat  '%s' \n", []OpenCmd{{"", []string{"cat", "%s"}}}},
		{"open_cmd *.md mdcat %s\nopen_cmd cat %s\n", []OpenCmd{{"*.md", []string{"mdcat", "%s"}}, {"", []string{"cat", "%s"}}}},
		{"open_cmd [Mm]akefile cat %s\n", []OpenCmd{{"[mm]akefile", []string{"cat", "%s"}}}},
		{"open_cmd '*.md' cat %s\n", []OpenCmd{{"*.md", []string{"cat", "%s"}}}},
		{"open_cmd \"my *.md\" cat %s\n", []OpenCmd{{"my *.md", []string{"cat", "%s"}}}},
	} {
		c, err := Parse("vedi.conf", []byte(tc.src))
		if err != nil || !reflect.DeepEqual(c.OpenCmds, tc.want) {
			t.Errorf("Parse(%q) = %+v, %v; want OpenCmds %+v", tc.src, c.OpenCmds, err, tc.want)
		}
	}
	for _, tc := range []struct{ src, want string }{
		{"open_cmd\n", "vedi.conf:1: open_cmd takes a command"},
		{"open_cmd *.md\n", "vedi.conf:1: open_cmd takes a command"},
		{"open_cmd [md cat %s\n", "vedi.conf:1: open_cmd: bad pattern [md"},
		{"open_cmd cat '%s\n", "vedi.conf:1: open_cmd: unclosed '"},
	} {
		if _, err := Parse("vedi.conf", []byte(tc.src)); err == nil || err.Error() != tc.want {
			t.Errorf("Parse(%q) err = %v, want %s", tc.src, err, tc.want)
		}
	}
}

// TestOpenCmdFor: a file is read through the last pattern line
// matching its base name, ignoring case, else the last plain line,
// else as it is.
func TestOpenCmdFor(t *testing.T) {
	c, err := Parse("vedi.conf", []byte(`
open_cmd *.md mdcat %s
open_cmd cat %s
open_cmd *.py bat %s
open_cmd *.md glow %s
open_cmd less %s
`))
	if err != nil {
		t.Fatal(err)
	}
	for _, tc := range []struct {
		name string
		want []string
	}{
		{"notes.md", []string{"glow", "%s"}},
		{"docs/README.MD", []string{"glow", "%s"}},
		{"a.py", []string{"bat", "%s"}},
		{"md/x.txt", []string{"less", "%s"}},
	} {
		if got := c.OpenCmdFor(tc.name); !reflect.DeepEqual(got, tc.want) {
			t.Errorf("OpenCmdFor(%q) = %v, want %v", tc.name, got, tc.want)
		}
	}
	c, _ = Parse("vedi.conf", []byte("open_cmd *.md mdcat %s\n"))
	if got := c.OpenCmdFor("a.txt"); got != nil {
		t.Errorf("OpenCmdFor(a.txt) with only a pattern = %v, want nil", got)
	}
}

// TestOnePageRowsBelow: one_page_rows_below takes a positive number;
// zero, the default, leaves the rows to -F.
func TestOnePageRowsBelow(t *testing.T) {
	for _, tc := range []struct {
		src  string
		want int
	}{
		{"", 0},
		{"one_page_rows_below 2\n", 2},
		{"one_page_rows_below 2\none_page_rows_below 3\n", 3},
	} {
		c, err := Parse("vedi.conf", []byte(tc.src))
		if err != nil || c.OnePageRowsBelow != tc.want {
			t.Errorf("Parse(%q) = %+v, %v; want OnePageRowsBelow %d", tc.src, c, err, tc.want)
		}
	}
	for _, src := range []string{"one_page_rows_below\n", "one_page_rows_below two\n", "one_page_rows_below 0\n", "one_page_rows_below -1\n", "one_page_rows_below 1 2\n"} {
		if _, err := Parse("vedi.conf", []byte(src)); err == nil || err.Error() != "vedi.conf:1: one_page_rows_below takes a positive number" {
			t.Errorf("Parse(%q) err = %v, want one_page_rows_below takes a positive number", src, err)
		}
	}
}

// TestTabWidth: tab_width takes a positive number; zero, the default,
// leaves the tab width to the app.
func TestTabWidth(t *testing.T) {
	for _, tc := range []struct {
		src  string
		want int
	}{
		{"", 0},
		{"tab_width 4\n", 4},
		{"tab_width 8\n", 8},
		{"tab_width 2\ntab_width 4\n", 4},
		{"tab_width 4\nmap x quit\n", 4},
	} {
		c, err := Parse("vedi.conf", []byte(tc.src))
		if err != nil || c.TabWidth != tc.want {
			t.Errorf("Parse(%q) = %+v, %v; want TabWidth %d", tc.src, c, err, tc.want)
		}
	}
	for _, src := range []string{"tab_width\n", "tab_width four\n", "tab_width 0\n", "tab_width -4\n", "tab_width 4 8\n"} {
		if _, err := Parse("vedi.conf", []byte(src)); err == nil || err.Error() != "vedi.conf:1: tab_width takes a positive number" {
			t.Errorf("Parse(%q) err = %v, want tab_width takes a positive number", src, err)
		}
	}
}

// TestEdgeMarkers: edge_markers yes marks text off the side of the
// screen in nowrap mode; no, the default, does not; anything else is
// an error.
func TestEdgeMarkers(t *testing.T) {
	for _, tc := range []struct {
		src  string
		want bool
	}{
		{"", false},
		{"edge_markers yes\n", true},
		{"edge_markers no\n", false},
		{"edge_markers yes\nedge_markers no\n", false},
		{"edge_markers no\nmap x quit\nedge_markers yes\n", true},
	} {
		c, err := Parse("vedi.conf", []byte(tc.src))
		if err != nil || c.EdgeMarkers != tc.want {
			t.Errorf("Parse(%q) = %+v, %v; want EdgeMarkers %v", tc.src, c, err, tc.want)
		}
	}
	bad := []struct{ src, err string }{
		{"edge_markers\n", "vedi.conf:1: edge_markers takes yes or no"},
		{"edge_markers maybe\n", "vedi.conf:1: edge_markers takes yes or no"},
		{"edge_markers yes no\n", "vedi.conf:1: edge_markers takes yes or no"},
	}
	for _, tc := range bad {
		if _, err := Parse("vedi.conf", []byte(tc.src)); err == nil || err.Error() != tc.err {
			t.Errorf("Parse(%q) err = %v, want %s", tc.src, err, tc.err)
		}
	}
}

// TestFileSeparators: file_separators no leaves out the rows between
// inputs; yes, the default, draws them; anything else is an error.
func TestFileSeparators(t *testing.T) {
	for _, tc := range []struct {
		src  string
		want bool
	}{
		{"", false},
		{"file_separators no\n", true},
		{"file_separators yes\n", false},
		{"file_separators no\nfile_separators yes\n", false},
		{"file_separators yes\nmap x quit\nfile_separators no\n", true},
	} {
		c, err := Parse("vedi.conf", []byte(tc.src))
		if err != nil || c.NoFileSeparators != tc.want {
			t.Errorf("Parse(%q) = %+v, %v; want NoFileSeparators %v", tc.src, c, err, tc.want)
		}
	}
	bad := []struct{ src, err string }{
		{"file_separators\n", "vedi.conf:1: file_separators takes yes or no"},
		{"file_separators maybe\n", "vedi.conf:1: file_separators takes yes or no"},
		{"file_separators yes no\n", "vedi.conf:1: file_separators takes yes or no"},
	}
	for _, tc := range bad {
		if _, err := Parse("vedi.conf", []byte(tc.src)); err == nil || err.Error() != tc.err {
			t.Errorf("Parse(%q) err = %v, want %s", tc.src, err, tc.err)
		}
	}
}

// TestStatusLine: status_line no hides the status line; yes, the
// default, draws it; anything else is an error.
func TestStatusLine(t *testing.T) {
	for _, tc := range []struct {
		src  string
		want bool
	}{
		{"", false},
		{"status_line no\n", true},
		{"status_line yes\n", false},
		{"status_line no\nstatus_line yes\n", false},
	} {
		c, err := Parse("vedi.conf", []byte(tc.src))
		if err != nil || c.NoStatusLine != tc.want {
			t.Errorf("Parse(%q) = %+v, %v; want NoStatusLine %v", tc.src, c, err, tc.want)
		}
	}
	for _, src := range []string{"status_line\n", "status_line maybe\n", "status_line yes no\n"} {
		want := "vedi.conf:1: status_line takes yes or no"
		if _, err := Parse("vedi.conf", []byte(src)); err == nil || err.Error() != want {
			t.Errorf("Parse(%q) err = %v, want %s", src, err, want)
		}
	}
}

// TestViewStyle: view_style plain draws the text without its colors,
// raw with its escapes as text; color, the default, with its colors;
// anything else is an error.
func TestViewStyle(t *testing.T) {
	for _, tc := range []struct {
		src  string
		want ansi.ViewStyle
	}{
		{"", ansi.ViewColor},
		{"view_style plain\n", ansi.ViewPlain},
		{"view_style color\n", ansi.ViewColor},
		{"view_style raw\n", ansi.ViewRaw},
		{"view_style plain\nview_style color\n", ansi.ViewColor},
	} {
		c, err := Parse("vedi.conf", []byte(tc.src))
		if err != nil || c.ViewStyle != tc.want {
			t.Errorf("Parse(%q) = %+v, %v; want ViewStyle %v", tc.src, c, err, tc.want)
		}
	}
	for _, src := range []string{"view_style\n", "view_style yes\n", "view_style plain color\n"} {
		want := "vedi.conf:1: view_style takes color, plain or raw"
		if _, err := Parse("vedi.conf", []byte(src)); err == nil || err.Error() != want {
			t.Errorf("Parse(%q) err = %v, want %s", src, err, want)
		}
	}
}
