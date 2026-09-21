package app

import (
	"time"

	"github.com/gdamore/tcell/v2"
	"go.dlh.dev/vedi/internal/buffer"
)

// doubleClick is how soon a press on the same cell as the last one
// counts as the next click of a double or triple click.
const doubleClick = 400 * time.Millisecond

// wheelRows is how far one wheel tick scrolls the view.
const wheelRows = 3

// autoScrollTick is how often a drag held at an edge scrolls a row.
const autoScrollTick = 50 * time.Millisecond

// handleMouse: button 1 places the cursor and drags the selection, the
// wheel scrolls. A press dismisses help like any key; the wheel is
// ignored there. The mouse is ignored at the / and : prompts, and a
// press from before one is forgotten.
func (a *App) handleMouse(ev *tcell.EventMouse) {
	if a.searching || a.gotoing {
		a.held, a.dragging = false, false
		return
	}
	x, y := ev.Position()
	btn := ev.Buttons()
	switch {
	case btn&tcell.WheelUp != 0:
		if !a.helping {
			a.scrollView(-wheelRows)
		}
	case btn&tcell.WheelDown != 0:
		if !a.helping {
			a.scrollView(wheelRows)
		}
	case btn&tcell.Button1 == 0:
		a.held, a.dragging = false, false
	case a.held:
		// Motion with the button down looks like a press; only a press on
		// the text drags.
		if a.dragging {
			a.drag(x, y)
		}
	default:
		a.held = true
		a.press(x, y)
	}
}

// press returns from help, ignores the status line, and otherwise puts
// the cursor on the cell; a second press there within doubleClick
// selects the word, a third the line.
func (a *App) press(x, y int) {
	a.act()
	if a.helping {
		a.helping = false
		return
	}
	if y >= a.textRows() {
		return
	}
	now := a.now()
	n := 1
	if x == a.lastPress.x && y == a.lastPress.y && now.Sub(a.lastPress.at) <= doubleClick {
		n = a.lastPress.n + 1
	}
	a.dragging, a.ticking = true, false
	a.cur = a.cellPos(x, y)
	a.lastPress = click{now, x, y, a.cur, n}
	a.anchor = nil
	switch {
	case n == 2:
		a.selectWord()
	case n >= 3:
		// The cursor lands after the line's newline, maybe below the
		// screen; scrolling it in would move the line under the pointer.
		a.selectLine()
		return
	}
	a.scrollToCursor()
}

// drag selects from the pressed text to the cell under the mouse,
// clamped to the text rows. The anchor is the position pressed, not the
// cell: the press may have moved the rows. On the top row or the status
// row the view scrolls a row per tick while the button stays held.
func (a *App) drag(x, y int) {
	if a.anchor == nil {
		p := a.lastPress.pos
		a.anchor = &p
	}
	a.dragX, a.dragY = x, y
	a.cur = a.cellPos(x, min(y, a.textRows()-1))
	a.scrollToCursor()
	if a.edge() != 0 && !a.ticking {
		a.ticking = true
		t := &Tick{}
		t.SetEventTime(a.now())
		var post func()
		post = func() {
			if a.scr.PostEvent(t) != nil { // queue full: try next tick
				time.AfterFunc(autoScrollTick, post)
			}
		}
		time.AfterFunc(autoScrollTick, post)
	}
}

// edge is the direction the held drag is scrolling: -1 on the top row,
// 1 on the status row, 0 inside the text or not dragging.
func (a *App) edge() int {
	switch {
	case !a.held || !a.dragging:
		return 0
	case a.dragY <= 0:
		return -1
	case a.dragY >= a.textRows():
		return 1
	}
	return 0
}

// tick scrolls a row for a drag still held at an edge and extends the
// selection to the row under the pointer; drag arms the next tick. A
// tick armed before the last press belongs to an earlier drag.
func (a *App) tick(ev *Tick) {
	if ev.When().Before(a.lastPress.at) {
		return
	}
	a.ticking = false
	if d := a.edge(); d != 0 {
		a.scrollView(d)
		a.drag(a.dragX, a.dragY)
	}
}

// selectWord selects the word runes around the cursor, if it is on one.
func (a *App) selectWord() {
	text := a.line(a.cur.Line)
	if a.cur.Col >= len(text) || !isWord(text[a.cur.Col]) {
		return
	}
	i, j := a.cur.Col, a.cur.Col+1
	for i > 0 && isWord(text[i-1]) {
		i--
	}
	for j < len(text) && isWord(text[j]) {
		j++
	}
	a.anchor = &buffer.Pos{Line: a.cur.Line, Col: i}
	a.cur.Col = j
}

// selectLine selects the cursor's line with its newline, as Shift+Down
// from its start would; the last line to its end.
func (a *App) selectLine() {
	a.anchor = &buffer.Pos{Line: a.cur.Line}
	if a.cur.Line+1 < a.buf.Len() {
		a.cur = buffer.Pos{Line: a.cur.Line + 1}
	} else {
		a.cur = a.endPos()
	}
}

// cellPos maps a screen cell to the position drawn there: y rows down
// from top, stopping at the buffer's last row; the rune whose cells
// hold x, or the newline past the row's end.
func (a *App) cellPos(x, y int) buffer.Pos {
	if a.buf.Len() == 0 {
		return buffer.Pos{}
	}
	p := a.snap(a.top)
	for i := 0; i < y; i++ {
		p = a.nextRow(p)
	}
	ln := a.lineLayout(p.Line)
	row, _ := ln.Pos(p.Col)
	return buffer.Pos{Line: p.Line, Col: ln.Col(row, x+a.xoff)}
}

// scrollView moves the view n rows (negative is up), keeping the first
// line to the last row at the top. A cursor that would leave the screen
// is pulled to the edge row, keeping its column.
func (a *App) scrollView(n int) {
	a.act()
	rows := a.textRows()
	if rows <= 0 || a.buf.Len() == 0 {
		return
	}
	a.top = a.snap(a.top)
	for ; n > 0; n-- {
		a.top = a.nextRow(a.top)
	}
	for ; n < 0; n++ {
		a.top = a.prevRow(a.top)
	}
	bottom := a.top
	for i := 0; i < rows-1; i++ {
		bottom = a.nextRow(bottom)
	}
	crow := a.snap(a.cur)
	_, x := a.lineLayout(a.cur.Line).Pos(a.cur.Col)
	edge := crow
	if crow.Less(a.top) {
		edge = a.top
	} else if bottom.Less(crow) {
		edge = bottom
	}
	if edge != crow {
		ln := a.lineLayout(edge.Line)
		row, _ := ln.Pos(edge.Col)
		a.cur = buffer.Pos{Line: edge.Line, Col: ln.Col(row, x)}
	}
}

// Tick is the auto-scroll timer's event, timed when it was armed: a
// drag held at an edge posts one to the screen every autoScrollTick,
// retrying a tick later when the queue is full so none is lost.
type Tick struct{ tcell.EventTime }
