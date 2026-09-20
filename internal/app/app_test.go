package app

import (
	"fmt"
	"strings"
	"testing"
	"time"

	"github.com/gdamore/tcell/v2"
	"go.dlh.dev/vedi/internal/ansi"
	"go.dlh.dev/vedi/internal/buffer"
	"go.dlh.dev/vedi/internal/clipboard"
	"go.dlh.dev/vedi/internal/layout"
	"go.dlh.dev/vedi/internal/search"
)

// newTestApp builds an app on a w×h simulation screen with input fully
// read, +N/+G applied, and the first frame drawn.
func newTestApp(t *testing.T, w, h int, input string, opts Options) (*App, tcell.SimulationScreen) {
	t.Helper()
	scr := tcell.NewSimulationScreen("UTF-8")
	if err := scr.Init(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(scr.Fini)
	scr.SetSize(w, h)
	buf := buffer.New()
	buffer.Fill(strings.NewReader(input), buf, func() {})
	if opts.Copier == nil {
		opts.Copier = clipboard.OSC52{Screen: scr}
	}
	a := New(scr, buf, opts)
	a.Handle(tcell.NewEventInterrupt(nil))
	a.Draw()
	return a, scr
}

func key(k tcell.Key, r rune, mod tcell.ModMask) *tcell.EventKey {
	return tcell.NewEventKey(k, r, mod)
}

// press feeds keys, redrawing after each, and reports whether the last
// quit.
func press(a *App, keys ...*tcell.EventKey) bool {
	quit := false
	for _, k := range keys {
		quit = a.Handle(k)
		a.Draw()
	}
	return quit
}

// row returns the text of screen row y with trailing spaces trimmed.
func row(scr tcell.SimulationScreen, y int) string {
	cells, w, _ := scr.GetContents()
	var sb strings.Builder
	for x := 0; x < w; x++ {
		c := cells[y*w+x]
		if len(c.Runes) == 0 {
			continue
		}
		sb.WriteString(string(c.Runes))
	}
	return strings.TrimRight(sb.String(), " ")
}

func cellStyle(scr tcell.SimulationScreen, x, y int) tcell.Style {
	cells, w, _ := scr.GetContents()
	return cells[y*w+x].Style
}

func TestDrawStyles(t *testing.T) {
	_, scr := newTestApp(t, 20, 5, "\x1b[31mred", Options{})
	fg, _, _ := cellStyle(scr, 0, 0).Decompose()
	if fg != tcell.PaletteColor(1) {
		t.Errorf("fg = %v, want red", fg)
	}
}

func clip(scr tcell.SimulationScreen) string { return string(scr.GetClipboardData()) }

func TestReadErrorInStatus(t *testing.T) {
	a, scr := newTestApp(t, 30, 4, "", Options{})
	a.buf.Append(buffer.Line{Text: []rune("partial")})
	a.buf.Finish(fmt.Errorf("disk on fire"), false)
	a.Handle(tcell.NewEventInterrupt(nil))
	a.Draw()
	if got := row(scr, 3); got != "read error: disk on fire" {
		t.Errorf("status = %q", got)
	}
	if got := row(scr, 0); got != "partial" {
		t.Errorf("text kept after error: %q", got)
	}
}

func TestNotifyCoalesces(t *testing.T) {
	a, scr := newTestApp(t, 10, 4, "x", Options{})
	a.Notify()
	a.Notify()
	if !scr.HasPendingEvent() {
		t.Fatal("Notify should post an event")
	}
	a.Handle(scr.PollEvent())
	if scr.HasPendingEvent() {
		t.Error("second Notify before a draw should be coalesced")
	}
	a.Notify()
	if !scr.HasPendingEvent() {
		t.Error("Notify after handling should post again")
	}
}

// TestEndToEnd pipes a colored screen dump (each line reopened with
// \x1b[m, as kitty writes it) through the whole stack and checks what
// lands on the clipboard.
func TestEndToEnd(t *testing.T) {
	fixture := "\x1b[m$ \x1b[1mmake\x1b[m test\n" +
		"\x1b[m\x1b[32mPASS\x1b[m  internal/ansi   \t0.01s\n" +
		"\x1b[m\x1b[31mFAIL\x1b[m  internal/app    \t0.20s\n" +
		"\x1b[m$ \n"
	a, scr := newTestApp(t, 40, 6, fixture, Options{Mode: layout.NoWrap, StartLine: 2})
	fg, _, _ := cellStyle(scr, 0, 0).Decompose()
	if fg != tcell.PaletteColor(2) {
		t.Errorf("PASS should be green, got %v", fg)
	}
	press(a, key(tcell.KeyDown, 0, tcell.ModShift), key(tcell.KeyDown, 0, tcell.ModShift))
	if !press(a, key(tcell.KeyEnter, 0, 0)) {
		t.Fatal("Enter with a selection should quit")
	}
	want := "PASS  internal/ansi   \t0.01s\nFAIL  internal/app    \t0.20s\n"
	if got := clip(scr); got != want {
		t.Errorf("clipboard = %q\nwant        %q", got, want)
	}
}

func TestSelectionStyleIsConstant(t *testing.T) {
	// red, default, reverse-video and a control char, all selected.
	a, scr := newTestApp(t, 20, 5, "\x1b[31mr\x1b[md\x1b[7mv\x1b[m\r", Options{})
	before := cellStyle(scr, 0, 0)
	if fg, _, _ := before.Decompose(); fg != tcell.PaletteColor(1) {
		t.Fatalf("unselected fg = %v, want red", fg)
	}
	press(a, key(tcell.KeyCtrlA, 0, tcell.ModCtrl))
	want := tcell.StyleDefault.Reverse(true)
	for x, name := range []string{"red", "default", "reverse", "^M"} {
		if got := cellStyle(scr, x, 0); got != want {
			t.Errorf("selected %s cell style = %v, want %v", name, got, want)
		}
	}
}

func TestMatchStyleIsConstant(t *testing.T) {
	// red, default and reverse-video text, all matched.
	a, scr := newTestApp(t, 20, 5, "\x1b[31mo\x1b[mo\x1b[7mo\x1b[m", Options{})
	press(a, key(tcell.KeyRune, '/', 0), key(tcell.KeyRune, 'o', 0), key(tcell.KeyEnter, 0, 0))
	want := tcell.StyleDefault.Foreground(tcell.PaletteColor(0)).Background(tcell.PaletteColor(11))
	for x, name := range []string{"red", "default", "reverse"} {
		if got := cellStyle(scr, x, 0); got != want {
			t.Errorf("matched %s cell style = %v, want %v", name, got, want)
		}
	}
	// A selected match is drawn as selected.
	press(a, key(tcell.KeyRight, 0, tcell.ModShift))
	if got := cellStyle(scr, 0, 0); got != selStyle {
		t.Errorf("selected match cell style = %v, want %v", got, selStyle)
	}
}

// BenchmarkLongLine moves right and redraws on a 1 MB line, which must
// not lay the whole line out again on every key.
func BenchmarkLongLine(b *testing.B) {
	scr := tcell.NewSimulationScreen("UTF-8")
	if err := scr.Init(); err != nil {
		b.Fatal(err)
	}
	defer scr.Fini()
	scr.SetSize(80, 24)
	buf := buffer.New()
	buf.Append(buffer.Line{Text: []rune(strings.Repeat("a", 1<<20))})
	buf.Finish(nil, true)
	a := New(scr, buf, Options{})
	a.Draw()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		press(a, key(tcell.KeyRight, 0, 0))
	}
}

