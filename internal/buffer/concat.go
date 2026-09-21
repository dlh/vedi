package buffer

import (
	"io"
	"sync"
)

// Source is an input that Fill reads once in order and Line reads at
// random: a file.
type Source interface {
	io.Reader
	io.ReaderAt
}

// A Concat is its parts one after another. Read walks them and learns
// each part's size at its EOF, so ReadAt is exact for every byte Read
// has passed, which is all a Buffer asks for.
type Concat struct {
	mu    sync.Mutex
	parts []Source
	sizes []int64 // of the parts Read has finished
	cur   int64   // read so far from the part after those
}

func NewConcat(parts ...Source) *Concat { return &Concat{parts: parts} }

func (c *Concat) Read(p []byte) (int, error) {
	for {
		c.mu.Lock()
		i := len(c.sizes)
		c.mu.Unlock()
		if i >= len(c.parts) {
			return 0, io.EOF
		}
		n, err := c.parts[i].Read(p)
		c.mu.Lock()
		c.cur += int64(n)
		if err == io.EOF {
			c.sizes = append(c.sizes, c.cur)
			c.cur = 0
			err = nil
		}
		c.mu.Unlock()
		if n > 0 || err != nil {
			return n, err
		}
	}
}

// ReadAt reads from the parts Read has finished and the one it is on.
func (c *Concat) ReadAt(p []byte, off int64) (int, error) {
	c.mu.Lock()
	sizes := c.sizes[:len(c.sizes):len(c.sizes)]
	if len(sizes) < len(c.parts) {
		sizes = append(sizes, c.cur)
	}
	c.mu.Unlock()
	n := 0
	for i, size := range sizes {
		if n == len(p) {
			break
		}
		if off >= size {
			off -= size
			continue
		}
		want := min(int64(len(p)-n), size-off)
		k, err := c.parts[i].ReadAt(p[n:n+int(want)], off)
		n += k
		if int64(k) < want {
			if err == nil || err == io.EOF {
				err = io.ErrUnexpectedEOF
			}
			return n, err
		}
		off = 0
	}
	if n < len(p) {
		return n, io.EOF
	}
	return n, nil
}
