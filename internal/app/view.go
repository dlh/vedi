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

// MatchStyle draws every cell of a search match: black on the
// terminal's bright yellow, which themes that mute the base palette
// tend to leave vivid.
var MatchStyle = tcell.StyleDefault.Foreground(tcell.PaletteColor(0)).Background(tcell.PaletteColor(11))

// Draw renders the text from top, the selection in selStyle, search
// matches in MatchStyle, and the status line. While a Screen waits
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
	var matches []int
	matchLine := -1
	for y := 0; y < rows && p.Line < a.buf.Len(); y++ {
		if a.highlight && p.Line != matchLine {
			matches, matchLine = a.matcher.All(a.line(p.Line)), p.Line
		}
		if x, ok := a.drawRow(y, p, matches); ok {
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

// drawRow draws the row starting at p on screen row y, with matches the
// line's search matches, and reports the cursor's column, if the cursor
// is on it.
func (a *App) drawRow(y int, p buffer.Pos, matches []int) (curX int, ok bool) {
	w, _ := a.scr.Size()
	line := a.buf.Line(p.Line)
	ln := a.lineLayout(p.Line)
	xs := ln.Cells()
	segi := ln.SegmentAt(p.Col)
	seg := ln.Segments()[segi]
	x0 := xs[seg.Start] + a.xoff
	selStart, selEnd, hasSel := a.selection()
	runs, ri := line.Runs, 0
	n, mi := a.matcher.Len(), 0

	// The newline is a virtual cell after the last rune, on the last
	// row: the cursor can rest on it, and it shows as one reverse cell
	// when selected.
	end := seg.End
	if segi == ln.Rows()-1 {
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
		// Runs and matches are sorted: skip those ending before i.
		for ri < len(runs) && runs[ri].End <= i {
			ri++
		}
		for mi < len(matches) && matches[mi]+n <= i {
			mi++
		}
		var st tcell.Style
		switch {
		case selected:
			st = linkAt(selStyle, runs, ri, i)
		case mi < len(matches) && matches[mi] <= i:
			st = linkAt(MatchStyle, runs, ri, i)
		default:
			st = styleAt(runs, ri, i)
			if isControl(line.Text[i]) {
				st = st.Reverse(true)
			}
		}
		j := i + 1
		for j < len(line.Text) && xs[j+1] == xs[j] {
			j++
		}
		drawGlyph(a.scr, x, y, w, line.Text[i], line.Text[i+1:j], xs[i+1]-xs[i], st)
		i = j - 1
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
// needs two rows to exist, with "h help" at the right edge unless help
// is up, a prompt is open, or it would come within two spaces of the
// text.
func (a *App) drawStatus() {
	w, h := a.scr.Size()
	if h < 2 {
		return
	}
	st := tcell.StyleDefault.Reverse(true)
	for x := 0; x < w; x++ {
		a.scr.SetContent(x, h-1, ' ', nil, st)
	}
	text := []rune(a.statusText())
	width := drawRunes(a.scr, 0, h-1, w, text, st)
	hint := []rune("h help")
	if !(a.helping || a.searching || a.gotoing || width+2+len(hint) > w) {
		drawRunes(a.scr, w-len(hint), h-1, w, hint, st)
	}
}

// drawRunes draws text from (x, y) glyph by glyph and returns its
// width in cells.
func drawRunes(scr tcell.Screen, x, y, w int, text []rune, st tcell.Style) int {
	xs := layout.Cells(text)
	for i := 0; i < len(text); i++ {
		j := i + 1
		for j < len(text) && xs[j+1] == xs[j] {
			j++
		}
		drawGlyph(scr, x+xs[i], y, w, text[i], text[i+1:j], xs[i+1]-xs[i], st)
		i = j - 1
	}
	return xs[len(text)]
}

func (a *App) statusText() string {
	switch {
	case a.helping:
		return "help  any key returns"
	case a.searching && a.promptBack:
		return "?" + string(a.query)
	case a.searching:
		return "/" + string(a.query)
	case a.gotoing:
		return ":" + string(a.lineNo)
	case a.status != "":
		return a.status
	}
	eof, err := a.buf.Finished()
	if err != nil {
		return "read error: " + err.Error()
	}
	mode := "wrap"
	if a.mode == layout.NoWrap {
		mode = "nowrap"
	}
	reading := ""
	if !eof {
		reading = "  reading…"
	}
	return fmt.Sprintf("%s  line %d/%d  %s%s", a.name, a.cur.Line+1, a.buf.Len(), mode, reading)
}

// isControl reports whether r is drawn as ^X: a C0 control other than
// tab, or DEL.
func isControl(r rune) bool {
	return r < 0x20 && r != '\t' || r == 0x7f
}

// drawGlyph draws r at (x, y): a tab as width spaces, a control char as
// ^X (DEL as ^?), anything else as itself with its combining marks.
func drawGlyph(scr tcell.Screen, x, y, w int, r rune, comb []rune, width int, st tcell.Style) {
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
	case x >= 0 && x < w:
		scr.SetContent(x, y, r, comb, st)
	}
}

// put draws r at (x, y) if x is on screen.
func put(scr tcell.Screen, x, y, w int, r rune, st tcell.Style) {
	if x >= 0 && x < w {
		scr.SetContent(x, y, r, nil, st)
	}
}

// styleAt is the style of rune i, where runs[ri] is the first run not
// ending before i.
func styleAt(runs []ansi.Run, ri, i int) tcell.Style {
	if ri < len(runs) && runs[ri].Start <= i {
		return runs[ri].Style
	}
	return tcell.StyleDefault
}

// linkAt is st with the link of rune i on it; see styleAt.
func linkAt(st tcell.Style, runs []ansi.Run, ri, i int) tcell.Style {
	if ri < len(runs) && runs[ri].Start <= i {
		st = st.Url(runs[ri].Url)
		if runs[ri].UrlId != "" {
			st = st.UrlId(runs[ri].UrlId)
		}
	}
	return st
}