// TestNotifyFullQueue: a Notify dropped by a full event queue is still
// acted on once the queue drains, even with no later Notify.
func TestNotifyFullQueue(t *testing.T) {
	a, scr := newTestApp(t, 30, 4, "", Options{})
	for scr.PostEvent(key(tcell.KeyRune, 'j', 0)) == nil {
	}
	a.buf.Append(buffer.Line{Text: []rune("late")})
	a.buf.Finish(fmt.Errorf("disk on fire"), false)
	a.Notify()
	for scr.HasPendingEvent() {
		a.Handle(scr.PollEvent())
		a.Draw()
	}
	if got := row(scr, 3); got != "read error: disk on fire" {
		t.Errorf("status = %q", got)
	}
}

// TestTickFullQueue: a tick that finds the event queue full still
// arrives once the queue drains, so an edge drag keeps scrolling.
func TestTickFullQueue(t *testing.T) {
	a, scr := newTestApp(t, 30, 4, "a\nb\nc\nd\ne", Options{})
	for scr.PostEvent(key(tcell.KeyRune, 'j', 0)) == nil {
	}
	a.Handle(tcell.NewEventMouse(0, 0, tcell.Button1, 0))
	a.Handle(tcell.NewEventMouse(0, 3, tcell.Button1, 0))
	time.Sleep(2 * autoScrollTick)
	for scr.HasPendingEvent() {
		a.Handle(scr.PollEvent())
	}
	got := make(chan tcell.Event, 1)
	go func() { got <- scr.PollEvent() }()
	select {
	case ev := <-got:
		if _, ok := ev.(*Tick); !ok {
			t.Errorf("got %T, want *Tick", ev)
		}
	case <-time.After(time.Second):
		t.Error("tick never arrived")
	}
}

