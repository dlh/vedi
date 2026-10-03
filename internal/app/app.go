// Package app holds the pager state and drives it from key and mouse
// events.
package app

import (
	"fmt"
	"strings"
	"sync"
	"sync/atomic"
	"time"
	"unicode"

	"github.com/gdamore/tcell/v3"
	"go.dlh.dev/vedi/internal/buffer"
	"go.dlh.dev/vedi/internal/clipboard"
	"go.dlh.dev/vedi/internal/config"
	"go.dlh.dev/vedi/internal/input"
	"go.dlh.dev/vedi/internal/layout"
	"go.dlh.dev/vedi/internal/search"
)

// Stdin is the Name of input read from stdin, which a window title in
// the input renames.
const Stdin = "<stdin>"

type Options struct {
	Names          []string // what the status line calls each input, by its part in the buffer
	Mode           layout.Mode
	WrapStyle      layout.WrapStyle // where wrap mode breaks rows
	StartLine      int              // 1-based line to put at the top; 0 for none
	Follow         bool             // keep the cursor on the last line until EOF (+G)
	Screen         *Screen
	Copier         clipboard.Copier
	Now            func() time.Time // the clock double-clicks are timed by; nil for time.Now
	MacOS          bool             // there is a ⌘ key
	Keys           input.Keymap     // nil for the defaults
	TabWidth       int              // cells per tab stop; 0 for 8
	EdgeMarkers    bool             // mark text off the sides with < and >, a wrapped row with \
	FileSeparators bool             // a row names each input, when there are several
	AutoReload     bool             // a Changed event reads the file again
	// Open reads the input again for a reload: it returns a buffer
	// being filled, whose reader calls notify as buffer.Fill does,
	// and a close for the files under it. Nil when the input cannot
	// be read again, as a pipe cannot.
	Open func(notify func()) (buf *buffer.Buffer, close func(), err error)
}

// Screen is the view the terminal was showing, so the pager can open on
// the same rows: the last screenful, scrolled ScrolledBy rows up, with
// the cursor at the 1-based (CursorRow, CursorCol) of that screenful.
// A zero CursorRow leaves the cursor at the top. The status line takes
// one row, so the terminal's top row is dropped.
type Screen struct {
	ScrolledBy int
	CursorRow  int
	CursorCol  int
}

type App struct {
	scr    tcell.Screen
	links  map[linkKey]tcell.Style // see linked
	drawn  []bool                  // the canvas's cells, kept between frames
	buf    *buffer.Buffer
	first  *buffer.Buffer // the startup buffer, whose reader Notify serves
	copier clipboard.Copier
	names  []string
	mode   layout.Mode
	style  layout.WrapStyle // where wrap mode breaks rows
	tab    int              // cells per tab stop; 0 for the default
	marks  bool             // mark text off the sides with < and >, a wrapped row with \
	seps   bool             // a row names each input, when there are several
	auto   bool             // a Changed event reads the file again

	cur     buffer.Pos
	anchor  *buffer.Pos // selection anchor; nil when there is no selection
	marking bool        // the mark is set: motions extend the selection
	top     buffer.Pos  // first visible row: a line and the start of one of its segments
	xoff    int         // horizontal scroll in NoWrap mode

	follow    bool
	startLine int     // 0-based +N target; -1 once applied
	screen    *Screen // applied at EOF, then nil; no text is drawn until then

	status string // one-shot message, cleared by the next key or click

	open      func(func()) (*buffer.Buffer, func(), error)
	closeBuf  func()         // closes buf's files; nil for the startup buffer
	next      *buffer.Buffer // a reload being read; nil for none
	closeNext func()
	dirty     bool // a change came during the reload: one more after it

	helping    bool      // the key bindings are shown instead of the text
	text       *textView // the text and its view, set aside while help is up
	searching  bool      // the / or ? prompt is open
	promptBack bool      // it is ?
	backward   bool      // the last search was ?, and so n goes up and N down
	query      prompt    // the / and ? prompts' line
	commanding bool      // the : prompt is open
	command    prompt    // its line
	matches    []string  // what the last Tab found, shown while the line is as it left it; nil for none
	matchPos   int       // the match filled in; -1 for what was typed
	stem       string    // the line before the first Tab
	filled     string    // the line as the last Tab left it
	matcher    search.Matcher
	highlight  bool

	now       func() time.Time
	macOS     bool
	keys      input.Keymap
	lastPress click // the last button-1 press, for multiple clicks and drags
	held      bool  // button 1 is down
	dragging  bool  // and went down on the text, so motion selects
	dragX     int   // the cell the drag last reached
	dragY     int
	ticking   bool // an auto-scroll Tick is on its way

	pending atomic.Bool               // the reader has data to take up
	owed    atomic.Uint64             // the held post's generation; 0 for none
	gen     atomic.Uint64             // the last generation
	posted  atomic.Pointer[time.Time] // when the last post was
	stopMu  sync.RWMutex              // guards stopped; held to post
	stopped bool                      // Stop was called: the queue may be closed

	laid    layout.Layout       // what laidOut was laid out for
	laidOut map[int]layout.Line // lines laid out, by index
}

