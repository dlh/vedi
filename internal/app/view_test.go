package app

import (
	"slices"
	"testing"
)

func TestWrapKeys(t *testing.T) {
	keys := []string{"⇟", "Space", "f", "⌃F", "⌃V"}
	tests := []struct {
		width int
		want  []string
	}{
		{20, []string{"⇟, Space, f, ⌃F, ⌃V"}},
		{18, []string{"⇟, Space, f, ⌃F", "⌃V"}},
		{8, []string{"⇟, Space", "f, ⌃F", "⌃V"}},
		{3, []string{"⇟", "Space", "f", "⌃F", "⌃V"}},
		{0, []string{"⇟", "Space", "f", "⌃F", "⌃V"}},
	}
	for _, tc := range tests {
		if got := wrapKeys(keys, tc.width); !slices.Equal(got, tc.want) {
			t.Errorf("wrapKeys(%d) = %q, want %q", tc.width, got, tc.want)
		}
	}
}
