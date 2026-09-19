package ansi

import (
	"testing"

	"github.com/gdamore/tcell/v2"
)

func TestParseText(t *testing.T) {
	tests := []struct {
		name string
		in   string
		want string
	}{
		{"plain", "héllo", "héllo"},
		{"csi skipped", "a\x1b[2Kb", "ab"},
		{"osc bel", "a\x1b]8;;http://x\x07b", "ab"},
		{"osc st", "a\x1b]8;;\x1b\\b", "ab"},
		{"charset", "a\x1b(Bb", "ab"},
		{"esc single", "a\x1b7b", "ab"},
		{"c0 dropped", "a\x07b\x08c", "abc"},
		{"del dropped", "a\x7fb", "ab"},
		{"tab and cr kept", "a\tb\rc", "a\tb\rc"},
		{"truncated csi", "abc\x1b[3", "abc"},
		{"truncated esc", "abc\x1b", "abc"},
		{"truncated osc", "abc\x1b]8;;foo", "abc"},
		{"truncated charset", "abc\x1b(", "abc"},
		{"invalid utf8", "a\xffb", "a�b"},
		{"empty", "", ""},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			text, runs := NewParser().Parse([]byte(tc.in))
			if got := string(text); got != tc.want {
				t.Fatalf("text = %q, want %q", got, tc.want)
			}
			if len(text) == 0 {
				if len(runs) != 0 {
					t.Fatalf("runs = %v, want none", runs)
				}
				return
			}
			want := []Run{{0, len(text), tcell.StyleDefault}}
			if len(runs) != 1 || runs[0] != want[0] {
				t.Fatalf("runs = %v, want %v", runs, want)
			}
		})
	}
}
