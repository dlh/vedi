// Package app holds the pager state and drives it from key events.
package app

import (
	"fmt"
	"strings"
	"sync/atomic"
	"unicode"

	"github.com/gdamore/tcell/v2"
	"go.dlh.dev/vedi/internal/buffer"
	"go.dlh.dev/vedi/internal/clipboard"
	"go.dlh.dev/vedi/internal/input"
	"go.dlh.dev/vedi/internal/layout"
	"go.dlh.dev/vedi/internal/search"
)

type Options struct {
	Mode      layout.Mode
	StartLine int  // 1-based line to put at the top; 0 for none
	Follow    bool // keep the cursor on the last line until EOF (+G)
	Copier    clipboard.Copier
}

type App struct {
	scr    tcell.Screen
	buf    *buffer.Buffer
	copier clipboard.Copier
	mode   layout.Mode

	cur    buffer.Pos
	anchor *buffer.Pos // selection anchor; nil when there is no selection
	top    buffer.Pos  // first visible row: a line and the start of one of its segments
	xoff   int         // horizontal scroll in NoWrap mode

	follow    bool
	startLine int // 0-based +N target; -1 once applied

	status  string // one-shot message, cleared on the next key
	readErr string

	searching bool // the / prompt is open
	query     []rune
	matcher   search.Matcher
	highlight bool

	pending atomic.Bool
}

func New(scr tcell.Screen, buf *buffer.Buffer, opts Options) *App {
	a := &App{
		scr:       scr,
		buf:       buf,
		copier:    opts.Copier,
		mode:      opts.Mode,
		follow:    opts.Follow,
		startLine: opts.StartLine - 1,
	}
	if opts.StartLine <= 0 {
		a.startLine = -1
	}
	return a
}

// Notify asks for a redraw. It is safe to call from the reader
// goroutine; repeated calls before the next draw are coalesced.
func (a *App) Notify() {
	if a.pending.CompareAndSwap(false, true) {
		if a.scr.PostEvent(tcell.NewEventInterrupt(nil)) != nil {
			a.pending.Store(false) // queue full; the next Notify retries
		}
	}
}

// Run draws and handles events until an action quits.
func (a *App) Run() {
	a.Draw()
	for {
		if a.Handle(a.scr.PollEvent()) {
			return
		}
		a.Draw()
	}
}

// Handle processes one event and reports whether to quit.
func (a *App) Handle(ev tcell.Event) bool {
	switch ev := ev.(type) {
	case *tcell.EventResize:
		a.scr.Sync()
		a.scrollToCursor()
	case *tcell.EventInterrupt:
		a.pending.Store(false)
		a.onData()
	case *tcell.EventKey:
		a.follow = false
		a.startLine = -1
		a.status = ""
		if a.searching {
			a.handleSearchKey(ev)
			return false
		}
		return a.handleKey(input.Decode(ev))
	}
	return false
}

// onData runs after the reader appended lines: it applies a pending +N,
// follows the end for +G, and records a read error.
func (a *App) onData() {
	eof, err := a.buf.Finished()
	if err != nil {
		a.readErr = "read error: " + err.Error()
	}
	n := a.buf.Len()
	if a.startLine >= 0 && (n > a.startLine || eof) {
		a.cur = buffer.Pos{Line: min(a.startLine, max(n-1, 0))}
		a.top = a.cur
		a.startLine = -1
	}
	if a.follow {
		a.cur = buffer.Pos{Line: max(n-1, 0)}
	}
	a.scrollToCursor()
}

func (a *App) handleKey(c input.Command) bool {
	if input.IsMovement(c.Action) {
		if c.Extend {
			if a.anchor == nil {
				p := a.cur
				a.anchor = &p
			}
		} else {
			a.anchor = nil
		}
	}
	switch c.Action {
	case input.Up:
		a.moveRows(-1)
	case input.Down:
		a.moveRows(1)
	case input.Left:
		a.moveCol(-1)
	case input.Right:
		a.moveCol(1)
	case input.Home:
		a.cur.Col = 0
	case input.End:
		a.cur.Col = len(a.line(a.cur.Line))
	case input.PageUp:
		a.moveRows(-a.pageRows())
	case input.PageDown:
		a.moveRows(a.pageRows())
	case input.WordLeft:
		a.wordLeft()
	case input.WordRight:
		a.wordRight()
	case input.First:
		a.cur = buffer.Pos{}
	case input.Last:
		a.cur = buffer.Pos{Line: max(a.buf.Len()-1, 0)}
	case input.SelectAll:
		a.anchor = &buffer.Pos{}
		a.cur = a.endPos()
	case input.ClearSelection:
		if a.anchor != nil {
			a.anchor = nil
		} else {
			a.highlight = false
		}
	case input.Copy:
		a.copy()
	case input.Enter:
		if a.anchor != nil {
			return a.copy()
		}
		a.moveRows(1)
	case input.Search:
		a.searching = true
		a.query = a.query[:0]
	case input.SearchNext:
		a.findNext(true)
	case input.SearchPrev:
		a.findPrev()
	case input.ToggleWrap:
		if a.mode == layout.Wrap {
			a.mode = layout.NoWrap
		} else {
			a.mode = layout.Wrap
		}
		a.xoff = 0
	case input.Quit:
		return true
	}
	a.scrollToCursor()
	return false
}

