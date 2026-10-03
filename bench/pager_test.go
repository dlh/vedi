package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// TestEnv: each pager runs with its defaults, so the variables that
// configure less and vedi are dropped, and TERM is fixed.
func TestEnv(t *testing.T) {
	t.Setenv("LESS", "-R")
	t.Setenv("LESSOPEN", "|cat %s")
	t.Setenv("VEDI", "-F -S")
	t.Setenv("TERM", "dumb")
	t.Setenv("KEEP", "1")
	got := map[string]string{}
	for _, kv := range env() {
		k, v, _ := strings.Cut(kv, "=")
		got[k] = v
	}
	for _, k := range []string{"LESS", "LESSOPEN", "VEDI"} {
		if v, ok := got[k]; ok {
			t.Errorf("%s=%q kept", k, v)
		}
	}
	if got["TERM"] != "xterm-256color" || got["LESSHISTFILE"] != "-" || got["KEEP"] != "1" {
		t.Errorf("TERM=%q LESSHISTFILE=%q KEEP=%q", got["TERM"], got["LESSHISTFILE"], got["KEEP"])
	}
}

// TestSession: a shell stand-in for a pager prints the file, waits for
// a key, says bye and exits; RSS is positive whatever the platform.
func TestSession(t *testing.T) {
	file := filepath.Join(t.TempDir(), "in")
	if err := writeInput(file, 3); err != nil {
		t.Fatal(err)
	}
	s, err := start("", []string{"sh", "-c", `cat "$1"; read k; echo bye`, "sh", file}, nil)
	if err != nil {
		t.Fatal(err)
	}
	if err := s.wait(marker(3), 5*time.Second); err != nil {
		s.kill()
		t.Fatal(err)
	}
	if err := s.send("\r"); err != nil {
		t.Fatal(err)
	}
	if err := s.wait("bye", 5*time.Second); err != nil {
		s.kill()
		t.Fatal(err)
	}
	rss, err := s.finish(5 * time.Second)
	if err != nil || rss <= 0 {
		t.Errorf("finish = %d, %v", rss, err)
	}
}

// TestSessionStdin: a pipe on stdin still leaves the pty as the
// controlling terminal.
func TestSessionStdin(t *testing.T) {
	r, w, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	w.WriteString("hello\n")
	w.Close()
	s, err := start("", []string{"cat"}, r)
	r.Close()
	if err != nil {
		t.Fatal(err)
	}
	if err := s.wait("hello", 5*time.Second); err != nil {
		s.kill()
		t.Fatal(err)
	}
	s.finish(5 * time.Second)
}

func TestWaitTimeout(t *testing.T) {
	s, err := start("", []string{"sleep", "5"}, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer s.kill()
	if err := s.wait("never", 50*time.Millisecond); err != errTimeout {
		t.Errorf("err = %v, want errTimeout", err)
	}
}

// TestWaitExit: a pager that exits first fails the wait at once.
func TestWaitExit(t *testing.T) {
	s, err := start("", []string{"true"}, nil)
	if err != nil {
		t.Fatal(err)
	}
	if err := s.wait("never", 5*time.Second); err == nil || err == errTimeout {
		t.Errorf("err = %v, want exit error", err)
	}
	s.finish(time.Second)
}
