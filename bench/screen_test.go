package main

import (
	"strings"
	"testing"
)

func TestScreenText(t *testing.T) {
	for _, tc := range []struct{ in, want string }{
		{"\x1b[31mred\x1b[0m", "red"},
		{"\x1b]8;;http://x\x07link\x1b]8;;\x1b\\", "link"},
		{"\x1b(Bplain", "plain"},
		{"a\r\nb", "a\nb"},
		{"caf\xc3\xa9", "café"},
		{"\x1b[2;3Hx", "\n  x"},
		{"abc\x1b[2D\x1b[K", "a"},
	} {
		s := newScreen(3, 10)
		s.write([]byte(tc.in))
		if got := strings.TrimRight(s.String(), "\n"); got != tc.want {
			t.Errorf("write(%q) = %q, want %q", tc.in, got, tc.want)
		}
	}
}

// TestScreenSplit: a sequence, or a rune, cut by a read boundary
// leaks nothing.
func TestScreenSplit(t *testing.T) {
	s := newScreen(1, 10)
	s.write([]byte("\x1b[3"))
	s.write([]byte("1mr\xc3"))
	s.write([]byte("\xa9d"))
	if got := strings.TrimRight(s.String(), " \n"); got != "réd" {
		t.Errorf("got %q, want %q", got, "réd")
	}
}

// TestScreenRedraw: a pager that rewrites only the changed cells still
// leaves the whole line on screen.
func TestScreenRedraw(t *testing.T) {
	s := newScreen(2, 12)
	s.write([]byte("\x1b[1;1H00004979:xx"))
	s.write([]byte("\x1b[1;5H1"))
	if !s.contains("00001979:xx") {
		t.Errorf("screen:\n%s", s)
	}
}

// TestScreenScroll: a line feed on the last row scrolls, and the
// scroll region holds it.
func TestScreenScroll(t *testing.T) {
	s := newScreen(3, 4)
	s.write([]byte("a\r\nb\r\nc\r\nd"))
	if got := s.String(); got != "b\nc\nd\n" {
		t.Errorf("scroll: got %q", got)
	}
	s.write([]byte("\x1b[1;2r\x1b[2;1H\ne"))
	if got := s.String(); got != "c\ne\nd\n" {
		t.Errorf("region: got %q", got)
	}
	s.write([]byte("\x1b[r\x1b[1;1H\x1b[M"))
	if got := s.String(); got != "e\nd\n\n" {
		t.Errorf("delete line: got %q", got)
	}
}

// TestScreenDA1: a primary device attributes request is reported once,
// and a private-mode query is not one.
func TestScreenDA1(t *testing.T) {
	s := newScreen(1, 4)
	s.write([]byte("\x1b[?u\x1b[>q"))
	if s.askedDA1() {
		t.Error("private query taken for DA1")
	}
	s.write([]byte("\x1b[c"))
	if !s.askedDA1() || s.askedDA1() {
		t.Error("DA1 not reported exactly once")
	}
}
