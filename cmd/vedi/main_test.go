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
	in, paged, onDisk, closeInput, err := openInput([]string{pipe}, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer closeInput()
	if paged || onDisk {
		t.Errorf("paged %v, onDisk %v for a pipe", paged, onDisk)
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
	_, paged, onDisk, closeInput, err := openInput([]string{name}, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer closeInput()
	if !paged || !onDisk {
		t.Errorf("paged %v, onDisk %v for a regular file", paged, onDisk)
	}
}

// TestOpenInputCmd: with an open command a file is read through it,
// so its text is kept in memory, but it is still on disk to watch and
// read again.
func TestOpenInputCmd(t *testing.T) {
	name := filepath.Join(t.TempDir(), "f")
	if err := os.WriteFile(name, []byte("hello\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	in, paged, onDisk, closeInput, err := openInput([]string{name}, []string{"sed", "s/l/L/g", "%s"})
	if err != nil {
		t.Fatal(err)
	}
	defer closeInput()
	if paged || !onDisk {
		t.Errorf("paged %v, onDisk %v for a file read through a command", paged, onDisk)
	}
	got, err := io.ReadAll(in)
	if err != nil || string(got) != "heLLo\n" {
		t.Errorf("read %q, %v", got, err)
	}
}

// TestOpenInputCmdStdin: the open command is for files; stdin is read
// as it is.
func TestOpenInputCmdStdin(t *testing.T) {
	r, w, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	defer r.Close()
	stdin := os.Stdin
	os.Stdin = r
	t.Cleanup(func() { os.Stdin = stdin })
	go func() {
		w.Write([]byte("hello\n"))
		w.Close()
	}()
	in, _, onDisk, closeInput, err := openInput(nil, []string{"cat", "-n", "%s"})
	if err != nil {
		t.Fatal(err)
	}
	defer closeInput()
	if onDisk {
		t.Error("onDisk for stdin")
	}
	got, err := io.ReadAll(in)
	if err != nil || string(got) != "hello\n" {
		t.Errorf("read %q, %v", got, err)
	}
}

// TestOpenInputCmdFIFO: a named pipe is left for the command to open:
// opened twice, the second reader would wait for a writer that has
// been and gone.
func TestOpenInputCmdFIFO(t *testing.T) {
	pipe := filepath.Join(t.TempDir(), "pipe")
	if err := syscall.Mkfifo(pipe, 0o600); err != nil {
		t.Fatal(err)
	}
	go func() { os.WriteFile(pipe, []byte("hello\n"), 0) }()
	type result struct {
		got []byte
		err error
	}
	done := make(chan result, 1)
	go func() {
		in, _, onDisk, closeInput, err := openInput([]string{pipe}, []string{"cat", "%s"})
		if err != nil {
			done <- result{nil, err}
			return
		}
		defer closeInput()
		if onDisk {
			t.Error("onDisk for a pipe")
		}
		got, err := io.ReadAll(in)
		done <- result{got, err}
	}()
	select {
	case r := <-done:
		if r.err != nil || string(r.got) != "hello\n" {
			t.Errorf("read %q, %v", r.got, r.err)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("reading the pipe through the command hung")
	}
}

// TestOpenInputCmdMissingFile: a file that is not there is the error,
// before any command runs.
func TestOpenInputCmdMissingFile(t *testing.T) {
	name := filepath.Join(t.TempDir(), "missing")
	_, _, _, _, err := openInput([]string{name}, []string{"cat", "%s"})
	if !os.IsNotExist(err) {
		t.Errorf("err = %v, want not exist", err)
	}
}

// TestOpenInputCmdNotFound: a command that cannot start is the error.
func TestOpenInputCmdNotFound(t *testing.T) {
	name := filepath.Join(t.TempDir(), "f")
	if err := os.WriteFile(name, nil, 0o644); err != nil {
		t.Fatal(err)
	}
	if _, _, _, _, err := openInput([]string{name}, []string{"vedi-no-such-command", "%s"}); err == nil {
		t.Error("openInput succeeded")
	}
}

// TestOpenParts: each file is a part of the buffer, so the app can
// name the one a line came from.
func TestOpenParts(t *testing.T) {
	dir := t.TempDir()
	var names []string
	for i, text := range []string{"a\n", "b\nc\n"} {
		name := filepath.Join(dir, string(rune('a'+i)))
		if err := os.WriteFile(name, []byte(text), 0o644); err != nil {
			t.Fatal(err)
		}
		names = append(names, name)
	}
	buf, closeInput, _, _, err := open(names, nil, func() {})
	if err != nil {
		t.Fatal(err)
	}
	defer closeInput()
	for eof := false; !eof; eof, _ = buf.Finished() {
		time.Sleep(time.Millisecond)
	}
	for line, want := range []int{0, 1, 1} {
		if got := buf.PartAt(line); got != want {
			t.Errorf("PartAt(%d) = %d, want %d", line, got, want)
		}
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
	if !waitOnePage(buf, data, nil, fixed(40, 3), false) {
		t.Error("text that fits is paged, not printed")
	}
}

// TestWaitOnePagePagesBeforeEOF: text that outgrows the screen is paged
// as soon as it does, with the input still open.
func TestWaitOnePagePagesBeforeEOF(t *testing.T) {
	buf, pw, data := onePageInput(t)
	go pw.Write([]byte("1\n2\n3\n"))
	if waitOnePage(buf, data, nil, fixed(40, 3), false) {
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
		result <- waitOnePage(buf, data, resize, func() (int, int) { return 40, <-sizes }, false)
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
