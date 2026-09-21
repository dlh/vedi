package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestWriteInput(t *testing.T) {
	path := filepath.Join(t.TempDir(), "in")
	if err := writeInput(path, 3); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	lines := strings.Split(strings.TrimSuffix(string(data), "\n"), "\n")
	if len(lines) != 3 || !strings.HasPrefix(lines[2], marker(3)) {
		t.Errorf("lines = %q", lines)
	}
	var s stripper
	if w := len(s.strip(nil, []byte(lines[0]))); w != 54 {
		t.Errorf("line width = %d, want 54", w)
	}
}
