package main

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

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
	if err := s.wait(marker(3), 0, 5*time.Second); err != nil {
		s.kill()
		t.Fatal(err)
	}
	from := s.mark()
	if err := s.send("\r"); err != nil {
		t.Fatal(err)
	}
	if err := s.wait("bye", from, 5*time.Second); err != nil {
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
	if err := s.wait("hello", 0, 5*time.Second); err != nil {
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
	if err := s.wait("never", 0, 50*time.Millisecond); err != errTimeout {
		t.Errorf("err = %v, want errTimeout", err)
	}
}

// TestWaitExit: a pager that exits first fails the wait at once.
func TestWaitExit(t *testing.T) {
	s, err := start("", []string{"true"}, nil)
	if err != nil {
		t.Fatal(err)
	}
	if err := s.wait("never", 0, 5*time.Second); err == nil || err == errTimeout {
		t.Errorf("err = %v, want exit error", err)
	}
	s.finish(time.Second)
}