// laidLines is how many lines are kept laid out before starting over.
const laidLines = 1024

type click struct {
	at   time.Time
	x, y int
	pos  buffer.Pos // the text under the cell, as laid out at the press
	n    int        // presses in a row on this cell: 2 is a double-click
}

func New(scr tcell.Screen, buf *buffer.Buffer, opts Options) *App {
	a := &App{
		scr:       scr,
		buf:       buf,
		first:     buf,
		copier:    opts.Copier,
		names:     opts.Names,
		mode:      opts.Mode,
		style:     opts.WrapStyle,
		tab:       opts.TabWidth,
		marks:     opts.EdgeMarkers,
		seps:      opts.FileSeparators,
		top:       buffer.Pos{Col: -1}, // the first row: line 0's separator, if it has one
		auto:      opts.AutoReload,
		follow:    opts.Follow,
		startLine: opts.StartLine - 1,
		screen:    opts.Screen,
		open:      opts.Open,
		now:       opts.Now,
		macOS:     opts.MacOS,
	}
	if a.now == nil {
		a.now = time.Now
	}
	a.keys = opts.Keys
	if a.keys == nil {
		a.keys = config.Default(opts.MacOS)
	}
	if opts.StartLine <= 0 {
		a.startLine = -1
	}
	return a
}

// redrawEvery bounds how often the reader's data is drawn.
const redrawEvery = 50 * time.Millisecond

// Notify asks for a redraw on the startup buffer's behalf. It is safe
// to call from the reader goroutine.
func (a *App) Notify() { a.notify(a.first) }

// notify is Notify for buf, the startup buffer or one a reload reads;
// nil counts as not finished. It reads buf, never a.buf, which the
// loop goroutine swaps. Repeated calls before the next draw are
// coalesced, and one within redrawEvery of the last is held until
// that has passed, unless buf has finished. When the queue is full
// the event is dropped but the work stays pending, and Handle does it
// on whatever event drains the queue.
func (a *App) notify(buf *buffer.Buffer) {
	first := a.pending.CompareAndSwap(false, true)
	eof := false
	if buf != nil {
		eof, _ = buf.Finished()
	}
	if eof {
		if first || a.owed.Swap(0) != 0 {
			a.post()
		}
		return
	}
	if !first {
		return
	}
	var wait time.Duration
	if t := a.posted.Load(); t != nil {
		wait = redrawEvery - time.Since(*t)
	}
	if wait <= 0 {
		a.post()
		return
	}
	// A held post's timer stays scheduled when a key takes the data up
	// first, so only the latest generation may post.
	g := a.gen.Add(1)
	a.owed.Store(g)
	time.AfterFunc(wait, func() {
		if a.owed.CompareAndSwap(g, 0) {
			a.post()
		}
	})
}

func (a *App) post() {
	t := time.Now()
	a.posted.Store(&t)
	a.Post(tcell.NewEventInterrupt(nil))
}

// Post queues ev for the loop without blocking and reports whether
// it was taken: false means the queue is full. After Stop it is
// dropped and reported taken, since there is no loop to miss it.
func (a *App) Post(ev tcell.Event) bool {
	a.stopMu.RLock()
	defer a.stopMu.RUnlock()
	if a.stopped {
		return true
	}
	select {
	case a.scr.EventQ() <- ev:
		return true
	default:
		return false
	}
}

// Stop ends posting. Call it before the screen's Fini, which closes
// the queue: a timer or a reader may post after that.
func (a *App) Stop() {
	a.stopMu.Lock()
	a.stopped = true
	a.stopMu.Unlock()
}

