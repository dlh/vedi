package buffer

import (
	"io"
	"slices"
	"sync"
)

// Source is an input that Fill reads once in order and Line reads at
// random: a file.
type Source interface {
	io.Reader
	io.ReaderAt
}

// A Concat is its parts one after another, each but the last ending
// in a newline: a part that ends mid-line is given one, so the next
// part's first line is its own; the last ends as it ends, so a text
// whose last row was not blank is known not to have been. Read walks
// the parts and learns each part's size at its EOF, so ReadAt is
// exact for every byte Read has passed, which is all a Buffer asks
// for.
type Concat struct {
	// OnPart, when set, is called with each part's index before its
	// first byte is read, from Read's goroutine.
	OnPart func(i int)

	mu    sync.Mutex
	parts []Source
	sizes []int64 // of the parts Read has finished, a given newline included
	given []bool  // which of those were given their newline
	cur   int64   // read so far from the part after those
	last  byte    // the last byte read from it; '\n' before any
	owed  bool    // the newline given to the last finished part is still to be returned
	begun int     // parts OnPart has been called for
}

func NewConcat(parts ...Source) *Concat { return &Concat{parts: parts, last: '\n'} }

func (c *Concat) Read(p []byte) (int, error) {
	if len(p) == 0 {
		return 0, nil
	}
	for {
		c.mu.Lock()
		i := len(c.sizes)
		owed := c.owed
		c.owed = false
		c.mu.Unlock()
		if owed {
			p[0] = '\n'
			return 1, nil
		}
		if i >= len(c.parts) {
			return 0, io.EOF
		}
		if i == c.begun {
			c.begun++
			if c.OnPart != nil {
				c.OnPart(i)
			}
		}
		n, err := c.parts[i].Read(p)
		c.mu.Lock()
		c.cur += int64(n)
		if n > 0 {
			c.last = p[n-1]
		}
		if err == io.EOF {
			given := i+1 < len(c.parts) && c.cur > 0 && c.last != '\n'
			if given {
				c.cur++
				if n < len(p) {
					p[n] = '\n'
					n++
				} else {
					c.owed = true
				}
			}
			c.sizes = append(c.sizes, c.cur)
			c.given = append(c.given, given)
			c.cur, c.last = 0, '\n'
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
	sizes := slices.Clip(c.sizes)
	given := slices.Clip(c.given)
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
		// The part's last byte may be the newline it was given, which
		// the part itself cannot serve.
		real := want
		if i < len(given) && given[i] && off+want == size {
			real--
		}
		if real > 0 {
			k, err := c.parts[i].ReadAt(p[n:n+int(real)], off)
			n += k
			if int64(k) < real {
				if err == nil || err == io.EOF {
					err = io.ErrUnexpectedEOF
				}
				return n, err
			}
		}
		if real < want {
			p[n] = '\n'
			n++
		}
		off = 0
	}
	if n < len(p) {
		return n, io.EOF
	}
	return n, nil
}
