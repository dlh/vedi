package watch

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

// start watches names and returns a channel that gets a value per
// change; the callback never blocks.
func start(t *testing.T, names ...string) chan struct{} {
	t.Helper()
	ch := make(chan struct{}, 16)
	stop := Files(names, func() {
		select {
		case ch <- struct{}{}:
		default:
		}
	})
	t.Cleanup(stop)
	return ch
}

func fired(ch chan struct{}) bool {
	select {
	case <-ch:
		return true
	case <-time.After(3 * time.Second):
		return false
	}
}

// quiet reports that nothing fires for longer than the slowest check.
func quiet(ch chan struct{}) bool {
	select {
	case <-ch:
		return false
	case <-time.After(1200 * time.Millisecond):
		return true
	}
}

func write(t *testing.T, name, text string) {
	t.Helper()
	if err := os.WriteFile(name, []byte(text), 0o644); err != nil {
		t.Fatal(err)
	}
}

// age moves a file's mtime an hour back. Linux stamps mtimes from a
// coarse clock, so a same-sized file written over it moments later
// would otherwise get the same stamp.
func age(t *testing.T, name string) {
	t.Helper()
	earlier := time.Now().Add(-time.Hour)
	if err := os.Chtimes(name, earlier, earlier); err != nil {
		t.Fatal(err)
	}
}

func TestWriteFires(t *testing.T) {
	name := filepath.Join(t.TempDir(), "f")
	write(t, name, "a\n")
	ch := start(t, name)
	write(t, name, "ab\n")
	if !fired(ch) {
		t.Fatal("a write did not fire")
	}
}

func TestRenameReplaceFires(t *testing.T) {
	dir := t.TempDir()
	name := filepath.Join(dir, "f")
	write(t, name, "a\n")
	age(t, name)
	ch := start(t, name)
	tmp := filepath.Join(dir, "f.tmp")
	write(t, tmp, "b\n")
	if err := os.Rename(tmp, name); err != nil {
		t.Fatal(err)
	}
	if !fired(ch) {
		t.Fatal("a rename over the file did not fire")
	}
}

func TestTouchFires(t *testing.T) {
	name := filepath.Join(t.TempDir(), "f")
	write(t, name, "a\n")
	ch := start(t, name)
	later := time.Now().Add(time.Hour)
	if err := os.Chtimes(name, later, later); err != nil {
		t.Fatal(err)
	}
	if !fired(ch) {
		t.Fatal("a changed mtime did not fire")
	}
}

func TestVanishFiresNothing(t *testing.T) {
	name := filepath.Join(t.TempDir(), "f")
	write(t, name, "a\n")
	ch := start(t, name)
	if err := os.Remove(name); err != nil {
		t.Fatal(err)
	}
	if !quiet(ch) {
		t.Fatal("removing the file fired")
	}
	write(t, name, "abc\n")
	if !fired(ch) {
		t.Fatal("the file coming back different did not fire")
	}
}

func TestTwoFilesOneDir(t *testing.T) {
	dir := t.TempDir()
	a, b := filepath.Join(dir, "a"), filepath.Join(dir, "b")
	write(t, a, "a\n")
	write(t, b, "b\n")
	ch := start(t, a, b)
	write(t, b, "bb\n")
	if !fired(ch) {
		t.Fatal("a write to the second file did not fire")
	}
}

func TestStopStops(t *testing.T) {
	name := filepath.Join(t.TempDir(), "f")
	write(t, name, "a\n")
	ch := make(chan struct{}, 16)
	stop := Files([]string{name}, func() { ch <- struct{}{} })
	stop()
	write(t, name, "ab\n")
	if !quiet(ch) {
		t.Fatal("fired after stop")
	}
}

// TestPollingAlone: the ticker finds a change with no fsnotify
// watcher, as when a directory cannot be watched.
func TestPollingAlone(t *testing.T) {
	name := filepath.Join(t.TempDir(), "f")
	write(t, name, "a\n")
	ch := make(chan struct{}, 16)
	w := newWatcher([]string{name}, func() { ch <- struct{}{} })
	done := make(chan struct{})
	t.Cleanup(func() { close(done) })
	go w.run(nil, 10*time.Millisecond, done)
	write(t, name, "ab\n")
	if !fired(ch) {
		t.Fatal("the ticker did not find the write")
	}
}

