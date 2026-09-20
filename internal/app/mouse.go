package app

import (
	"time"

	"github.com/gdamore/tcell/v2"
	"go.dlh.dev/vedi/internal/buffer"
)

// doubleClick is how soon a second press on the same cell selects the
// word there.
const doubleClick = 400 * time.Millisecond

// wheelRows is how far one wheel tick scrolls the view.
const wheelRows = 3

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
// selects the word.
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
	double := x == a.lastPress.x && y == a.lastPress.y && now.Sub(a.lastPress.at) <= doubleClick
	a.dragging = true
	a.cur = a.cellPos(x, y)
	a.lastPress = click{now, x, y, a.cur}
	a.anchor = nil
	if double {
		a.selectWord()
	}
	a.scrollToCursor()
}

// drag selects from the pressed text to the cell under the mouse,
// clamped to the text rows. The anchor is the position pressed, not the
// cell: the press may have moved the rows.
func (a *App) drag(x, y int) {
	if a.anchor == nil {
		p := a.lastPress.pos
		a.anchor = &p
	}
	a.cur = a.cellPos(x, min(y, a.textRows()-1))
	a.scrollToCursor()
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
