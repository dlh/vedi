package app

import (
	"time"

	"github.com/gdamore/tcell/v3"
	"go.dlh.dev/vedi/internal/buffer"
	"go.dlh.dev/vedi/internal/layout"
)

// doubleClick is how soon a press on the same cell as the last one
// counts as the next click of a double or triple click.
const doubleClick = 400 * time.Millisecond

// wheelRows and wheelCols are how far one wheel tick scrolls the view.
const (
	wheelRows = 3
	wheelCols = 4
)

// autoScrollTick is how often a drag held at an edge scrolls a row.
const autoScrollTick = 50 * time.Millisecond

// handleMouse: button 1 places the cursor and drags the selection,
// with Shift extends it, the wheel scrolls, sideways too, the bindings
// as well. A press returns
// from help like any key. The mouse is ignored at the / and : prompts,
// and a press from before one is forgotten.
func (a *App) handleMouse(ev *tcell.EventMouse) {
	if a.searching || a.commanding {
		a.held, a.dragging = false, false
		return
	}
	x, y := ev.Position()
	btn := ev.Buttons()
	rows, cols := wheel(ev)
	switch {
	case rows != 0 && a.helping:
		a.scrollHelp(rows)
	case cols != 0 && a.helping:
		a.scrollHelpSideways(cols)
	case rows != 0:
		a.scrollView(rows)
	case cols != 0:
		a.scrollViewSideways(cols)
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
		a.press(x, y, ev.Modifiers()&tcell.ModShift != 0)
	}
}

// wheel is the rows and columns a wheel event scrolls, zero for any
// other event. Shift turns the wheel sideways: up is left, down right.
func wheel(ev *tcell.EventMouse) (rows, cols int) {
	btn := ev.Buttons()
	switch {
	case btn&tcell.WheelUp != 0:
		rows = -wheelRows
	case btn&tcell.WheelDown != 0:
		rows = wheelRows
	case btn&tcell.WheelLeft != 0:
		cols = -wheelCols
	case btn&tcell.WheelRight != 0:
		cols = wheelCols
	}
	if rows != 0 && ev.Modifiers()&tcell.ModShift != 0 {
		rows, cols = 0, rows/wheelRows*wheelCols
	}
	return rows, cols
}

// press returns from help, ignores the status line, and otherwise puts
// the cursor on the cell; a second press there within doubleClick
// selects the word, a third the line. With shift the selection extends
// to the cell instead, from the cursor when there is none, and the
// press counts toward no double click.
func (a *App) press(x, y int, shift bool) {
	a.act()
	if a.helping {
		a.hideHelp()
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
	if shift {
		if a.anchor == nil {
			p := a.cur
			a.anchor = &p
		}
		a.cur = a.cellPos(x, y)
		a.lastPress = click{now, x, y, a.cur, 0}
		a.scrollToCursor()
		return
	}
	a.cur = a.cellPos(x, y)
	a.lastPress = click{now, x, y, a.cur, n}
	a.anchor, a.marking = nil, false
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
			if !a.Post(t) { // queue full: try next tick
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
	for range y {
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
	_, x := a.cursorPos(a.lineLayout(a.cur.Line))
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

// scrollViewSideways moves the view n columns (negative is left), no
// further right than brings the widest row on screen and its newline
// to the right edge; already past that, after scrolling down to
// narrower rows, a right tick stays put rather than turn back.
// Wrapped, nothing is off screen to the side. The cursor keeps its
// place, hidden while off screen: the next motion brings the view
// back to it.
func (a *App) scrollViewSideways(n int) {
	a.act()
	l := a.layout()
	rows := a.textRows()
	if a.mode != layout.NoWrap || l.Width <= 0 || rows <= 0 || a.buf.Len() == 0 {
		return
	}
	a.top = a.snap(a.top)
	widest := 0
	for i := a.top.Line; i < min(a.top.Line+rows, a.buf.Len()); i++ { // a row a line
		xs := a.lineLayout(i).Cells()
		widest = max(widest, xs[len(xs)-1]+1)
	}
	a.xoff = max(0, min(a.xoff+n, max(a.xoff, widest-l.Width)))
}

// Tick is the auto-scroll timer's event, timed when it was armed: a
// drag held at an edge posts one to the screen every autoScrollTick,
// retrying a tick later when the queue is full so none is lost.
type Tick struct{ tcell.EventTime }
