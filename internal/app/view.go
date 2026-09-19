package app

import (
	"fmt"

	"github.com/gdamore/tcell/v2"
	"go.dlh.dev/vedi/internal/ansi"
	"go.dlh.dev/vedi/internal/buffer"
	"go.dlh.dev/vedi/internal/input"
	"go.dlh.dev/vedi/internal/layout"
)

// selStyle draws every selected cell, whatever the text's own color:
// the terminal's default colors, swapped.
var selStyle = tcell.StyleDefault.Reverse(true)

// Draw renders the text from top, the selection in selStyle, search
// matches in reverse video, and the status line. While a Screen waits
// for EOF only the status line is drawn: the view is not known until
// then. While help is up the bindings take the text's place.
func (a *App) Draw() {
	a.scr.Clear()
	a.top = a.snap(a.top)
	curX, curY := -1, -1
	switch {
	case a.helping:
		a.drawHelp()
	case a.screen == nil:
		curX, curY = a.drawText()
	}
	a.drawStatus()
	if curX >= 0 {
		w, _ := a.scr.Size()
		a.scr.ShowCursor(max(0, min(curX, w-1)), curY)
	} else {
		a.scr.HideCursor()
	}
	a.scr.Show()
}

// drawText draws the visible rows and returns the cursor's screen
// position, or (-1, -1) off screen.
func (a *App) drawText() (curX, curY int) {
	curX, curY = -1, -1
	rows := a.textRows()
	p := a.top
	for y := 0; y < rows && p.Line < a.buf.Len(); y++ {
		if x, ok := a.drawRow(y, p); ok {
			curX, curY = x, y
		}
		next := a.nextRow(p)
		if next == p {
			break
		}
		p = next
	}
	return curX, curY
}

// drawRow draws the row starting at p on screen row y and reports the
// cursor's column, if the cursor is on it.
func (a *App) drawRow(y int, p buffer.Pos) (curX int, ok bool) {
	w, _ := a.scr.Size()
	line := a.buf.Line(p.Line)
	xs := layout.Cells(line.Text)
	l := a.layout()
	seg := l.Segments(line.Text)[l.SegmentAt(line.Text, p.Col)]
	x0 := xs[seg.Start] + a.xoff
	var matches []int
	if a.highlight {
		matches = a.matcher.All(line.Text)
	}
	selStart, selEnd, hasSel := a.selection()

	// The newline is a virtual cell after the last rune: the cursor can
	// rest on it, and it shows as one reverse cell when selected.
	end := seg.End
	if end == len(line.Text) {
		end++
	}
	for i := seg.Start; i < end; i++ {
		pos := buffer.Pos{Line: p.Line, Col: i}
		x := xs[i] - x0
		if pos == a.cur {
			curX, ok = x, true
		}
		selected := hasSel && !pos.Less(selStart) && pos.Less(selEnd)
		if i == len(line.Text) {
			if selected {
				put(a.scr, x, y, w, ' ', selStyle)
			}
			continue
		}
		st := runeStyle(line, i, matches, a.matcher.Len(), selected)
		drawGlyph(a.scr, x, y, w, line.Text[i], xs[i+1]-xs[i], st)
	}
	return curX, ok
}

// drawHelp draws input.Bindings, keys in a column as wide as the
// widest, as many as fit above the status line.
func (a *App) drawHelp() {
	w, _ := a.scr.Size()
	keyw := 0
	for _, b := range input.Bindings {
		keyw = max(keyw, len([]rune(b.Keys)))
	}
	for y, b := range input.Bindings {
		if y >= a.textRows() {
			break
		}
		row := []rune(fmt.Sprintf("%-*s  %s", keyw, b.Keys, b.Doc))
		for x, r := range row {
			put(a.scr, x, y, w, r, tcell.StyleDefault)
		}
	}
}

// drawStatus draws statusText in reverse video on the bottom row, which
// needs two rows to exist, with "? help" at the right edge unless help
// is up, a prompt is open, or it would come within two spaces of the
// text.
func (a *App) drawStatus() {
	w, h := a.scr.Size()
	if h < 2 {
		return
	}
	st := tcell.StyleDefault.Reverse(true)
	text := []rune(a.statusText())
	hint := []rune("? help")
	if a.helping || a.searching || a.gotoing || len(text)+2+len(hint) > w {
		hint = nil
	}
	for x := 0; x < w; x++ {
		r := ' '
		if x < len(text) {
			r = text[x]
		} else if x >= w-len(hint) {
			r = hint[x-(w-len(hint))]
		}
		a.scr.SetContent(x, h-1, r, nil, st)
	}
}

func (a *App) statusText() string {
	switch {
	case a.helping:
		return "help  any key returns"
	case a.searching:
		return "/" + string(a.query)
	case a.gotoing:
		return ":" + string(a.lineNo)
	case a.status != "":
		return a.status
	case a.readErr != "":
		return a.readErr
	}
	mode := "wrap"
	if a.mode == layout.NoWrap {
		mode = "nowrap"
	}
	reading := ""
	if eof, _ := a.buf.Finished(); !eof {
		reading = "  reading…"
	}
	return fmt.Sprintf("%s  line %d/%d  %s%s", a.name, a.cur.Line+1, a.buf.Len(), mode, reading)
}

// runeStyle is the style of rune i: its own, reversed for a control
// char or a search match of length n; selStyle when selected.
func runeStyle(line buffer.Line, i int, matches []int, n int, selected bool) tcell.Style {
	if selected {
		return selStyle
	}
	st := styleAt(line.Runs, i)
	if isControl(line.Text[i]) || matched(matches, n, i) {
		st = st.Reverse(true)
	}
	return st
}

// isControl reports whether r is drawn as ^X: a C0 control other than
// tab, or DEL.
func isControl(r rune) bool {
	return r < 0x20 && r != '\t' || r == 0x7f
}

// drawGlyph draws r at (x, y): a tab as width spaces, a control char as
// ^X (DEL as ^?), anything else as itself.
func drawGlyph(scr tcell.Screen, x, y, w int, r rune, width int, st tcell.Style) {
	switch {
	case r == '\t':
		for k := 0; k < width; k++ {
			put(scr, x+k, y, w, ' ', st)
		}
	case r == 0x7f:
		put(scr, x, y, w, '^', st)
		put(scr, x+1, y, w, '?', st)
	case r < 0x20:
		put(scr, x, y, w, '^', st)
		put(scr, x+1, y, w, r+0x40, st)
	default:
		put(scr, x, y, w, r, st)
	}
}

// put draws r at (x, y) if x is on screen.
func put(scr tcell.Screen, x, y, w int, r rune, st tcell.Style) {
	if x >= 0 && x < w {
		scr.SetContent(x, y, r, nil, st)
	}
}

func styleAt(runs []ansi.Run, i int) tcell.Style {
	for _, r := range runs {
		if i >= r.Start && i < r.End {
			return r.Style
		}
	}
	return tcell.StyleDefault
}

// matched reports whether rune i is inside a match of length n.
func matched(matches []int, n, i int) bool {
	for _, m := range matches {
		if i >= m && i < m+n {
			return true
		}
	}
	return false
}
