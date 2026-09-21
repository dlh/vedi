package main

import "testing"

func TestStrip(t *testing.T) {
	for _, tc := range []struct{ in, want string }{
		{"\x1b[31mred\x1b[0m", "red"},
		{"\x1b]8;;http://x\x07link\x1b]8;;\x1b\\", "link"},
		{"\x1b(Bplain", "plain"},
		{"a\r\nb", "a\r\nb"},
	} {
		var s stripper
		if got := string(s.strip(nil, []byte(tc.in))); got != tc.want {
			t.Errorf("strip(%q) = %q, want %q", tc.in, got, tc.want)
		}
	}
}

// TestStripSplit: a sequence cut by a read boundary leaks nothing.
func TestStripSplit(t *testing.T) {
	var s stripper
	out := s.strip(nil, []byte("\x1b[3"))
	out = s.strip(out, []byte("1mred"))
	if string(out) != "red" {
		t.Errorf("got %q, want %q", out, "red")
	}
}