// Run draws and handles events until an action quits.
func (a *App) Run() {
	a.Draw()
	for {
		ev, ok := <-a.scr.EventQ()
		if !ok || a.Handle(ev) {
			return
		}
		a.Draw()
	}
}

// Handle processes one event and reports whether to quit. Data the
// reader notified of is taken up first, whatever the event.
func (a *App) Handle(ev tcell.Event) bool {
	_, interrupt := ev.(*tcell.EventInterrupt)
	if a.pending.Swap(false) || interrupt {
		a.swapIfDone()
		a.onData()
	}
	switch ev := ev.(type) {
	case *tcell.EventResize:
		a.scr.Sync()
		if a.helping {
			a.scrollHelp(0)
			a.scrollHelpSideways(0)
		} else {
			a.scrollToCursor()
		}
	case *tcell.EventKey:
		a.act()
		if a.searching {
			a.handleSearchKey(ev)
			return false
		}
		if a.helping {
			a.helpKey(a.keys.Lookup(ev))
			return false
		}
		if a.commanding {
			a.handleCommandKey(ev)
			return false
		}
		return a.handleKey(a.keys.Lookup(ev))
	case *tcell.EventMouse:
		a.handleMouse(ev)
	case *Tick:
		a.tick(ev)
	case *Changed:
		if a.auto {
			a.reload(false)
		}
	}
	return false
}

// act records that the user did something: startup positioning stops
// and the one-shot status clears.
func (a *App) act() {
	a.follow = false
	a.startLine = -1
	a.screen = nil
	a.status = ""
}

// onData runs after the reader appended lines: it applies a pending +N,
// follows for +G and applies a Screen at EOF. While the bindings are
// shown there is nothing to do: the h key ended startup positioning,
// and the text view is placed when it comes back. New text is not a
// motion: unless one of those moved the cursor, a view the wheel
// scrolled sideways off it stays put.
func (a *App) onData() {
	if a.helping {
		return
	}
	eof, _ := a.buf.Finished()
	n := a.buf.Len()
	cur, xoff := a.cur, a.xoff
	if a.startLine >= 0 && (n > a.startLine || eof) {
		a.cur = buffer.Pos{Line: min(a.startLine, max(n-1, 0))}
		a.top = a.withSep(a.cur)
		a.startLine = -1
	}
	if a.follow {
		a.cur = buffer.Pos{Line: max(n-1, 0)}
	}
	if a.screen != nil && eof {
		a.showScreen(*a.screen)
		a.screen = nil
	}
	a.scrollToCursor()
	if a.cur == cur && a.mode == layout.NoWrap {
		a.xoff = xoff
	}
}

// Verdict is -F's decision about the text read so far.
type Verdict int

const (
	Undecided Verdict = iota // the text fits so far but has not ended: it may grow
	Print                    // it ended fitting the screen: print it; the pager never opens
	Page                     // it outgrew the screen, or ended with a read error to show
)

// text is what OnePage measures: a buffer being filled, or a test's
// stand-in that grows at a chosen moment.
type text interface {
	Len() int
	Line(i int) buffer.Line
	Parts() []int
	StartsPart(i int) bool
	Finished() (bool, error)
}

// OnePage is -F's decision for the text buf holds so far, on a w×h
// screen with a status line: text that fits the rows above it at EOF
// is printed instead of paged. The printed text is the terminal's to
// lay out: it wraps whatever the mode and its tabs stop every 8 cells
// whatever tab_width says, so it is measured that way. seps counts a
// row for each input when there are several, as the pager draws one.
func OnePage(buf text, w, h int, seps bool) Verdict {
	// EOF is read before measuring: the reader may append and finish
	// at any moment, and an EOF seen afterwards would vouch for lines
	// the measuring missed.
	eof, err := buf.Finished()
	if err != nil {
		return Page
	}
	l := layout.Layout{Width: w, Mode: layout.Wrap}
	rows := 0
	seps = seps && len(buf.Parts()) > 1
	for i := 0; i < buf.Len(); i++ {
		if seps && buf.StartsPart(i) {
			rows++ // the row naming the input
		}
		if rows += l.Rows(buf.Line(i).Text); rows > textRows(h) {
			return Page
		}
	}
	// The error is read again after: a line whose bytes are gone from
	// a file that shrank is found by reading it, and is paged so it
	// is seen.
	if _, err := buf.Finished(); err != nil {
		return Page
	}
	if eof {
		return Print
	}
	return Undecided
}

