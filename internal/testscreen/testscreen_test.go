package testscreen

import (
	"testing"
	"time"

	"github.com/gdamore/tcell/v3"
)

func TestSizeAndEmptyQueue(t *testing.T) {
	for range 50 {
		s := New(t, 20, 5)
		if w, h := s.Size(); w != 20 || h != 5 {
			t.Fatalf("size = %dx%d, want 20x5", w, h)
		}
		if s.Pending() {
			t.Fatal("a new screen has events queued")
		}
	}
	s := New(t, 20, 5)
	time.Sleep(20 * time.Millisecond)
	if s.Pending() {
		t.Fatalf("an event arrived after New: %T", <-s.EventQ())
	}
}

func TestRowReadsClustersWhole(t *testing.T) {
	s := New(t, 20, 5)
	x := 0
	for _, g := range []string{"a", "日", "é", "👨‍👩‍👧", "b"} {
		_, w := s.Put(x, 1, g, tcell.StyleDefault)
		x += w
	}
	s.Show()
	if got, want := s.Row(1), "a日é👨‍👩‍👧b"; got != want {
		t.Errorf("row = %q, want %q", got, want)
	}
	if got := s.Row(0); got != "" {
		t.Errorf("empty row = %q", got)
	}
}

func TestStyleAt(t *testing.T) {
	s := New(t, 20, 5)
	st := tcell.StyleDefault.Foreground(tcell.PaletteColor(1)).Url("http://x")
	s.Put(3, 2, "a", st)
	s.Show()
	if got := s.StyleAt(3, 2); got != st {
		t.Errorf("style = %v, want %v", got, st)
	}
	if got := s.StyleAt(4, 2); got != tcell.StyleDefault {
		t.Errorf("untouched style = %v, want default", got)
	}
}

func TestCursor(t *testing.T) {
	s := New(t, 20, 5)
	s.ShowCursor(2, 1)
	s.Show()
	if x, y, vis := s.Cursor(); x != 2 || y != 1 || !vis {
		t.Errorf("cursor = %d %d %v, want 2 1 true", x, y, vis)
	}
	s.HideCursor()
	s.Show()
	if _, _, vis := s.Cursor(); vis {
		t.Error("cursor visible after HideCursor")
	}
}

func TestClipboard(t *testing.T) {
	s := New(t, 20, 5)
	s.SetClipboard([]byte("hi\n"))
	if got := s.Clipboard(); got != "hi\n" {
		t.Errorf("clipboard = %q", got)
	}
}

func TestResize(t *testing.T) {
	s := New(t, 20, 5)
	s.Resize(t, 30, 8)
	if w, h := s.Size(); w != 30 || h != 8 {
		t.Fatalf("size = %dx%d, want 30x8", w, h)
	}
	s.Put(29, 7, "z", tcell.StyleDefault)
	s.Show()
	if got := s.Row(7); got != "                             z" {
		t.Errorf("row = %q", got)
	}
}

// TestHermetic: the screen is the same whatever terminal runs the tests.
func TestHermetic(t *testing.T) {
	t.Setenv("TERM", "vt100")
	t.Setenv("TERM_PROGRAM", "Apple_Terminal")
	s := New(t, 20, 5)
	s.SetClipboard([]byte("x"))
	if got := s.Clipboard(); got != "x" {
		t.Errorf("clipboard = %q", got)
	}
}