// benchApp builds an app on a 200×60 screen over lines, drawn once.
func benchApp(b *testing.B, lines ...buffer.Line) *App {
	scr := tcell.NewSimulationScreen("UTF-8")
	if err := scr.Init(); err != nil {
		b.Fatal(err)
	}
	b.Cleanup(scr.Fini)
	scr.SetSize(200, 60)
	buf := buffer.New()
	for _, l := range lines {
		buf.Append(l)
	}
	buf.Finish(nil, true)
	a := New(scr, buf, Options{})
	a.Draw()
	return a
}

// BenchmarkDrawHighlight redraws a 1 MB line wrapped over the screen
// with search highlighting on, which must search the line once per
// draw, not once per row.
func BenchmarkDrawHighlight(b *testing.B) {
	a := benchApp(b, buffer.Line{Text: []rune(strings.Repeat("abcdefgh ", 1<<17))})
	a.matcher = search.New("zzz")
	a.highlight = true
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		a.Draw()
	}
}

// BenchmarkDrawRuns redraws lines styled rune by rune, which must not
// scan every run for every rune.
func BenchmarkDrawRuns(b *testing.B) {
	var sb strings.Builder
	for i := 0; i < 200; i++ {
		sb.WriteString("\x1b[3" + string(rune('1'+i%7)) + "mx")
	}
	var lines []buffer.Line
	for i := 0; i < 60; i++ {
		text, runs := ansi.NewParser().Parse([]byte(sb.String()))
		lines = append(lines, buffer.Line{Text: text, Runs: runs})
	}
	a := benchApp(b, lines...)
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		a.Draw()
	}
}

// newOnePageApp is a -F app on a 40×3 screen whose input is still
// open, counting Paging calls in *paging.
func newOnePageApp(t *testing.T, paging *int) *App {
	t.Helper()
	scr := tcell.NewSimulationScreen("UTF-8")
	if err := scr.Init(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(scr.Fini)
	scr.SetSize(40, 3)
	return New(scr, buffer.New(), Options{QuitIfOnePage: true, Paging: func() { *paging++ }})
}

// TestPagingWhenTooLong: -F gives up as soon as the text outgrows the
// screen, before EOF, and says so once.
func TestPagingWhenTooLong(t *testing.T) {
	paging := 0
	a := newOnePageApp(t, &paging)
	for _, s := range []string{"1", "2", "3"} {
		a.buf.Append(buffer.Line{Text: []rune(s)})
		if a.Handle(tcell.NewEventInterrupt(nil)) {
			t.Fatal("quit before EOF")
		}
	}
	if paging != 1 {
		t.Fatalf("Paging called %d times before EOF, want 1", paging)
	}
	a.buf.Finish(nil, true)
	if a.Handle(tcell.NewEventInterrupt(nil)) || a.PrintText() {
		t.Error("quit at EOF though the text does not fit")
	}
	if paging != 1 {
		t.Errorf("Paging called %d times, want 1", paging)
	}
}

// TestPagingOnKey: a key gives -F up.
func TestPagingOnKey(t *testing.T) {
	paging := 0
	a := newOnePageApp(t, &paging)
	press(a, key(tcell.KeyDown, 0, 0), key(tcell.KeyDown, 0, 0))
	if paging != 1 {
		t.Errorf("Paging called %d times, want 1", paging)
	}
}

func TestPrintTextWhenFits(t *testing.T) {
	paging := 0
	a := newOnePageApp(t, &paging)
	a.buf.Append(buffer.Line{Text: []rune("1")})
	a.buf.Finish(nil, true)
	if !a.Handle(tcell.NewEventInterrupt(nil)) || !a.PrintText() {
		t.Error("did not quit to print")
	}
	if paging != 0 {
		t.Error("Paging called")
	}
}

// TestReadErrorKeepsPager: -F stays on a read error, so it is seen.
func TestReadErrorKeepsPager(t *testing.T) {
	paging := 0
	a := newOnePageApp(t, &paging)
	a.buf.Append(buffer.Line{Text: []rune("1")})
	a.buf.Finish(fmt.Errorf("disk on fire"), true)
	if a.Handle(tcell.NewEventInterrupt(nil)) || a.PrintText() {
		t.Error("quit despite the read error")
	}
	if paging != 1 {
		t.Errorf("Paging called %d times, want 1", paging)
	}
}
