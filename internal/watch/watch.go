// Package watch tells when files change on disk.
package watch

import (
	"os"
	"path/filepath"
	"slices"
	"time"

	"github.com/fsnotify/fsnotify"
)

// stamp is what a check compares: a file's size and mtime.
type stamp struct {
	size  int64
	mtime time.Time
}

func (s stamp) equal(o stamp) bool { return s.size == o.size && s.mtime.Equal(o.mtime) }

func stat(name string) (stamp, bool) {
	fi, err := os.Stat(name)
	if err != nil {
		return stamp{}, false
	}
	return stamp{fi.Size(), fi.ModTime()}, true
}

type watcher struct {
	names   []string
	last    []stamp
	lost    []bool // the watch on the file is gone: it is added again
	changed func()
}

func newWatcher(names []string, changed func()) *watcher {
	w := &watcher{changed: changed}
	for _, name := range names {
		name = filepath.Clean(name)
		s, _ := stat(name)
		w.names = append(w.names, name)
		w.last = append(w.last, s)
		w.lost = append(w.lost, false)
	}
	return w
}

// Files calls changed when a named file's size or mtime differs from
// its last stat. Every file is checked every second, and on any
// fsnotify event for it; the watch is on the file, and is put back
// when an editor's rename-replace takes its inode away. Without
// fsnotify the check runs every half second. A file that cannot be
// statted keeps its last stamp: one that vanishes fires nothing, one
// that comes back fires if it came back different. stop ends the
// watch.
func Files(names []string, changed func()) (stop func()) {
	w := newWatcher(names, changed)
	every := time.Second
	fsw := w.notifier()
	if fsw == nil {
		every = 500 * time.Millisecond
	}
	done := make(chan struct{})
	go w.run(fsw, every, done)
	return func() { close(done) }
}

// notifier watches the files, or is nil when fsnotify cannot. A file
// that cannot be watched is tried again on every check.
func (w *watcher) notifier() *fsnotify.Watcher {
	fsw, err := fsnotify.NewWatcher()
	if err != nil {
		return nil
	}
	for i := range w.names {
		w.lost[i] = true
	}
	w.rewatch(fsw)
	return fsw
}

// rewatch adds every file whose watch is gone, fresh: an inode
// renamed away may still be watched under the name.
func (w *watcher) rewatch(fsw *fsnotify.Watcher) {
	for i, name := range w.names {
		if !w.lost[i] {
			continue
		}
		fsw.Remove(name)
		if fsw.Add(name) == nil {
			w.lost[i] = false
		}
	}
}

// run checks on every tick and on every event for a file, until
// done. An event that takes a file's inode away marks its watch lost.
// An fsnotify error, an overflow say, is dropped: the ticker still
// runs.
func (w *watcher) run(fsw *fsnotify.Watcher, every time.Duration, done chan struct{}) {
	var events chan fsnotify.Event
	var errs chan error
	if fsw != nil {
		defer fsw.Close()
		events, errs = fsw.Events, fsw.Errors
	}
	t := time.NewTicker(every)
	defer t.Stop()
	for {
		select {
		case <-done:
			return
		case <-t.C:
			if fsw != nil {
				w.rewatch(fsw)
			}
			w.check()
		case ev, ok := <-events:
			if !ok {
				events = nil
				break
			}
			i := slices.Index(w.names, filepath.Clean(ev.Name))
			if i < 0 {
				break
			}
			if ev.Has(fsnotify.Remove) || ev.Has(fsnotify.Rename) {
				w.lost[i] = true
			}
			w.rewatch(fsw)
			w.check()
		case _, ok := <-errs:
			if !ok {
				errs = nil
			}
		}
	}
}

// check stats every file and calls changed once if any differs.
func (w *watcher) check() {
	fired := false
	for i, name := range w.names {
		if s, ok := stat(name); ok && !s.equal(w.last[i]) {
			w.last[i] = s
			fired = true
		}
	}
	if fired {
		w.changed()
	}
}