func (a *App) line(i int) []rune { return a.buf.Line(i).Text }

func (a *App) layout() layout.Layout {
	w, _ := a.scr.Size()
	return layout.Layout{Width: w, Mode: a.mode}
}

// textRows is the rows left for text: all but the status line, which
// needs two rows to exist.
func (a *App) textRows() int {
	_, h := a.scr.Size()
	if h >= 2 {
		return h - 1
	}
	return h
}

func (a *App) pageRows() int { return max(a.textRows(), 1) }

func (a *App) endPos() buffer.Pos {
	n := a.buf.Len()
	if n == 0 {
		return buffer.Pos{}
	}
	return buffer.Pos{Line: n - 1, Col: len(a.line(n - 1))}
}

// moveRows moves the cursor n visual rows (negative is up), keeping its
// cell column where possible.
func (a *App) moveRows(n int) {
	l := a.layout()
	text := a.line(a.cur.Line)
	row, x := l.Pos(text, a.cur.Col)
	for n > 0 {
		if row+1 < l.Rows(text) {
			row++
		} else if a.cur.Line+1 < a.buf.Len() {
			a.cur.Line++
			text = a.line(a.cur.Line)
			row = 0
		} else {
			break
		}
		n--
	}
	for n < 0 {
		if row > 0 {
			row--
		} else if a.cur.Line > 0 {
			a.cur.Line--
			text = a.line(a.cur.Line)
			row = l.Rows(text) - 1
		} else {
			break
		}
		n++
	}
	a.cur.Col = l.Col(text, row, x)
}

// moveCol moves one rune left or right, crossing line ends.
func (a *App) moveCol(d int) {
	text := a.line(a.cur.Line)
	switch {
	case d < 0 && a.cur.Col > 0:
		a.cur.Col--
	case d < 0 && a.cur.Line > 0:
		a.cur.Line--
		a.cur.Col = len(a.line(a.cur.Line))
	case d > 0 && a.cur.Col < len(text):
		a.cur.Col++
	case d > 0 && a.cur.Line+1 < a.buf.Len():
		a.cur.Line++
		a.cur.Col = 0
	}
}

func isWord(r rune) bool { return unicode.IsLetter(r) || unicode.IsDigit(r) || r == '_' }

// wordRight moves to the end of the current or next word.
func (a *App) wordRight() {
	text := a.line(a.cur.Line)
	if a.cur.Col >= len(text) {
		a.moveCol(1)
		return
	}
	i := a.cur.Col
	for i < len(text) && !isWord(text[i]) {
		i++
	}
	for i < len(text) && isWord(text[i]) {
		i++
	}
	a.cur.Col = i
}

// wordLeft moves to the start of the current or previous word.
func (a *App) wordLeft() {
	if a.cur.Col == 0 {
		a.moveCol(-1)
		return
	}
	text := a.line(a.cur.Line)
	i := min(a.cur.Col, len(text))
	for i > 0 && !isWord(text[i-1]) {
		i--
	}
	for i > 0 && isWord(text[i-1]) {
		i--
	}
	a.cur.Col = i
}

// selection returns the ordered selection bounds; ok is false when
// there is no anchor or it equals the cursor.
func (a *App) selection() (start, end buffer.Pos, ok bool) {
	if a.anchor == nil || *a.anchor == a.cur {
		return start, end, false
	}
	start, end = *a.anchor, a.cur
	if end.Less(start) {
		start, end = end, start
	}
	return start, end, true
}

// selectedText is the selected runes, lines joined with newlines.
func (a *App) selectedText(start, end buffer.Pos) string {
	var sb strings.Builder
	for li := start.Line; li <= end.Line; li++ {
		text := a.line(li)
		from, to := 0, len(text)
		if li == start.Line {
			from = min(start.Col, len(text))
		}
		if li == end.Line {
			to = min(end.Col, len(text))
		}
		if li > start.Line {
			sb.WriteByte('\n')
		}
		sb.WriteString(string(text[from:to]))
	}
	return sb.String()
}