// showScreen puts the view where the terminal had it. The buffer's last
// row was the terminal's bottom row, so the top is rows-1 plus the
// scroll above it; a trailing newline means the bottom row was blank
// and not in the buffer, one row fewer. The status line's row comes off
// the top of the screenful, and the cursor row moves up with it. When
// scrolled, the cursor was below the view and goes to the top.
func (a *App) showScreen(s Screen) {
	n := a.buf.Len()
	if n == 0 {
		return
	}
	_, h := a.scr.Size()
	rows := a.textRows()
	back := rows - 1 + s.ScrolledBy
	if a.buf.TrailingNewline() {
		back--
	}
	top := a.snap(a.endPos())
	for i := 0; i < back; i++ {
		top = a.prevRow(top)
	}
	a.top = top
	a.cur = top
	if s.ScrolledBy > 0 || s.CursorRow <= 0 {
		return
	}
	p := top
	for i := 0; i < min(s.CursorRow-(h-rows), rows)-1; i++ {
		p = a.nextRow(p)
	}
	ln := a.lineLayout(p.Line)
	row, _ := ln.Pos(p.Col)
	a.cur = buffer.Pos{Line: p.Line, Col: ln.Col(row, max(s.CursorCol-1, 0))}
}

func (a *App) handleKey(c input.Command) bool {
	if input.IsMovement(c.Action) {
		if c.Extend {
			a.extend()
		} else {
			a.drop()
		}
	}
	left := 0 // rows a row motion had nowhere to take
	switch c.Action {
	case input.Up:
		left = a.moveRows(-1)
	case input.Down:
		left = a.moveRows(1)
	case input.Left:
		a.moveCol(-1)
	case input.Right:
		a.moveCol(1)
	case input.Home:
		a.cur.Col = 0
	case input.End:
		a.cur.Col = len(a.line(a.cur.Line))
	case input.PageUp:
		left = a.moveRows(-a.pageRows())
	case input.PageDown:
		left = a.moveRows(a.pageRows())
	case input.HalfPageUp:
		left = a.moveRows(-a.halfPageRows())
	case input.HalfPageDown:
		left = a.moveRows(a.halfPageRows())
	case input.WordLeft:
		a.wordLeft()
	case input.WordRight:
		a.wordRight()
	case input.First:
		a.cur = buffer.Pos{}
	case input.Last:
		a.cur = buffer.Pos{Line: max(a.buf.Len()-1, 0)}
	case input.PrevFile:
		a.prevInput()
	case input.NextFile:
		a.nextInput()
	case input.SelectAll:
		a.anchor = &buffer.Pos{}
		a.cur = a.endPos()
	case input.ClearSelection:
		if a.anchor != nil {
			a.anchor, a.marking = nil, false
		} else {
			a.highlight = false
		}
	case input.SetMark:
		if a.marking {
			a.anchor, a.marking = nil, false
		} else {
			a.marking = true
			a.extend()
		}
	case input.Copy:
		a.copy()
	case input.CopyAndQuit:
		if _, _, ok := a.selection(); ok {
			return a.copy()
		}
		a.drop()
		a.moveRows(1)
	case input.Search, input.SearchBack:
		a.openSearch(c.Action == input.SearchBack)
	case input.SearchNext:
		a.find(a.backward, true)
	case input.SearchPrev:
		a.find(!a.backward, true)
	case input.CommandPrompt:
		a.commanding, a.dragging = true, false
		a.command.open()
	case input.Cycle:
		a.cycle(c.Arg)
	case input.Reload:
		a.reload(true)
	case input.Help:
		a.showHelp()
	case input.Quit:
		return true
	}
	// Extending past the last row, by Shift or the mark, takes the rest
	// of the last line; past the first, the start of the first.
	extending := c.Extend || a.marking
	if extending && left > 0 {
		a.cur = a.endPos()
	} else if extending && left < 0 {
		a.cur = buffer.Pos{}
	}
	a.scrollToCursor()
	return false
}

func (a *App) line(i int) []rune { return a.buf.Line(i).Text }

// setMode switches between wrap and nowrap; the horizontal scroll
// starts over.
func (a *App) setMode(m layout.Mode) {
	a.mode, a.xoff = m, 0
}

// wrapMarks reports whether wrap mode marks rows that continue, on a
// screen w wide: the last column is then the marker's, and text wraps
// a column early. Under three columns there is no room.
func (a *App) wrapMarks(w int) bool {
	return a.marks && a.mode == layout.Wrap && w > 2
}

