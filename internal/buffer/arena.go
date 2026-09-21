package buffer

import (
	"io"
	"sync"
)

// chunkSize is the arena's chunk, and every chunk but the last is full,
// so an offset's chunk is a shift away.
const (
	chunkShift = 18
	chunkSize  = 1 << chunkShift
)

// An arena holds bytes in fixed chunks: Write copies once, ReadAt
// serves any offset written so far.
type arena struct {
	mu     sync.Mutex
	chunks [][]byte
	n      int64
}

func (a *arena) Write(p []byte) {
	a.mu.Lock()
	defer a.mu.Unlock()
	for len(p) > 0 {
		if len(a.chunks) == 0 || len(a.chunks[len(a.chunks)-1]) == chunkSize {
			a.chunks = append(a.chunks, make([]byte, 0, chunkSize))
		}
		last := &a.chunks[len(a.chunks)-1]
		k := copy((*last)[len(*last):chunkSize], p)
		*last = (*last)[:len(*last)+k]
		p = p[k:]
		a.n += int64(k)
	}
}

// ReadAt implements io.ReaderAt over the bytes written so far.
func (a *arena) ReadAt(p []byte, off int64) (int, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	n := 0
	for n < len(p) && off >= 0 && off < a.n {
		c := a.chunks[off>>chunkShift]
		k := copy(p[n:], c[off&(chunkSize-1):])
		n += k
		off += int64(k)
	}
	if n < len(p) {
		return n, io.EOF
	}
	return n, nil
}