// copy sends the selection to the clipboard and reports success. The
// selection stays either way.
func (a *App) copy() bool {
	start, end, ok := a.selection()
	if !ok {
		a.status = "nothing selected"
		return false
	}
	if err := a.copier.Copy(a.selectedText(start, end)); err != nil {
		a.status = "copy failed: " + err.Error()
		return false
	}
	n := end.Line - start.Line + 1
	if end.Col == 0 && end.Line > start.Line {
		n--
	}
	if n == 1 {
		a.status = "copied 1 line"
	} else {
		a.status = fmt.Sprintf("copied %d lines", n)
	}
	return true
}

func (a *App) handleSearchKey(ev *tcell.EventKey) {
	switch ev.Key() {
	case tcell.KeyEscape:
		a.searching = false
	case tcell.KeyEnter:
		a.searching = false
		a.matcher = search.New(string(a.query))
		a.findNext(false)
	case tcell.KeyBackspace, tcell.KeyBackspace2:
		if len(a.query) > 0 {
			a.query = a.query[:len(a.query)-1]
		}
	case tcell.KeyRune:
		a.query = append(a.query, ev.Rune())
	}
}

func (a *App) findNext(after bool) {
	if a.matcher.Empty() {
		a.status = "no search pattern"
		return
	}
	pos, wrapped, found := search.Next(a.buf, a.matcher, a.cur, after)
	a.jumpTo(pos, wrapped, found)
}

func (a *App) findPrev() {
	if a.matcher.Empty() {
		a.status = "no search pattern"
		return
	}
	pos, wrapped, found := search.Prev(a.buf, a.matcher, a.cur)
	a.jumpTo(pos, wrapped, found)
}

func (a *App) jumpTo(pos buffer.Pos, wrapped, found bool) {
	if !found {
		a.status = "not found: " + string(a.query)
		return
	}
	a.cur = pos
	a.anchor = nil
	a.highlight = true
	if wrapped {
		a.status = "search wrapped"
	}
	a.scrollToCursor()
}

// snap clamps p to the buffer and moves it back to the start of its
// segment, so it can serve as a row start.
func (a *App) snap(p buffer.Pos) buffer.Pos {
	n := a.buf.Len()
	if n == 0 {
		return buffer.Pos{}
	}
	p.Line = max(0, min(p.Line, n-1))
	l := a.layout()
	text := a.line(p.Line)
	segs := l.Segments(text)
	p.Col = segs[l.SegmentAt(text, p.Col)].Start
	return p
}

// nextRow returns the row start after p, or p at the end.
func (a *App) nextRow(p buffer.Pos) buffer.Pos {
	l := a.layout()
	text := a.line(p.Line)
	segs := l.Segments(text)
	if i := l.SegmentAt(text, p.Col); i+1 < len(segs) {
		return buffer.Pos{Line: p.Line, Col: segs[i+1].Start}
	}
	if p.Line+1 < a.buf.Len() {
		return buffer.Pos{Line: p.Line + 1}
	}
	return p
}

// prevRow returns the row start before p, or p at the beginning.
func (a *App) prevRow(p buffer.Pos) buffer.Pos {
	l := a.layout()
	text := a.line(p.Line)
	segs := l.Segments(text)
	if i := l.SegmentAt(text, p.Col); i > 0 {
		return buffer.Pos{Line: p.Line, Col: segs[i-1].Start}
	}
	if p.Line > 0 {
		prev := l.Segments(a.line(p.Line - 1))
		return buffer.Pos{Line: p.Line - 1, Col: prev[len(prev)-1].Start}
	}
	return p
}

// scrollToCursor moves top and xoff the least that brings the cursor on
// screen.
func (a *App) scrollToCursor() {
	rows := a.textRows()
	if rows <= 0 {
		return
	}
	n := a.buf.Len()
	a.cur.Line = max(0, min(a.cur.Line, max(n-1, 0)))
	a.cur.Col = max(0, min(a.cur.Col, len(a.line(a.cur.Line))))
	a.top = a.snap(a.top)
	crow := a.snap(a.cur)
	if crow.Less(a.top) {
		a.top = crow
	} else {
		// rows-1 rows above the cursor, if below top, is the new top.
		p := crow
		for i := 0; i < rows-1 && a.top.Less(p); i++ {
			p = a.prevRow(p)
		}
		if a.top.Less(p) {
			a.top = p
		}
	}
	l := a.layout()
	if a.mode == layout.NoWrap && l.Width > 0 {
		_, x := l.Pos(a.line(a.cur.Line), a.cur.Col)
		if x < a.xoff {
			a.xoff = x
		} else if x >= a.xoff+l.Width {
			a.xoff = x - l.Width + 1
		}
	} else {
		a.xoff = 0
	}
}