func (a *App) layout() layout.Layout {
	w, _ := a.scr.Size()
	if a.wrapMarks(w) {
		w--
	}
	return layout.Layout{Width: w, Mode: a.mode, Tab: a.tab, WrapStyle: a.style}
}

// lineLayout is line i laid out for the current width and mode.
// Layouts are kept until the width or mode changes, or laidLines of
// them are held. A kept layout is used only while it fits the text:
// a line past the end may appear later, and a file may shrink. The
// cursor's line gets a row for its newline while the cursor is on it
// and the line's last row is full; that row is not kept. With wrap
// markers the newline has the marker's column, unless a tab wider
// than the screen fills that too.
func (a *App) lineLayout(i int) layout.Line {
	l := a.layout()
	if a.laidOut == nil || a.laid != l || len(a.laidOut) >= laidLines {
		a.laid, a.laidOut = l, map[int]layout.Line{}
	}
	text := a.line(i)
	ln, ok := a.laidOut[i]
	if !ok || len(ln.Cells()) != len(text)+1 {
		ln = l.Line(text)
		a.laidOut[i] = ln
	}
	if i == a.cur.Line && a.cur.Col >= len(text) {
		if w, _ := a.scr.Size(); a.wrapMarks(w) {
			l.Width = w // the newline may take the marker's column
		}
		ln = l.NewlineRow(ln)
	}
	return ln
}

// textRows is the rows left for text: all but the status line, which
// needs two rows to exist.
func (a *App) textRows() int {
	_, h := a.scr.Size()
	return textRows(h)
}

// textRows is the rows left for text on a screen h rows tall: all but
// the status line's, which a one-row screen does without.
func textRows(h int) int {
	if h >= 2 {
		return h - 1
	}
	return h
}

func (a *App) pageRows() int { return max(a.textRows(), 1) }

func (a *App) halfPageRows() int { return max(a.textRows()/2, 1) }

func (a *App) endPos() buffer.Pos {
	n := a.buf.Len()
	if n == 0 {
		return buffer.Pos{}
	}
	return buffer.Pos{Line: n - 1, Col: len(a.line(n - 1))}
}

// sepAt reports whether line i has a row above it naming its input:
// the first line of an input, with separators on and several inputs
// given. It is Pos{Line: i, Col: -1}, which orders before the line's
// own rows. An input with no lines starts where the next does and
// gets none.
func (a *App) sepAt(i int) bool {
	return a.seps && len(a.names) > 1 && a.buf.StartsPart(i)
}

// withSep is row p with its line's separator in its place when p is
// the line's first row and the line has one: the row that names an
// input comes into view with its first line.
func (a *App) withSep(p buffer.Pos) buffer.Pos {
	if p.Col == 0 && a.sepAt(p.Line) {
		p.Col = -1
	}
	return p
}

// moveRows moves the cursor n visual rows (negative is up), keeping its
// cell column where possible, and returns the rows left when the text
// ran out. Crossing a line resets the column first: lineLayout opens a
// newline row for the cursor's line, and the old column must not open
// one on the new line. A row naming an input counts as a row crossed,
// though the cursor cannot stop on it: one row's move steps over it.
func (a *App) moveRows(n int) int {
	ln := a.lineLayout(a.cur.Line)
	row, x := a.cursorPos(ln)
	for n > 0 {
		if row+1 < ln.Rows() {
			row++
		} else if a.cur.Line+1 < a.buf.Len() {
			if n > 1 && a.sepAt(a.cur.Line+1) {
				n--
			}
			a.cur = buffer.Pos{Line: a.cur.Line + 1}
			ln = a.lineLayout(a.cur.Line)
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
			if n < -1 && a.sepAt(a.cur.Line) {
				n++
			}
			a.cur = buffer.Pos{Line: a.cur.Line - 1}
			ln = a.lineLayout(a.cur.Line)
			row = ln.Rows() - 1
		} else {
			break
		}
		n++
	}
	a.cur.Col = ln.Col(row, x)
	return n
}

// cursorPos is the cursor's row and cell x on its line ln, at the
// glyph that shows it: a rune inside a cluster, where a search can
// leave the cursor, has no cell of its own.
func (a *App) cursorPos(ln layout.Line) (row, x int) {
	g, _ := ln.Glyph(a.cur.Col)
	return ln.Pos(g)
}

