package app

import (
	"sync/atomic"

	"github.com/gdamore/tcell/v3"
	"go.dlh.dev/vedi/internal/buffer"
)

// Changed is the watcher's event: a file being viewed changed on
// disk.
type Changed struct{ tcell.EventTime }

// reloadNotify is the notify a reload's reader gets: notify for the
// buffer Open returned, set once Open has. A call before then counts
// as before EOF and is held at most redrawEvery.
type reloadNotify struct {
	a   *App
	buf atomic.Pointer[buffer.Buffer]
}

func (n *reloadNotify) notify() { n.a.notify(n.buf.Load()) }

// reload reads the input again, for the reload key or a change on
// disk. One reload runs at a time; a change during one asks for
// another after it. Only the key reports that it cannot.
func (a *App) reload(byKey bool) {
	if a.open == nil {
		if byKey {
			a.status = "not a file"
		}
		return
	}
	if a.next != nil {
		a.dirty = true
		return
	}
	n := &reloadNotify{a: a}
	buf, closeBuf, err := a.open(n.notify)
	if err != nil {
		if byKey {
			a.status = "reload failed: " + err.Error()
		}
		return
	}
	n.buf.Store(buf)
	a.next, a.closeNext = buf, closeBuf
	a.swapIfDone()
}

// swapIfDone puts a finished reload in the text's place, and starts
// the reload a change asked for meanwhile.
func (a *App) swapIfDone() {
	if a.next == nil {
		return
	}
	if eof, _ := a.next.Finished(); !eof {
		return
	}
	buf, closeBuf := a.next, a.closeNext
	a.next, a.closeNext = nil, nil
	a.swap(buf, closeBuf)
	if a.dirty {
		a.dirty = false
		a.reload(false)
	}
}

// swap puts buf under the view, or under the view help set aside,
// and closes the old buffer's files.
func (a *App) swap(buf *buffer.Buffer, closeBuf func()) {
	if a.helping {
		a.text.reload(buf)
	} else {
		t := textView{a.buf, a.cur, a.top, a.anchor, a.xoff, a.mode, a.matcher, a.backward, a.highlight}
		t.reload(buf)
		a.buf, a.cur, a.top, a.anchor = t.buf, t.cur, t.top, t.anchor
		a.laidOut = nil
		a.scrollToCursor()
	}
	if a.closeBuf != nil {
		a.closeBuf()
	}
	a.closeBuf = closeBuf
}

// reload puts buf in the view's place, keeping the cursor's place in
// the text: a cursor on the last line goes to the new last line, and
// every position is clamped, the line to the last, the column to the
// line's end. A top past the new end starts over from the first row,
// so a text shorter than the screen fills it from the top; scrolling
// to the cursor then places it.
func (t *textView) reload(buf *buffer.Buffer) {
	atEnd := t.cur.Line >= t.buf.Len()-1
	t.buf = buf
	if atEnd {
		t.cur.Line = buf.Len() - 1
	}
	t.cur = t.clamp(t.cur)
	switch {
	case t.top.Line >= buf.Len():
		t.top = buffer.Pos{Col: -1}
	case t.top.Col >= 0:
		t.top = t.clamp(t.top)
	}
	// A separator row stays one: snap drops it if the line no longer
	// starts an input.
	if t.anchor != nil {
		p := t.clamp(*t.anchor)
		t.anchor = &p
	}
}

func (t *textView) clamp(p buffer.Pos) buffer.Pos {
	n := t.buf.Len()
	if n == 0 {
		return buffer.Pos{}
	}
	p.Line = max(0, min(p.Line, n-1))
	p.Col = max(0, min(p.Col, len(t.buf.Line(p.Line).Text)))
	return p
}
