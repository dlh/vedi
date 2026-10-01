package app

import (
	"fmt"
	"strings"

	"github.com/gdamore/tcell/v3"
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

// edgeStyle draws the < and > that mark text off the side of the screen
// in nowrap mode: reverse, as control characters are, so they read as
// the pager's and not the text's.
var edgeStyle = tcell.StyleDefault.Reverse(true)

// Draw renders the text from top, the selection in selStyle, search
// matches in MatchStyle, and the status line. While a Screen waits
// for EOF only the status line is drawn: the view is not known until
// then. While help is up the bindings are the text, and no cursor is
// shown; nor is one scrolled off the side.
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
	if w, _ := a.scr.Size(); curX >= 0 && curX < w {
		a.scr.ShowCursor(curX, curY)
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
	curW := 1 // cells under the cursor
	for i := seg.Start; i < end; i++ {
		x := xs[i] - x0
		// The cursor or a match on any rune of the glyph shows on the
		// glyph, which has no smaller part to show it on.
		_, j := ln.Glyph(i)
		if a.cur.Line == p.Line && i <= a.cur.Col && a.cur.Col < j {
			curX, ok = x, true
			if i < len(line.Text) {
				curW = max(1, xs[i+1]-xs[i])
			}
		}
		pos := buffer.Pos{Line: p.Line, Col: i}
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
			st = selStyle
		case mi < len(matches) && matches[mi] < j:
			st = MatchStyle
		default:
			st = styleAt(runs, ri, i)
			if isControl(line.Text[i]) {
				st = st.Reverse(true)
			}
		}
		if ri < len(runs) && runs[ri].Start <= i && runs[ri].Url != "" {
			st = a.linked(st, runs[ri].Url, runs[ri].UrlId)
		}
		drawGlyph(a.scr, x, y, w, line.Text[i:j], xs[i+1]-xs[i], st)
		i = j - 1
	}
	// With edge markers on, text off either side of the screen is
	// marked at that edge. A wide rune half under the > is dropped:
	// the terminal would draw it over the marker. Motions keep the
	// cursor clear of the markers; the wheel can scroll one over it,
	// and then it is hidden as if off screen.
	if width := xs[len(line.Text)]; a.marks && a.mode == layout.NoWrap {
		if a.xoff > 0 && width > 0 {
			put(a.scr, 0, y, w, '<', edgeStyle)
			ok = ok && curX != 0
		}
		if width > a.xoff+w {
			if w >= 2 {
				if _, st, rw := a.scr.Get(w-2, y); rw == 2 {
					a.scr.SetContent(w-2, y, ' ', nil, st)
				}
			}
			put(a.scr, w-1, y, w, '>', edgeStyle)
			ok = ok && curX+curW <= w-1
		}
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
	var glyphs strings.Builder
	for again := true; again; {
		again = false
		for _, m := range []struct{ word, glyph string }{{"Ctrl+", "⌃"}, {"Alt+", "⌥"}, {"Cmd+", "⌘"}} {
			if strings.HasPrefix(name, m.word) {
				glyphs.WriteString(m.glyph)
				name = name[len(m.word):]
				again = true
			}
		}
	}
	if glyphs.Len() > 0 && len([]rune(strings.TrimPrefix(name, "Shift+"))) == 1 {
		name = strings.ToUpper(name)
		name = strings.Replace(name, "SHIFT+", "Shift+", 1)
	}
	return glyphs.String() + name
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
	for x := range w {
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
	xs := layout.Layout{}.Cells(text)
	for i := 0; i < len(text); i++ {
		j := i + 1
		for j < len(text) && xs[j+1] == xs[j] {
			j++
		}
		drawGlyph(scr, x+xs[i], y, w, text[i:j], xs[i+1]-xs[i], st)
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
	if a.next != nil {
		reading += "  reloading…"
	}
	return fmt.Sprintf("%s  line %d/%d  %s%s", a.name, a.cur.Line+1, a.buf.Len(), mode, reading)
}

// isControl reports whether r is drawn as ^X: a C0 control other than
// tab, or DEL.
func isControl(r rune) bool {
	return r < 0x20 && r != '\t' || r == 0x7f
}

// drawGlyph draws the cluster text at (x, y): a tab as width spaces, a
// control char as ^X (DEL as ^?), anything else as itself with its
// combining marks.
func drawGlyph(scr tcell.Screen, x, y, w int, text []rune, width int, st tcell.Style) {
	r := text[0]
	switch {
	case r == '\t':
		for k := range width {
			put(scr, x+k, y, w, ' ', st)
		}
	case r == 0x7f:
		put(scr, x, y, w, '^', st)
		put(scr, x+1, y, w, '?', st)
	case r < 0x20:
		put(scr, x, y, w, '^', st)
		put(scr, x+1, y, w, r+0x40, st)
	case x < 0 || x >= w:
	case len(text) > 1:
		// One string for tcell instead of a rune slice and a string.
		scr.Put(x, y, string(text), st)
	default:
		scr.SetContent(x, y, r, nil, st)
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

// linkKey is a style and the link on it.
type linkKey struct {
	st      tcell.Style
	url, id string
}

// maxLinks bounds the styles linked keeps.
const maxLinks = 4096

// linked is st with the link on it. tcell compares a style's link by
// pointer and sends a cell again when its style differs, so each
// style and link gets one Style, kept: a cell drawn again is then
// unchanged.
func (a *App) linked(st tcell.Style, url, id string) tcell.Style {
	k := linkKey{st, url, id}
	s, ok := a.links[k]
	if !ok {
		if len(a.links) >= maxLinks {
			clear(a.links)
		}
		s = st.Url(url)
		if id != "" {
			s = s.UrlId(id)
		}
		if a.links == nil {
			a.links = map[linkKey]tcell.Style{}
		}
		a.links[k] = s
	}
	return s
}