// moveCol moves one rune left or right, crossing lines at the ends, and
// on over the rest of a cluster (combining marks, an emoji sequence) so
// the cursor rests on a rune with a cell.
// A line that shrank leaves the cursor past its end; it comes back
// first.
func (a *App) moveCol(d int) {
	text := a.line(a.cur.Line)
	a.cur.Col = min(a.cur.Col, len(text))
	xs := a.lineLayout(a.cur.Line).Cells()
	switch {
	case d < 0 && a.cur.Col > 0:
		a.cur.Col--
		for a.cur.Col > 0 && xs[a.cur.Col+1] == xs[a.cur.Col] {
			a.cur.Col--
		}
	case d < 0 && a.cur.Line > 0:
		a.cur.Line--
		a.cur.Col = len(a.line(a.cur.Line))
	case d > 0 && a.cur.Col < len(text):
		a.cur.Col++
		for a.cur.Col < len(text) && xs[a.cur.Col+1] == xs[a.cur.Col] {
			a.cur.Col++
		}
	case d > 0 && a.cur.Line+1 < a.buf.Len():
		a.cur.Line++
		a.cur.Col = 0
	}
}

func isWord(r rune) bool {
	return unicode.IsLetter(r) || unicode.IsDigit(r) || unicode.IsMark(r) || r == '_'
}

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

// openSearch opens the / prompt, or the ? prompt if back.
func (a *App) openSearch(back bool) {
	a.searching, a.dragging = true, false
	a.promptBack = back
	a.query.open()
}

// handleSearchKey edits the / and ? prompts. Enter searches; with
// nothing typed it repeats the last pattern the prompt's way.
func (a *App) handleSearchKey(ev *tcell.EventKey) {
	switch {
	case cancels(ev):
		a.searching = false
	case ev.Key() == tcell.KeyEnter:
		a.searching = false
		a.backward = a.promptBack
		if len(a.query.text) == 0 {
			a.find(a.backward, true)
			return
		}
		a.query.remember()
		a.matcher = search.New(string(a.query.text))
		a.find(a.backward, false)
	default:
		a.query.edit(ev)
	}
}

// find goes to the next match before or after the cursor. A match at
// the cursor counts going forward unless skip; backward always skips.
func (a *App) find(backward, skip bool) {
	if a.matcher.Empty() {
		a.status = "no search pattern"
		return
	}
	if backward {
		a.jumpTo(search.Prev(a.buf, a.matcher, a.cur))
	} else {
		a.jumpTo(search.Next(a.buf, a.matcher, a.cur, skip))
	}
}

func (a *App) jumpTo(pos buffer.Pos, wrapped, found bool) {
	if !found {
		a.status = "not found: " + a.matcher.Pattern()
		return
	}
	a.cur = pos
	a.drop()
	a.highlight = true
	if wrapped {
		a.status = "search wrapped"
	}
	a.scrollMatchToTop()
	a.scrollToCursor()
}

// scrollMatchToTop makes the cursor's row the top one if it is off
// the screen, stopping at the last screenful.
func (a *App) scrollMatchToTop() {
	rows := a.textRows()
	if rows <= 0 || a.buf.Len() == 0 {
		return
	}
	crow := a.snap(a.cur)
	if top := a.snap(a.top); !crow.Less(top) {
		bottom := top
		for i := 0; i < rows-1; i++ {
			bottom = a.nextRow(bottom)
		}
		if !bottom.Less(crow) {
			return
		}
	}
	a.placeTop(crow)
}

// placeTop makes row p the top one, stopping at the last screenful:
// each row missing below p is one more above it.
func (a *App) placeTop(p buffer.Pos) {
	rows := a.textRows()
	below := 0
	for q := p; below < rows-1; below++ {
		next := a.nextRow(q)
		if next == q {
			break
		}
		q = next
	}
	for ; below < rows-1; below++ {
		p = a.prevRow(p)
	}
	a.top = p
}

// nextInput moves to the first line of the input after the cursor's;
// at the last, nowhere. An input with no lines is passed over.
func (a *App) nextInput() {
	for _, start := range a.buf.Parts() {
		if start > a.cur.Line && start < a.buf.Len() {
			a.jumpToInput(start)
			return
		}
	}
}

