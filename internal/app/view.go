package app

import (
	"fmt"
	"strings"

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
// then. While help is up the bindings are the text, and no cursor is
// shown.
func (a *App) Draw() {
	a.scr.Clear()
	a.top = a.snap(a.top)
	curX, curY := -1, -1
	if a.screen == nil {
		curX, curY = a.drawText()
	}
	if a.helping {
		curX = -1
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

// helpCols is how wide the help is laid out, whatever the screen:
// less's help is likewise fixed. A narrower screen cuts it at the
// edge and scrolls sideways.
const helpCols = 80

// helpText lays out the help screen as text, after less's help: a
// centered title, a note on the modifier glyphs, the first section's
// rows with no heading, then each section's name centered in capitals
// between dashed rules. Rows are indented two, the keys two spaces
// apart in a column as wide as the widest list but at most half the
// room, the doc beside with a period; a list or a doc wider than its
// column continues on the next line, in its column, and a key wider
// than the column has a line to itself.
func helpText(secs []input.Section) string {
	glyphs := func(keys []string) []string {
		var out []string
		for _, k := range keys {
			out = append(out, helpGlyphs(k))
		}
		return out
	}
	keyw := 0
	for _, s := range secs {
		for _, r := range s.Rows {
			keyw = max(keyw, len([]rune(strings.Join(glyphs(r.Keys), "  "))))
		}
	}
	keyw = min(keyw, (helpCols-4)/2)
	docw := helpCols - 4 - keyw
	rule := " " + strings.Repeat("-", helpCols-2) + "\n"
	var sb strings.Builder
	sb.WriteString(helpCenter("SUMMARY OF VEDI COMMANDS") + "\n\n")
	sb.WriteString("      ⌃ ⌥ ⌘ are Ctrl, Alt and Cmd: ⌃F is Ctrl+f in the config file.\n\n")
	for _, s := range secs {
		if s.Name != "" {
			sb.WriteString("\n" + helpCenter(strings.ToUpper(s.Name)) + "\n\n")
		}
		for _, r := range s.Rows {
			kl := wrapWords(glyphs(r.Keys), "  ", keyw)
			dl := wrapWords(strings.Fields(r.Doc+"."), " ", docw)
			for j := 0; j < max(len(kl), len(dl)); j++ {
				k, d := "", ""
				if j < len(kl) {
					k = kl[j]
				}
				if j < len(dl) {
					d = dl[j]
				}
				sb.WriteString(strings.TrimRight(fmt.Sprintf("  %-*s  %s", keyw, k, d), " ") + "\n")
			}
		}
		sb.WriteString(rule)
	}
	return sb.String()
}

// helpCenter centers text in helpCols.
func helpCenter(text string) string {
	return strings.Repeat(" ", max(helpCols-len([]rune(text)), 0)/2) + text
}

// helpGlyphs is a key's help name: Ctrl+, Alt+ and Cmd+ as ⌃ ⌥ ⌘,
// and a letter under them a capital, as ⌃F.
func helpGlyphs(name string) string {
	glyphs := ""
	for again := true; again; {
		again = false
		for _, m := range []struct{ word, glyph string }{{"Ctrl+", "⌃"}, {"Alt+", "⌥"}, {"Cmd+", "⌘"}} {
			if strings.HasPrefix(name, m.word) {
				glyphs += m.glyph
				name = name[len(m.word):]
				again = true
			}
		}
	}
	if glyphs != "" && len([]rune(strings.TrimPrefix(name, "Shift+"))) == 1 {
		name = strings.ToUpper(name)
		name = strings.Replace(name, "SHIFT+", "Shift+", 1)
	}
	return glyphs + name
}

// wrapWords joins words with sep into lines of at most width runes;
// a word wider than that gets a line of its own.
func wrapWords(words []string, sep string, width int) []string {
	var lines []string
	line := ""
	for _, w := range words {
		switch {
		case line == "":
			line = w
		case len([]rune(line))+len([]rune(sep))+len([]rune(w)) <= width:
			line += sep + w
		default:
			lines = append(lines, line)
			line = w
		}
	}
	return append(lines, line)
}

// drawStatus draws statusText in reverse video on the bottom row, which
// needs two rows to exist, with "h help", naming whatever key shows
// help, at the right edge unless help is unbound or up, a prompt is
// open, or it would come within two spaces of the text.
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
	k, ok := a.keys.Find(input.Command{Action: input.Help})
	hint := []rune(k.String() + " help")
	if ok && !(a.helping || a.searching || a.gotoing || width+2+len(hint) > w) {
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
		return "help  motion scrolls, any other key returns"
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
