package main

import (
	"io"
	"os"
	"path/filepath"
	"syscall"
	"testing"
)

// TestOpenInputFIFO: a named pipe cannot be read at random, so it is
// read once and kept in memory.
func TestOpenInputFIFO(t *testing.T) {
	dir := t.TempDir()
	pipe := filepath.Join(dir, "pipe")
	if err := syscall.Mkfifo(pipe, 0o600); err != nil {
		t.Fatal(err)
	}
	go func() { os.WriteFile(pipe, []byte("hello\n"), 0) }()
	in, src, closeInput, err := openInput([]string{pipe})
	if err != nil {
		t.Fatal(err)
	}
	defer closeInput()
	if src != nil {
		t.Error("src != nil for a pipe")
	}
	got, err := io.ReadAll(in)
	if err != nil || string(got) != "hello\n" {
		t.Errorf("read %q, %v", got, err)
	}
}

// TestOpenInputFile: a regular file is read at random.
func TestOpenInputFile(t *testing.T) {
	name := filepath.Join(t.TempDir(), "f")
	if err := os.WriteFile(name, []byte("hello\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	_, src, closeInput, err := openInput([]string{name})
	if err != nil {
		t.Fatal(err)
	}
	defer closeInput()
	if src == nil {
		t.Error("src == nil for a regular file")
	}
}
