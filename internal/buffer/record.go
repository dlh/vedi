package buffer

import (
	"bytes"
	"io"
	"sync"
)

// A Recorder reads from r and keeps a copy of what it read, so -F can
// print the text as it came in. Stop drops the copy and keeps no more;
// it is safe to call while Fill reads.
type Recorder struct {
	r       io.Reader
	mu      sync.Mutex
	kept    bytes.Buffer
	stopped bool
}

func Record(r io.Reader) *Recorder { return &Recorder{r: r} }

func (r *Recorder) Read(p []byte) (int, error) {
	n, err := r.r.Read(p)
	r.mu.Lock()
	if !r.stopped {
		r.kept.Write(p[:n])
	}
	r.mu.Unlock()
	return n, err
}

func (r *Recorder) Stop() {
	r.mu.Lock()
	r.stopped = true
	r.kept = bytes.Buffer{}
	r.mu.Unlock()
}

// Bytes is what was read, or nil after Stop.
func (r *Recorder) Bytes() []byte {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.stopped {
		return nil
	}
	return r.kept.Bytes()
}
