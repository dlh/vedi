package app

import (
	"testing"

	"go.dlh.dev/vedi/internal/input"
)

// TestHelpText: the layout of less's help, 80 columns wide whatever
// the screen: a centered title, a note on the modifier glyphs, the
// first section's rows with no heading, then each section's name
// centered in capitals between dashed rules. Rows are indented two,
// keys two spaces apart in a column as wide as the widest list but at
// most half the room, the doc beside with a period; a list or a doc
// too wide for its column continues on the next line. Ctrl, Alt and
// Cmd are ⌃ ⌥ ⌘, a letter under them in capitals.
func TestHelpText(t *testing.T) {
	secs := []input.Section{
		{Rows: []input.Row{{Keys: []string{"h"}, Doc: "Show the key bindings"}}},
		{Name: "Moving", Rows: []input.Row{
			{Keys: []string{"Up", "Down", "j"}, Doc: "Move the cursor"},
			{Keys: []string{"Ctrl+f", "Alt+Shift+Left", "Cmd+Shift+g", "Alt+<"}, Doc: "Copy the selection and quit; with none, down a line"},
		}},
	}
	want := "                            SUMMARY OF VEDI COMMANDS\n" +
		"\n" +
		"      ⌃ ⌥ ⌘ are Ctrl, Alt and Cmd: ⌃F is Ctrl+f in the config file.\n" +
		"\n" +
		"  h                              Show the key bindings.\n" +
		" ------------------------------------------------------------------------------\n" +
		"\n" +
		"                                     MOVING\n" +
		"\n" +
		"  Up  Down  j                    Move the cursor.\n" +
		"  ⌃F  ⌥Shift+Left  ⌘Shift+G  ⌥<  Copy the selection and quit; with none, down a\n" +
		"                                 line.\n" +
		" ------------------------------------------------------------------------------\n"
	if got := helpText(secs); got != want {
		t.Errorf("helpText =\n%s\nwant\n%s", got, want)
	}
}

func TestHelpGlyphs(t *testing.T) {
	tests := []struct{ name, want string }{
		{"j", "j"}, {"G", "G"}, {"Space", "Space"}, {"Shift+Left", "Shift+Left"},
		{"Ctrl+f", "⌃F"}, {"Ctrl+Left", "⌃Left"}, {"Alt+v", "⌥V"}, {"Alt+<", "⌥<"},
		{"Cmd+c", "⌘C"}, {"Cmd+Shift+g", "⌘Shift+G"}, {"Ctrl+Shift+Left", "⌃Shift+Left"},
		{"Ctrl+Alt+Cmd+Shift+Up", "⌃⌥⌘Shift+Up"}, {"Up, Down at the prompt", "Up, Down at the prompt"},
	}
	for _, tc := range tests {
		if got := helpGlyphs(tc.name); got != tc.want {
			t.Errorf("helpGlyphs(%q) = %q, want %q", tc.name, got, tc.want)
		}
	}
}