// TestFilesReturnsAtOnce: watching a file in a big directory must not
// hold up the caller, as watching the directory would on kqueue.
func TestFilesReturnsAtOnce(t *testing.T) {
	dir := t.TempDir()
	for i := range 4000 {
		write(t, filepath.Join(dir, "f"+string(rune('a'+i%26))+string(rune('a'+i/26%26))+string(rune('a'+i/676))), "")
	}
	name := filepath.Join(dir, "f")
	write(t, name, "a\n")
	before := time.Now()
	stop := Files([]string{name}, func() {})
	took := time.Since(before)
	stop()
	if took > 20*time.Millisecond {
		t.Fatalf("Files took %v", took)
	}
}

// openFDs is how many file descriptors the process holds.
func openFDs(t *testing.T) int {
	t.Helper()
	ents, err := os.ReadDir("/dev/fd")
	if err != nil {
		t.Skip("no /dev/fd")
	}
	return len(ents)
}

// TestWatchesTheFileAlone: the watch is on the file, one descriptor,
// not on its directory, which on kqueue would open every sibling.
func TestWatchesTheFileAlone(t *testing.T) {
	dir := t.TempDir()
	for i := range 200 {
		write(t, filepath.Join(dir, "sibling"+string(rune('a'+i%26))+string(rune('a'+i/26))), "")
	}
	name := filepath.Join(dir, "f")
	write(t, name, "a\n")
	before := openFDs(t)
	ch := start(t, name)
	write(t, name, "ab\n")
	if !fired(ch) {
		t.Fatal("a write did not fire")
	}
	if got := openFDs(t) - before; got > 4 {
		t.Fatalf("watching one file opened %d descriptors", got)
	}
}

// TestRenameReplaceRewatches: after an editor's rename-replace the new
// file is watched too, so its next write is seen by an event, not by
// the ticker, which here would take an hour.
func TestRenameReplaceRewatches(t *testing.T) {
	dir := t.TempDir()
	name := filepath.Join(dir, "f")
	write(t, name, "a\n")
	age(t, name)
	ch := make(chan struct{}, 16)
	w := newWatcher([]string{name}, func() { ch <- struct{}{} })
	fsw := w.notifier()
	if fsw == nil {
		t.Skip("no fsnotify")
	}
	done := make(chan struct{})
	t.Cleanup(func() { close(done) })
	go w.run(fsw, time.Hour, done)
	tmp := filepath.Join(dir, "f.tmp")
	write(t, tmp, "b\n")
	if err := os.Rename(tmp, name); err != nil {
		t.Fatal(err)
	}
	if !fired(ch) {
		t.Fatal("the rename over the file did not fire")
	}
	write(t, name, "bbb\n")
	if !fired(ch) {
		t.Fatal("a write to the new file did not fire")
	}
}

// TestStopWaitsForCheck: stop returns only once a check under way is
// over, so nothing fires after it.
func TestStopWaitsForCheck(t *testing.T) {
	name := filepath.Join(t.TempDir(), "f")
	write(t, name, "a\n")
	age(t, name)
	entered := make(chan struct{})
	release := make(chan struct{})
	stop := Files([]string{name}, func() {
		close(entered)
		<-release
	})
	write(t, name, "ab\n")
	select {
	case <-entered:
	case <-time.After(3 * time.Second):
		t.Fatal("a write did not fire")
	}
	stopped := make(chan struct{})
	go func() {
		stop()
		close(stopped)
	}()
	select {
	case <-stopped:
		t.Fatal("stop returned during a check")
	case <-time.After(100 * time.Millisecond):
	}
	close(release)
	select {
	case <-stopped:
	case <-time.After(3 * time.Second):
		t.Fatal("stop did not return after the check")
	}
}
