package buffer

import (
	"bytes"
	"io"
	"testing"
)

// TestArenaReadAtSpansChunks: reads cross chunk boundaries, stop at
// the end with io.EOF, and see bytes written in any number of pieces.
func TestArenaReadAtSpansChunks(t *testing.T) {
	var a arena
	want := bytes.Repeat([]byte("0123456789"), chunkSize/4)
	a.Write(want[:chunkSize-3])
	a.Write(want[chunkSize-3 : chunkSize+3])
	a.Write(want[chunkSize+3:])
	got := make([]byte, len(want))
	if n, err := a.ReadAt(got, 0); n != len(want) || err != nil || !bytes.Equal(got, want) {
		t.Fatalf("ReadAt(all) = %d, %v", n, err)
	}
	p := make([]byte, 6)
	if n, err := a.ReadAt(p, chunkSize-3); n != 6 || err != nil || string(p) != string(want[chunkSize-3:chunkSize+3]) {
		t.Fatalf("ReadAt across chunk = %q, %d, %v", p, n, err)
	}
	if n, err := a.ReadAt(p, int64(len(want))-2); n != 2 || err != io.EOF {
		t.Fatalf("ReadAt at end = %d, %v; want 2, EOF", n, err)
	}
	if n, err := a.ReadAt(p, int64(len(want))+1); n != 0 || err != io.EOF {
		t.Fatalf("ReadAt past end = %d, %v; want 0, EOF", n, err)
	}
}
