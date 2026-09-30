package main

import (
	"io"
	"os"
	"path/filepath"
	"strings"
	"syscall"
	"testing"
	"time"

	"go.dlh.dev/vedi/internal/buffer"
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

// onePageInput is a buffer filled from a pipe the way main fills it for
// -F: the reader notifies data, until the app takes over.
func onePageInput(t *testing.T) (*buffer.Buffer, *io.PipeWriter, chan struct{}) {
	t.Helper()
	pr, pw := io.Pipe()
	t.Cleanup(func() { pw.Close() })
	buf := buffer.New()
	data := make(chan struct{}, 1)
	n := new(notifier)
	n.set(func() {
		select {
		case data <- struct{}{}:
		default:
		}
	})
	go buffer.Fill(pr, buf, n.notify)
	return buf, pw, data
}

// fixed is a terminal size that never changes.
func fixed(w, h int) func() (int, int) { return func() (int, int) { return w, h } }

// TestWaitOnePagePrintsAtEOF: -F waits for the input to end, and text
// that fits is then printed.
func TestWaitOnePagePrintsAtEOF(t *testing.T) {
	buf, pw, data := onePageInput(t)
	go func() {
		pw.Write([]byte("hello\n"))
		pw.Close()
	}()
	if !waitOnePage(buf, data, nil, fixed(40, 3)) {
		t.Error("text that fits is paged, not printed")
	}
}

// TestWaitOnePagePagesBeforeEOF: text that outgrows the screen is paged
// as soon as it does, with the input still open.
func TestWaitOnePagePagesBeforeEOF(t *testing.T) {
	buf, pw, data := onePageInput(t)
	go pw.Write([]byte("1\n2\n3\n"))
	if waitOnePage(buf, data, nil, fixed(40, 3)) {
		t.Error("text that outgrew the screen is printed")
	}
}

// TestWaitOnePageResizes: a terminal that shrinks while -F waits is
// measured again at its new size, without waiting for more input:
// eight lines fit ten rows and not three.
func TestWaitOnePageResizes(t *testing.T) {
	buf, pw, data := onePageInput(t)
	sizes := make(chan int) // the rows for each decision, so the test sees each one made
	resize := make(chan os.Signal, 1)
	result := make(chan bool, 1)
	go func() {
		result <- waitOnePage(buf, data, resize, func() (int, int) { return 40, <-sizes })
	}()
	sizes <- 10 // nothing read yet
	pw.Write([]byte(strings.Repeat("line\n", 8)))
	sizes <- 10 // eight lines fit ten rows: still waiting
	resize <- syscall.SIGWINCH
	select {
	case sizes <- 3:
	case <-time.After(2 * time.Second):
		t.Fatal("the resize did not wake the wait")
	}
	if <-result {
		t.Error("eight lines on three rows are printed")
	}
}

// TestNotifierChangesHands: a notify set later is the one called.
func TestNotifierChangesHands(t *testing.T) {
	n := new(notifier)
	got := ""
	n.set(func() { got = "first" })
	n.set(func() { got = "second" })
	n.notify()
	if got != "second" {
		t.Errorf("notified %q, want second", got)
	}
}
