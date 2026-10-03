package opencmd

import (
	"io"
	"os"
	"path/filepath"
	"testing"
	"time"
)

// TestOpenFile: %s is the file name, one argument even with a space
// in it, and the command's output is what is read.
func TestOpenFile(t *testing.T) {
	name := filepath.Join(t.TempDir(), "a b.txt")
	if err := os.WriteFile(name, []byte("hello\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	c, err := Open([]string{"cat", "%s"}, name)
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	got, err := io.ReadAll(c)
	if err != nil || string(got) != "hello\n" {
		t.Errorf("read %q, %v", got, err)
	}
}

// TestOpenInsideWord: %s is replaced inside a word too.
func TestOpenInsideWord(t *testing.T) {
	c, err := Open([]string{"echo", "file=%s"}, "f.rs")
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	got, err := io.ReadAll(c)
	if err != nil || string(got) != "file=f.rs\n" {
		t.Errorf("read %q, %v", got, err)
	}
}

// TestOpenWithoutPercent: a command without %s runs as given, and its
// stdin is empty.
func TestOpenWithoutPercent(t *testing.T) {
	c, err := Open([]string{"cat"}, "f.rs")
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	got, err := io.ReadAll(c)
	if err != nil || string(got) != "" {
		t.Errorf("read %q, %v", got, err)
	}
}

// TestOpenFails: a non-zero exit is the read error once the output
// ends, with what the command wrote to stderr.
func TestOpenFails(t *testing.T) {
	c, err := Open([]string{"sh", "-c", "echo partial; echo oops >&2; exit 3", "sh", "%s"}, "f")
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	got, err := io.ReadAll(c)
	if string(got) != "partial\n" || err == nil || err.Error() != "exit status 3: oops" {
		t.Errorf("read %q, %v", got, err)
	}
}

// TestOpenNotFound: a command that cannot start is an error from Open.
func TestOpenNotFound(t *testing.T) {
	if _, err := Open([]string{"vedi-no-such-command", "%s"}, "f"); err == nil {
		t.Error("Open succeeded")
	}
}

// TestCloseKills: Close ends a command still running, without waiting
// for it to finish on its own.
func TestCloseKills(t *testing.T) {
	c, err := Open([]string{"sleep", "10"}, "f")
	if err != nil {
		t.Fatal(err)
	}
	start := time.Now()
	c.Close()
	if d := time.Since(start); d > 2*time.Second {
		t.Errorf("Close took %v", d)
	}
}

// TestCloseKillsDescendants: a child of the command holding its
// stderr does not hold up Close either.
func TestCloseKillsDescendants(t *testing.T) {
	c, err := Open([]string{"sh", "-c", "sleep 10 & echo ready; wait"}, "f")
	if err != nil {
		t.Fatal(err)
	}
	p := make([]byte, 6)
	if _, err := io.ReadFull(c, p); err != nil || string(p) != "ready\n" {
		t.Fatalf("read %q, %v", p, err)
	}
	start := time.Now()
	c.Close()
	if d := time.Since(start); d > 2*time.Second {
		t.Errorf("Close took %v", d)
	}
}

// TestCloseWhileWaiting: a command that closes its stdout and runs on
// leaves a reader waiting for its exit; Close still ends it.
func TestCloseWhileWaiting(t *testing.T) {
	c, err := Open([]string{"sh", "-c", "echo ready; exec 1>&-; sleep 10"}, "f")
	if err != nil {
		t.Fatal(err)
	}
	p := make([]byte, 6)
	if _, err := io.ReadFull(c, p); err != nil || string(p) != "ready\n" {
		t.Fatalf("read %q, %v", p, err)
	}
	waiting := make(chan error, 1)
	go func() {
		_, err := io.ReadAll(c)
		waiting <- err
	}()
	time.Sleep(100 * time.Millisecond) // the reader is in the wait
	start := time.Now()
	c.Close()
	if d := time.Since(start); d > 2*time.Second {
		t.Errorf("Close took %v", d)
	}
	if err := <-waiting; err == nil {
		t.Error("the reader saw no error from the killed command")
	}
}

// TestCloseAfterEOF: Close after the output ended is fine, and so is
// Close twice.
func TestCloseAfterEOF(t *testing.T) {
	c, err := Open([]string{"echo", "hi"}, "f")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := io.ReadAll(c); err != nil {
		t.Fatal(err)
	}
	c.Close()
	c.Close()
}