// prevInput moves to the first line of the cursor's input, or from
// there to the first line of the input before it; on the first line
// of the first, nowhere.
func (a *App) prevInput() {
	_, start, _ := a.input()
	if a.cur.Line > start {
		a.jumpToInput(start)
		return
	}
	target := -1
	for _, s := range a.buf.Parts() {
		if s < start {
			target = s
		}
	}
	if target >= 0 {
		a.jumpToInput(target)
	}
}

// jumpToInput puts the cursor on line, the first of an input, with
// the row naming the input, or the line itself, at the top.
func (a *App) jumpToInput(line int) {
	a.cur = buffer.Pos{Line: line}
	a.placeTop(a.withSep(a.cur))
}

// extend anchors the selection at the cursor if there is none, so a
// motion extends it; drop clears it, unless the mark is set.
func (a *App) extend() {
	if a.anchor == nil {
		p := a.cur
		a.anchor = &p
	}
}

func (a *App) drop() {
	if !a.marking {
		a.anchor = nil
	}
}

// snap clamps p to the buffer and moves it back to the start of its
// segment, so it can serve as a row start.
func (a *App) snap(p buffer.Pos) buffer.Pos {
	n := a.buf.Len()
	p.Line = max(0, min(p.Line, n-1))
	// A separator row stays one; the first row of an empty buffer
	// waits there for the lines, whose first may have a separator.
	if p.Col < 0 && (n == 0 || a.sepAt(p.Line)) {
		return buffer.Pos{Line: p.Line, Col: -1}
	}
	if n == 0 {
		return buffer.Pos{}
	}
	ln := a.lineLayout(p.Line)
	p.Col = ln.Segments()[ln.SegmentAt(p.Col)].Start
	return p
}

// nextRow returns the row start after p, or p at the end: the next
// line's separator, when it has one, comes before the line.
func (a *App) nextRow(p buffer.Pos) buffer.Pos {
	if p.Col < 0 {
		return buffer.Pos{Line: p.Line}
	}
	ln := a.lineLayout(p.Line)
	if i := ln.SegmentAt(p.Col); i+1 < ln.Rows() {
		return buffer.Pos{Line: p.Line, Col: ln.Segments()[i+1].Start}
	}
	if p.Line+1 < a.buf.Len() {
		if a.sepAt(p.Line + 1) {
			return buffer.Pos{Line: p.Line + 1, Col: -1}
		}
		return buffer.Pos{Line: p.Line + 1}
	}
	return p
}

