// Package testscreen is a tcell screen on a mock terminal, for tests.
// tcell says the mock is not public API: go.mod pins the version.
package testscreen

import (
	"strings"
	"testing"
	"time"

	"github.com/gdamore/tcell/v3"
	"github.com/gdamore/tcell/v3/vt"
)

// Screen is a tcell.Screen whose terminal the test can resize and read
// back.
type Screen struct {
	tcell.Screen
	term vt.MockTerm
	w, h int // as asked for: the mock terminal is never less than 1×1
}

// New returns a w×h screen, initialized, with nothing queued. It is
// the same whatever terminal runs the tests.
func New(t testing.TB, w, h int) *Screen {
	t.Helper()
	term := vt.NewMockTerm(vt.MockOptSize(termSize(w, h)))
	scr, err := tcell.NewTerminfoScreenFromTty(term, tcell.OptTerm("xterm-256color"))
	if err == nil {
		err = scr.Init()
	}
	if err != nil {
		t.Fatal(err)
	}
	// Fini sleeps out the first 50 ms of a screen's life, so it runs
	// beside the tests, not in them.
	t.Cleanup(func() { go scr.Fini() })
	s := &Screen{scr, term, w, h}
	s.awaitResize(t)
	return s
}

func termSize(w, h int) vt.Coord {
	return vt.Coord{X: vt.Col(max(w, 1)), Y: vt.Row(max(h, 1))}
}

// Size is the screen's size, with a dimension asked to be zero as zero.
func (s *Screen) Size() (w, h int) {
	w, h = s.Screen.Size()
	if s.w == 0 {
		w = 0
	}
	if s.h == 0 {
		h = 0
	}
	return w, h
}

// awaitResize takes tcell's resize event off the queue.
func (s *Screen) awaitResize(t testing.TB) {
	t.Helper()
	select {
	case ev := <-s.EventQ():
		if _, ok := ev.(*tcell.EventResize); !ok {
			t.Fatalf("got %T, want *tcell.EventResize", ev)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("no resize event")
	}
}

// SetTermSize makes the terminal w×h. The screen has the new size on
// return; delivering the resize event is the test's job.
func (s *Screen) SetTermSize(t testing.TB, w, h int) {
	t.Helper()
	old := termSize(s.w, s.h)
	s.w, s.h = w, h
	if termSize(w, h) == old {
		return
	}
	s.term.SetSize(termSize(w, h))
	s.awaitResize(t)
	s.Show() // tcell takes the new size up when it draws
}

// Pending reports whether an event is queued.
func (s *Screen) Pending() bool { return len(s.EventQ()) > 0 }

// Row is the text of row y as last put, trailing spaces trimmed. It
// reads tcell's cells, not the mock's, which splits clusters.
func (s *Screen) Row(y int) string {
	w, _ := s.Size()
	var sb strings.Builder
	for x := 0; x < w; {
		str, _, cw := s.Get(x, y)
		sb.WriteString(str)
		x += cw
	}
	return strings.TrimRight(sb.String(), " ")
}

// StyleAt is the style of the cell at (x, y) as last put.
func (s *Screen) StyleAt(x, y int) tcell.Style {
	_, st, _ := s.Get(x, y)
	return st
}

// Cursor is where the terminal's cursor is and whether it shows.
func (s *Screen) Cursor() (x, y int, visible bool) {
	p := s.term.Pos()
	return int(p.X), int(p.Y), s.term.Backend().GetCursor().IsVisible()
}

// Clipboard is what the terminal was last asked to copy.
func (s *Screen) Clipboard() string { return string(s.term.Backend().GetClipboard()) }

// SameStyle reports whether a and b draw alike. tcell compares a
// style's link by pointer, so == tells two equal links apart.
func SameStyle(a, b tcell.Style) bool {
	aid, aurl := a.GetUrl()
	bid, burl := b.GetUrl()
	return aid == bid && aurl == burl && a.Url("").UrlId("") == b.Url("").UrlId("")
}

// Send delivers raw as the terminal's input: the bytes a key sends.
func (s *Screen) Send(raw string) { s.term.SendRaw([]byte(raw)) }