// prevRow returns the row start before p, or p at the beginning: a
// line's first row steps back to its separator, when it has one.
func (a *App) prevRow(p buffer.Pos) buffer.Pos {
	if p.Col >= 0 {
		ln := a.lineLayout(p.Line)
		if i := ln.SegmentAt(p.Col); i > 0 {
			return buffer.Pos{Line: p.Line, Col: ln.Segments()[i-1].Start}
		}
		if a.sepAt(p.Line) {
			return buffer.Pos{Line: p.Line, Col: -1}
		}
	}
	if p.Line > 0 {
		prev := a.lineLayout(p.Line - 1).Segments()
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
	if n == 0 {
		return
	}
	crow := a.snap(a.cur)
	if crow.Less(a.top) {
		a.top = crow
		if rows > 1 { // a row to spare for the separator
			a.top = a.withSep(crow)
		}
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
		// The cursor's whole glyph must fit, not just its first cell,
		// and clear of the < and > that mark text off either side.
		ln := a.lineLayout(a.cur.Line)
		xs := ln.Cells()
		g, _ := ln.Glyph(a.cur.Col)
		_, x := ln.Pos(g)
		w := 1
		if g < len(xs)-1 {
			w = max(1, xs[g+1]-xs[g])
		}
		width := xs[len(xs)-1]
		mark := 0 // the column a marker takes at an edge
		if a.marks {
			mark = 1
		}
		right := func(xoff int) int {
			if width > xoff+l.Width {
				return mark
			}
			return 0
		}
		if x < a.xoff+mark {
			a.xoff = max(0, x-mark)
		} else if x+w > a.xoff+l.Width-right(a.xoff) {
			xoff := x + w - l.Width
			xoff += right(xoff)
			a.xoff = min(max(0, x-mark), xoff)
		}
	} else {
		a.xoff = 0
	}
}

// textView is what help sets aside: the text, how it was shown, and
// its search, which help's does not disturb.
type textView struct {
	buf       *buffer.Buffer
	cur, top  buffer.Pos
	anchor    *buffer.Pos
	xoff      int
	mode      layout.Mode
	matcher   search.Matcher
	backward  bool
	highlight bool
}

// showHelp puts the bindings in the text's place, laid out as text
// in NoWrap mode, so drawing and scrolling are the text's. The
// search starts over: help's is its own.
func (a *App) showHelp() {
	a.helping, a.dragging = true, false
	a.text = &textView{a.buf, a.cur, a.top, a.anchor, a.xoff, a.mode, a.matcher, a.backward, a.highlight}
	a.buf, a.cur, a.top, a.anchor, a.xoff, a.mode = a.helpBuffer(), buffer.Pos{}, buffer.Pos{}, nil, 0, layout.NoWrap
	a.matcher, a.backward, a.highlight = search.Matcher{}, false, false
	a.laidOut = nil
}

// helpStep is how far Left and Right scroll the bindings sideways.
const helpStep = 8

// helpBuffer is the bindings laid out as text.
func (a *App) helpBuffer() *buffer.Buffer {
	b := buffer.New()
	b.Write([]byte(helpText(a.keys.Help())))
	b.Finish(nil, true)
	return b
}

// hideHelp puts the text back as it was, scrolled to the cursor: a
// reload meanwhile may have moved it.
func (a *App) hideHelp() {
	t := a.text
	a.buf, a.cur, a.top, a.anchor, a.xoff, a.mode = t.buf, t.cur, t.top, t.anchor, t.xoff, t.mode
	a.matcher, a.backward, a.highlight = t.matcher, t.backward, t.highlight
	a.helping, a.text, a.laidOut = false, nil, nil
	a.scrollToCursor()
}

// helpKey scrolls the bindings by a motion and searches them by the
// search keys; any other key returns. The search is the text's: its
// pattern, direction and highlight carry over, as less's do.
func (a *App) helpKey(c input.Command) {
	n := a.buf.Len()
	switch c.Action {
	case input.Search, input.SearchBack:
		a.openSearch(c.Action == input.SearchBack)
		return
	case input.SearchNext:
		a.find(a.backward, true)
		return
	case input.SearchPrev:
		a.find(!a.backward, true)
		return
	}
	if c.Extend || !input.IsMovement(c.Action) {
		a.hideHelp()
		return
	}
	switch c.Action {
	case input.Up:
		a.scrollHelp(-1)
	case input.Down:
		a.scrollHelp(1)
	case input.PageUp:
		a.scrollHelp(-a.pageRows())
	case input.PageDown:
		a.scrollHelp(a.pageRows())
	case input.HalfPageUp:
		a.scrollHelp(-a.halfPageRows())
	case input.HalfPageDown:
		a.scrollHelp(a.halfPageRows())
	case input.First:
		a.scrollHelp(-n)
	case input.Last:
		a.scrollHelp(n)
	case input.Left, input.WordLeft:
		a.scrollHelpSideways(-helpStep)
	case input.Right, input.WordRight:
		a.scrollHelpSideways(helpStep)
	case input.Home:
		a.scrollHelpSideways(-a.helpWidth())
	case input.End:
		a.scrollHelpSideways(a.helpWidth())
	}
}

// scrollHelp moves the bindings by n rows, no further than the last
// page; each line is a row, the mode being NoWrap. The cursor, hidden
// but where a search starts, is kept on screen as scrollView keeps
// the text's: scrolled off, it moves to the edge row.
func (a *App) scrollHelp(n int) {
	rows := a.textRows()
	a.top = buffer.Pos{Line: max(0, min(a.top.Line+n, a.buf.Len()-rows))}
	if a.cur.Line < a.top.Line {
		a.cur = a.top
	} else if a.cur.Line >= a.top.Line+rows {
		a.cur = buffer.Pos{Line: a.top.Line + rows - 1}
	}
}

// scrollHelpSideways moves the bindings by n columns, no further than
// brings the widest line's end to the right edge.
func (a *App) scrollHelpSideways(n int) {
	w, _ := a.scr.Size()
	a.xoff = max(0, min(a.xoff+n, a.helpWidth()-w))
}

// helpWidth is the widest line of the bindings, in cells.
func (a *App) helpWidth() int {
	width := 0
	for i := 0; i < a.buf.Len(); i++ {
		xs := a.lineLayout(i).Cells()
		width = max(width, xs[len(xs)-1])
	}
	return width
}
