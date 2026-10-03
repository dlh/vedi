package app

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/gdamore/tcell/v3"
	"go.dlh.dev/vedi/internal/buffer"
	"go.dlh.dev/vedi/internal/clipboard"
	"go.dlh.dev/vedi/internal/layout"
	"go.dlh.dev/vedi/internal/search"
	"go.dlh.dev/vedi/internal/testscreen"
)

// newTestApp builds an app on a w×h test screen with input fully
// read, +N/+G applied, and the first frame drawn.
func newTestApp(t *testing.T, w, h int, input string, opts Options) (*App, *testscreen.Screen) {
	t.Helper()
	scr := testscreen.New(t, w, h)
	buf := buffer.New()
	buffer.Fill(strings.NewReader(input), buf, func() {})
	if opts.Copier == nil {
		opts.Copier = clipboard.OSC52{Screen: scr}
	}
	a := newApp(t, scr, buf, opts)
	a.Handle(tcell.NewEventInterrupt(nil))
	a.Draw()
	return a, scr
}

// newApp is New, stopped before the test's screen finishes.
func newApp(t testing.TB, scr tcell.Screen, buf *buffer.Buffer, opts Options) *App {
	a := New(scr, buf, opts)
	t.Cleanup(a.Stop)
	return a
}

func key(k tcell.Key, r rune, mod tcell.ModMask) *tcell.EventKey {
	str := ""
	if r != 0 {
		str = string(r)
	}
	return tcell.NewEventKey(k, str, mod)
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
func row(scr *testscreen.Screen, y int) string { return scr.Row(y) }

func cellStyle(scr *testscreen.Screen, x, y int) tcell.Style { return scr.StyleAt(x, y) }

func TestDrawStyles(t *testing.T) {
	_, scr := newTestApp(t, 20, 5, "\x1b[31mred", Options{})
	fg := cellStyle(scr, 0, 0).GetForeground()
	if fg != tcell.PaletteColor(1) {
		t.Errorf("fg = %v, want red", fg)
	}
}

// TestTitleNamesStdin: an OSC 2 title in the input names stdin on the
// status line; a file keeps its name.
func TestTitleNamesStdin(t *testing.T) {
	input := "\x1b]2;~/src\x07text"
	_, scr := newTestApp(t, 30, 3, input, Options{Names: []string{Stdin}})
	if got := row(scr, 2); !strings.HasPrefix(got, "~/src  line 1/1") {
		t.Errorf("stdin status = %q, want ~/src first", got)
	}
	_, scr = newTestApp(t, 30, 3, input, Options{Names: []string{"log.txt"}})
	if got := row(scr, 2); !strings.HasPrefix(got, "log.txt  line 1/1") {
		t.Errorf("file status = %q, want log.txt first", got)
	}
	_, scr = newTestApp(t, 30, 3, "text", Options{Names: []string{Stdin}})
	if got := row(scr, 2); !strings.HasPrefix(got, "<stdin>  line 1/1") {
		t.Errorf("untitled status = %q, want <stdin> first", got)
	}
}

// TestTitleFollowsCursor: a stream that sets the title more than once
// is named by the one in effect at the cursor's line.
func TestTitleFollowsCursor(t *testing.T) {
	input := "\x1b]2;README.md\x1b\\# vedi\nA pager.\n\x1b]2;SECURITY.md\x1b\\# Security\n"
	a, scr := newTestApp(t, 30, 4, input, Options{Names: []string{Stdin}})
	for _, want := range []string{"README.md  line 1/3", "README.md  line 2/3", "SECURITY.md  line 3/3"} {
		if got := row(scr, 3); !strings.HasPrefix(got, want) {
			t.Errorf("status = %q, want %q first", got, want)
		}
		press(a, key(tcell.KeyDown, 0, 0))
	}
	press(a, key(tcell.KeyUp, 0, 0))
	if got := row(scr, 3); !strings.HasPrefix(got, "README.md  line 2/3") {
		t.Errorf("status after Up = %q, want README.md", got)
	}
}

// TestSeparatorStyle: the row naming an input draws the name bold and
// the dashes dim, so the name reads at a glance and the rule recedes.
func TestSeparatorStyle(t *testing.T) {
	scr := testscreen.New(t, 20, 4)
	buf := buffer.New()
	buf.StartPart()
	buf.Write([]byte("a\n"))
	buf.StartPart()
	buf.Write([]byte("b\n"))
	buf.Finish(nil, true)
	a := newApp(t, scr, buf, Options{Names: []string{"a.txt", "b.txt"}, FileSeparators: true, Copier: clipboard.OSC52{Screen: scr}})
	a.Handle(tcell.NewEventInterrupt(nil))
	a.Draw()
	if got := row(scr, 0); got != "── a.txt ───────────" {
		t.Fatalf("row 0 = %q", got)
	}
	dim, bold := tcell.StyleDefault.Dim(true), tcell.StyleDefault.Bold(true)
	for x, want := range map[int]tcell.Style{0: dim, 2: dim, 3: bold, 7: bold, 8: dim, 19: dim} {
		if got := cellStyle(scr, x, 0); got != want {
			t.Errorf("cell %d style = %v, want %v", x, got, want)
		}
	}
}

// TestTitleNamesStdinAmongFiles: a "-" among the files is named by its
// own title, and only while the cursor is in it.
func TestTitleNamesStdinAmongFiles(t *testing.T) {
	scr := testscreen.New(t, 30, 4)
	buf := buffer.New()
	buf.StartPart()
	buf.Write([]byte("\x1b]2;A\x07a\n"))
	buf.StartPart()
	buf.Write([]byte("\x1b]2;~/src\x07s\n"))
	buf.StartPart()
	buf.Write([]byte("b\n"))
	buf.Finish(nil, true)
	a := newApp(t, scr, buf, Options{Names: []string{"a.txt", Stdin, "b.txt"}, Copier: clipboard.OSC52{Screen: scr}})
	a.Handle(tcell.NewEventInterrupt(nil))
	a.Draw()
	for _, want := range []string{"a.txt 1/3  line 1/1", "~/src 2/3  line 1/1", "b.txt 3/3  line 1/1"} {
		if got := row(scr, 3); !strings.HasPrefix(got, want) {
			t.Errorf("status = %q, want %q first", got, want)
		}
		press(a, key(tcell.KeyDown, 0, 0))
	}
}

func TestDrawLinks(t *testing.T) {
	_, scr := newTestApp(t, 20, 5, "\x1b]8;;http://x\x1b\\a\x1b]8;;\x1b\\b", Options{})
	if got, want := cellStyle(scr, 0, 0), tcell.StyleDefault.Url("http://x"); !testscreen.SameStyle(got, want) {
		t.Errorf("cell 0 style = %v, want %v", got, want)
	}
	if got := cellStyle(scr, 1, 0); got != tcell.StyleDefault {
		t.Errorf("cell 1 style = %v, want default", got)
	}
}

func TestHighlightKeepsLink(t *testing.T) {
	a, scr := newTestApp(t, 20, 5, "\x1b]8;id=k;http://x\x1b\\a\x1b]8;;\x1b\\a", Options{})
	press(a, key(tcell.KeyRune, '/', 0), key(tcell.KeyRune, 'a', 0), key(tcell.KeyEnter, 0, 0))
	if got, want := cellStyle(scr, 0, 0), MatchStyle.Url("http://x").UrlId("k"); !testscreen.SameStyle(got, want) {
		t.Errorf("matched link style = %v, want %v", got, want)
	}
	if got := cellStyle(scr, 1, 0); got != MatchStyle {
		t.Errorf("matched plain style = %v, want %v", got, MatchStyle)
	}
	press(a, key(tcell.KeyCtrlA, 0, tcell.ModCtrl))
	if got, want := cellStyle(scr, 0, 0), selStyle.Url("http://x").UrlId("k"); !testscreen.SameStyle(got, want) {
		t.Errorf("selected link style = %v, want %v", got, want)
	}
	if got := cellStyle(scr, 1, 0); got != selStyle {
		t.Errorf("selected plain style = %v, want %v", got, selStyle)
	}
}

// TestLinkStyleIsStable: tcell compares a style's link by pointer and
// sends a cell again when its style differs, so a linked cell drawn
// again must get the very style it had, and cells of one link share it.
func TestLinkStyleIsStable(t *testing.T) {
	a, scr := newTestApp(t, 20, 5, "\x1b]8;id=k;http://x\x1b\\aab\x1b]8;;\x1b\\", Options{})
	check := func(what string) {
		t.Helper()
		before := cellStyle(scr, 0, 0)
		if _, url := before.GetUrl(); url != "http://x" {
			t.Fatalf("%s: url = %q", what, url)
		}
		if got := cellStyle(scr, 1, 0); got != before {
			t.Errorf("%s: two cells of one link have two styles", what)
		}
		a.Draw()
		if got := cellStyle(scr, 0, 0); got != before {
			t.Errorf("%s: style changed on a redraw", what)
		}
	}
	check("plain")
	press(a, key(tcell.KeyRune, '/', 0), key(tcell.KeyRune, 'a', 0), key(tcell.KeyEnter, 0, 0))
	check("matched")
	press(a, key(tcell.KeyCtrlA, 0, tcell.ModCtrl))
	check("selected")
}

func clip(scr *testscreen.Screen) string { return scr.Clipboard() }

func TestReadErrorInStatus(t *testing.T) {
	a, scr := newTestApp(t, 30, 4, "", Options{})
	a.buf.Write([]byte("partial"))
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
	if !scr.Pending() {
		t.Fatal("Notify should post an event")
	}
	a.Handle(<-scr.EventQ())
	if scr.Pending() {
		t.Error("second Notify before a draw should be coalesced")
	}
	a.Notify()
	if !scr.Pending() {
		t.Error("Notify after handling should post again")
	}
}

// readingApp is an app over a buffer still being read, drawn once.
func readingApp(t *testing.T) (*App, *testscreen.Screen) {
	t.Helper()
	scr := testscreen.New(t, 10, 4)
	a := newApp(t, scr, buffer.New(), Options{})
	a.buf.Write([]byte("x\n"))
	a.Notify()
	a.Handle(<-scr.EventQ())
	a.Draw()
	return a, scr
}

// TestNotifyThrottledWhileReading: a Notify within redrawEvery of the
// last data taken up posts nothing; the redraw comes when it elapses.
func TestNotifyThrottledWhileReading(t *testing.T) {
	a, scr := readingApp(t)
	a.Notify()
	if scr.Pending() {
		t.Fatal("Notify within redrawEvery should be held")
	}
	time.Sleep(2 * redrawEvery)
	if !scr.Pending() {
		t.Fatal("held Notify should post after redrawEvery")
	}
}

// TestNotifyAtEOFPostsAtOnce: the end of input is not held back.
func TestNotifyAtEOFPostsAtOnce(t *testing.T) {
	a, scr := readingApp(t)
	a.buf.Finish(nil, true)
	a.Notify()
	if !scr.Pending() {
		t.Fatal("Notify at EOF should post at once")
	}
}

// TestNotifyAtEOFFlushesHeld: EOF posts at once even when an earlier
// Notify is being held.
func TestNotifyAtEOFFlushesHeld(t *testing.T) {
	a, scr := readingApp(t)
	a.Notify()
	if scr.Pending() {
		t.Fatal("Notify within redrawEvery should be held")
	}
	a.buf.Finish(nil, true)
	a.Notify()
	if !scr.Pending() {
		t.Fatal("Notify at EOF should post at once")
	}
}

// TestNotifyHeldPostsOnceAfterKey: a key taking up the data during a
// hold, and a new Notify after it, still make one post.
func TestNotifyHeldPostsOnceAfterKey(t *testing.T) {
	a, scr := readingApp(t)
	a.Notify()
	a.Handle(key(tcell.KeyRune, 'j', 0))
	a.Notify()
	time.Sleep(2 * redrawEvery)
	n := 0
	for scr.Pending() {
		<-scr.EventQ()
		n++
	}
	if n != 1 {
		t.Fatalf("posts = %d, want 1", n)
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
	fg := cellStyle(scr, 0, 0).GetForeground()
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
	if fg := before.GetForeground(); fg != tcell.PaletteColor(1) {
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
	scr := testscreen.New(b, 80, 24)
	buf := buffer.New()
	buf.Write([]byte(strings.Repeat("a", 1<<20)))
	buf.Finish(nil, true)
	a := newApp(b, scr, buf, Options{})
	a.Draw()
	b.ReportAllocs()
	for b.Loop() {
		press(a, key(tcell.KeyRight, 0, 0))
	}
}

// TestNotifyFullQueue: a Notify dropped by a full event queue is still
// acted on once the queue drains, even with no later Notify.
func TestNotifyFullQueue(t *testing.T) {
	a, scr := newTestApp(t, 30, 4, "", Options{})
	for a.Post(key(tcell.KeyRune, 'j', 0)) {
	}
	a.buf.Write([]byte("late"))
	a.buf.Finish(fmt.Errorf("disk on fire"), false)
	a.Notify()
	for scr.Pending() {
		a.Handle(<-scr.EventQ())
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
	for a.Post(key(tcell.KeyRune, 'j', 0)) {
	}
	a.Handle(tcell.NewEventMouse(0, 0, tcell.Button1, 0))
	a.Handle(tcell.NewEventMouse(0, 3, tcell.Button1, 0))
	time.Sleep(2 * autoScrollTick)
	for scr.Pending() {
		a.Handle(<-scr.EventQ())
	}
	select {
	case ev := <-scr.EventQ():
		if _, ok := ev.(*Tick); !ok {
			t.Errorf("got %T, want *Tick", ev)
		}
	case <-time.After(time.Second):
		t.Error("tick never arrived")
	}
}

// benchApp builds an app on a 200×60 screen over lines, drawn once.
func benchApp(b *testing.B, lines ...string) *App {
	scr := testscreen.New(b, 200, 60)
	buf := buffer.New()
	for _, l := range lines {
		buf.Write([]byte(l + "\n"))
	}
	buf.Finish(nil, true)
	a := newApp(b, scr, buf, Options{})
	a.Draw()
	return a
}

// BenchmarkDrawHighlight redraws a 1 MB line wrapped over the screen
// with search highlighting on, which must search the line once per
// draw, not once per row.
func BenchmarkDrawHighlight(b *testing.B) {
	a := benchApp(b, strings.Repeat("abcdefgh ", 1<<17))
	a.matcher = search.New("zzz")
	a.highlight = true
	b.ReportAllocs()
	for b.Loop() {
		a.Draw()
	}
}

// BenchmarkDrawRuns redraws lines styled rune by rune, which must not
// scan every run for every rune.
func BenchmarkDrawRuns(b *testing.B) {
	var sb strings.Builder
	for i := range 200 {
		sb.WriteString("\x1b[3" + string(rune('1'+i%7)) + "mx")
	}
	lines := make([]string, 60)
	for i := range lines {
		lines[i] = sb.String()
	}
	a := benchApp(b, lines...)
	b.ReportAllocs()
	for b.Loop() {
		a.Draw()
	}
}

// styledLines is n colored lines, about 80 bytes each.
func styledLines(n int) []byte {
	var in bytes.Buffer
	for i := range n {
		fmt.Fprintf(&in, "\x1b[32m%08d\x1b[0m some plain text, about eighty bytes wide, \x1b[1mbold\x1b[0m end\n", i)
	}
	return in.Bytes()
}

// BenchmarkFirstDraw reads 100k styled lines and draws the first
// screen: the wait for a large file.
func BenchmarkFirstDraw(b *testing.B) {
	in := styledLines(100_000)
	scr := testscreen.New(b, 200, 60)
	b.SetBytes(int64(len(in)))
	b.ReportAllocs()
	for b.Loop() {
		buf := buffer.New()
		buffer.Fill(bytes.NewReader(in), buf, func() {})
		a := New(scr, buf, Options{})
		a.Handle(tcell.NewEventInterrupt(nil))
		a.Draw()
		a.Stop()
	}
}

// BenchmarkJumpEnd jumps to the last of 100k lines and back, drawing
// each.
func BenchmarkJumpEnd(b *testing.B) {
	lines := strings.Split(strings.TrimSuffix(string(styledLines(100_000)), "\n"), "\n")
	a := benchApp(b, lines...)
	b.ReportAllocs()
	for b.Loop() {
		press(a, key(tcell.KeyRune, 'G', 0), key(tcell.KeyRune, 'g', 0))
	}
}

// unicodeLines is n lines of accented, CJK and emoji text, each under
// 200 cells however clusters are measured.
func unicodeLines(n int) []string {
	lines := make([]string, n)
	for i := range lines {
		switch i % 3 {
		case 0:
			lines[i] = strings.Repeat("café naïve résumé façade coöperate ", 3)
		case 1:
			lines[i] = strings.Repeat("日本語のテキスト、中文文本 ", 4)
		default:
			lines[i] = strings.Repeat("👨\u200d👩\u200d👧\u200d👦 🇺🇸 👍🏽 e\u0301 ", 6)
		}
	}
	return lines
}

// BenchmarkDrawUnicode redraws a screen of accented, CJK and emoji
// text: every glyph goes to tcell with its cluster.
func BenchmarkDrawUnicode(b *testing.B) {
	a := benchApp(b, unicodeLines(60)...)
	b.ReportAllocs()
	for b.Loop() {
		a.Draw()
	}
}

// BenchmarkMoveColUnicode steps right and back over marked text,
// redrawing each time.
func BenchmarkMoveColUnicode(b *testing.B) {
	a := benchApp(b, unicodeLines(3)...)
	b.ReportAllocs()
	for b.Loop() {
		press(a, key(tcell.KeyRight, 0, 0), key(tcell.KeyLeft, 0, 0))
	}
}

// TestOnePageUndecidedBeforeEOF: -F does not decide while text that
// fits is still being read; it may grow.
func TestOnePageUndecidedBeforeEOF(t *testing.T) {
	buf := buffer.New()
	buf.Write([]byte("1\n2\n"))
	if v := OnePage(buf, 40, 3, false); v != Undecided {
		t.Errorf("OnePage = %v, want Undecided", v)
	}
}

// TestOnePagePagesWhenTooLong: text that outgrows the screen is paged
// as soon as it does, before EOF.
func TestOnePagePagesWhenTooLong(t *testing.T) {
	buf := buffer.New()
	buf.Write([]byte("1\n2\n3\n"))
	if v := OnePage(buf, 40, 3, false); v != Page {
		t.Errorf("OnePage = %v, want Page", v)
	}
}

func TestOnePagePrintsWhenFits(t *testing.T) {
	buf := buffer.New()
	buf.Write([]byte("1\n2\n"))
	buf.Finish(nil, true)
	if v := OnePage(buf, 40, 3, false); v != Print {
		t.Errorf("OnePage = %v, want Print", v)
	}
}

// TestOnePageCountsSeparators: with several inputs the row naming
// each takes a row of the screen.
func TestOnePageCountsSeparators(t *testing.T) {
	buf := buffer.New()
	buf.StartPart()
	buf.Write([]byte("1\n"))
	buf.StartPart()
	buf.Write([]byte("2\n"))
	buf.Finish(nil, true)
	if v := OnePage(buf, 40, 3, true); v != Page {
		t.Errorf("OnePage with separators = %v, want Page", v)
	}
	if v := OnePage(buf, 40, 3, false); v != Print {
		t.Errorf("OnePage without separators = %v, want Print", v)
	}
	one := buffer.New()
	one.StartPart()
	one.Write([]byte("1\n2\n"))
	one.Finish(nil, true)
	if v := OnePage(one, 40, 3, true); v != Print {
		t.Errorf("OnePage with one input = %v, want Print: no row names a lone input", v)
	}
}

// TestReloadKeepsSeparatorTop: a top row that is an input's separator
// stays one across a reload; snap drops it if the line no longer
// starts an input.
func TestReloadKeepsSeparatorTop(t *testing.T) {
	twoParts := func() *buffer.Buffer {
		b := buffer.New()
		b.StartPart()
		b.Write([]byte("a\n"))
		b.StartPart()
		b.Write([]byte("b\n"))
		b.Finish(nil, true)
		return b
	}
	tv := textView{buf: twoParts(), cur: buffer.Pos{Line: 1}, top: buffer.Pos{Line: 1, Col: -1}}
	tv.reload(twoParts())
	if want := (buffer.Pos{Line: 1, Col: -1}); tv.top != want {
		t.Errorf("top = %+v, want %+v", tv.top, want)
	}
}

// TestReloadSeparatorTopPastEnd: a separator top past the new end
// starts over from the first row, the first line's separator if it
// has one.
func TestReloadSeparatorTopPastEnd(t *testing.T) {
	long := buffer.New()
	long.StartPart()
	long.Write([]byte("a\nb\nc\n"))
	long.StartPart()
	long.Write([]byte("d\n"))
	long.Finish(nil, true)
	short := buffer.New()
	short.StartPart()
	short.Write([]byte("a\n"))
	short.Finish(nil, true)
	tv := textView{buf: long, cur: buffer.Pos{Line: 3}, top: buffer.Pos{Line: 3, Col: -1}}
	tv.reload(short)
	if want := (buffer.Pos{Col: -1}); tv.top != want {
		t.Errorf("top = %+v, want %+v", tv.top, want)
	}
}

// TestOnePagePagesOnReadError: -F pages on a read error, so it is seen.
func TestOnePagePagesOnReadError(t *testing.T) {
	buf := buffer.New()
	buf.Write([]byte("1\n"))
	buf.Finish(fmt.Errorf("disk on fire"), true)
	if v := OnePage(buf, 40, 3, false); v != Page {
		t.Errorf("OnePage = %v, want Page", v)
	}
}

// growsAtEOF is a buffer whose reader appends 100 lines and finishes
// at the moment EOF is first checked: the narrowest interleaving of a
// read with the measuring.
type growsAtEOF struct {
	*buffer.Buffer
	grown *bool
}

func (g growsAtEOF) Finished() (bool, error) {
	if !*g.grown {
		*g.grown = true
		g.Write([]byte(strings.Repeat("line\n", 100)))
		g.Finish(nil, true)
	}
	return g.Buffer.Finished()
}

// TestOnePageMeasuresAllThatEnded: EOF vouches only for text that was
// measured after it, never for lines that landed with it.
func TestOnePageMeasuresAllThatEnded(t *testing.T) {
	if v := OnePage(growsAtEOF{buffer.New(), new(bool)}, 40, 3, false); v != Page {
		t.Errorf("OnePage = %v, want Page for 100 lines on a three-row screen", v)
	}
}

// TestOnePagePagesOnErrorFoundMeasuring: a file that shrank after it
// was read has no bytes for its lines. Measuring is what finds that,
// after the reader reported no error; -F pages so the error is shown
// rather than printing what is left.
func TestOnePagePagesOnErrorFoundMeasuring(t *testing.T) {
	name := filepath.Join(t.TempDir(), "f")
	if err := os.WriteFile(name, []byte("1\n2\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	f, err := os.Open(name)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { f.Close() })
	buf := buffer.NewFrom(f)
	buffer.Fill(f, buf, func() {})
	if err := os.Truncate(name, 0); err != nil {
		t.Fatal(err)
	}
	if v := OnePage(buf, 40, 3, false); v != Page {
		t.Errorf("OnePage = %v, want Page for a truncated file", v)
	}
	if _, err := buf.Finished(); err == nil {
		t.Error("the truncation was not found while measuring, so the test proves nothing")
	}
}

// TestOnePageOneRowScreen: a one-row screen has no status line, so
// its one row is the text's.
func TestOnePageOneRowScreen(t *testing.T) {
	buf := buffer.New()
	buf.Write([]byte("1\n"))
	buf.Finish(nil, true)
	if v := OnePage(buf, 40, 1, false); v != Print {
		t.Errorf("OnePage = %v, want Print", v)
	}
}

// TestTruncatedFileDraws: a file that shrinks after its layouts are
// kept draws without panicking and reports the read error.
func TestTruncatedFileDraws(t *testing.T) {
	scr := testscreen.New(t, 30, 4)
	name := filepath.Join(t.TempDir(), "f")
	var sb strings.Builder
	for i := range 2000 {
		fmt.Fprintf(&sb, "line %d\n", i)
	}
	if err := os.WriteFile(name, []byte(sb.String()), 0o644); err != nil {
		t.Fatal(err)
	}
	f, err := os.Open(name)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { f.Close() })
	buf := buffer.NewFrom(f)
	buffer.Fill(f, buf, func() {})
	a := newApp(t, scr, buf, Options{Copier: clipboard.OSC52{Screen: scr}})
	a.Handle(tcell.NewEventInterrupt(nil))
	a.Handle(key(tcell.KeyRight, 0, 0))
	a.Handle(key(tcell.KeyRight, 0, 0))
	a.Draw()
	for i := 100; i < 1200; i++ { // evict the decoded lines, not the layouts
		buf.Line(i)
	}
	if err := os.Truncate(name, 0); err != nil {
		t.Fatal(err)
	}
	a.Draw()
	if got := row(scr, 0); got != "" {
		t.Errorf("row 0 = %q, want empty", got)
	}
	if got := row(scr, 3); got != "read error: input truncated" {
		t.Errorf("status = %q", got)
	}
	a.Handle(key(tcell.KeyLeft, 0, 0)) // from past the line's new end
	a.Draw()
	if a.cur != (buffer.Pos{}) {
		t.Errorf("cursor = %v, want 0 0", a.cur)
	}
}

// TestNotifyFromTwoReaders: the startup reader and a reload's notify
// at once, as when a file changes during its first read. Run with
// -race.
func TestNotifyFromTwoReaders(t *testing.T) {
	a, _ := newTestApp(t, 10, 4, "x", Options{})
	unfinished := buffer.New()
	var wg sync.WaitGroup
	for range 2 {
		wg.Add(2)
		go func() {
			defer wg.Done()
			for range 200 {
				a.notify(unfinished)
			}
		}()
		go func() {
			defer wg.Done()
			for range 200 {
				a.notify(nil)
			}
		}()
	}
	wg.Wait()
}

// opener is a test Open: each call makes a new buffer, left for the
// test to finish, and counts opens and closes.
type opener struct {
	bufs   []*buffer.Buffer
	closed int
	err    error
}

func (o *opener) open(notify func()) (*buffer.Buffer, func(), error) {
	if o.err != nil {
		return nil, nil, o.err
	}
	b := buffer.New()
	o.bufs = append(o.bufs, b)
	return b, func() { o.closed++ }, nil
}

// finish ends buffer i with text and delivers its reader's
// notification.
func (o *opener) finish(a *App, i int, text string) {
	o.bufs[i].Write([]byte(text))
	o.bufs[i].Finish(nil, true)
	a.notify(o.bufs[i])
	a.Handle(tcell.NewEventInterrupt(nil))
	a.Draw()
}

func TestReloadPendingStatus(t *testing.T) {
	o := &opener{}
	a, scr := newTestApp(t, 40, 4, "old\n", Options{Open: o.open, AutoReload: true})
	a.Handle(&Changed{})
	a.Draw()
	if got := row(scr, 3); !strings.HasSuffix(got, "  reloading…") {
		t.Errorf("status while pending = %q", got)
	}
	if got := row(scr, 0); got != "old" {
		t.Errorf("text while pending = %q", got)
	}
	o.finish(a, 0, "new\n")
	if got := row(scr, 0); got != "new" {
		t.Errorf("text after reload = %q", got)
	}
	if got := row(scr, 3); strings.Contains(got, "reloading") {
		t.Errorf("status after reload = %q", got)
	}
}

// TestReloadCoalesces: changes during a reload start exactly one more
// after it, and each swapped-out buffer's close runs.
func TestReloadCoalesces(t *testing.T) {
	o := &opener{}
	a, scr := newTestApp(t, 30, 4, "old\n", Options{Open: o.open, AutoReload: true})
	a.Handle(&Changed{})
	a.Handle(&Changed{})
	a.Handle(&Changed{})
	if len(o.bufs) != 1 {
		t.Fatalf("opens during a reload = %d, want 1", len(o.bufs))
	}
	o.finish(a, 0, "one\n")
	if len(o.bufs) != 2 {
		t.Fatalf("opens after the first swap = %d, want 2", len(o.bufs))
	}
	if o.closed != 0 {
		t.Errorf("closes after the first swap = %d, want 0: the startup buffer has none", o.closed)
	}
	o.finish(a, 1, "two\n")
	if len(o.bufs) != 2 {
		t.Errorf("opens after the second swap = %d, want 2", len(o.bufs))
	}
	if o.closed != 1 {
		t.Errorf("closes after the second swap = %d, want 1", o.closed)
	}
	if got := row(scr, 0); got != "two" {
		t.Errorf("text = %q", got)
	}
}

// TestAutoReloadOffDropsQueuedReload: auto_reload no at the : prompt
// drops the reload a change during the running one asked for.
func TestAutoReloadOffDropsQueuedReload(t *testing.T) {
	o := &opener{}
	a, scr := newTestApp(t, 30, 4, "old\n", Options{Open: o.open, AutoReload: true})
	a.Handle(&Changed{})
	a.Handle(&Changed{})
	press(a, key(tcell.KeyRune, ':', 0))
	for _, r := range "auto_reload no" {
		press(a, key(tcell.KeyRune, r, 0))
	}
	press(a, key(tcell.KeyEnter, 0, 0))
	o.finish(a, 0, "one\n")
	if len(o.bufs) != 1 {
		t.Errorf("opens after the swap = %d, want 1", len(o.bufs))
	}
	if got := row(scr, 0); got != "one" {
		t.Errorf("text = %q", got)
	}
}

func TestReloadOpenFailsOnChange(t *testing.T) {
	o := &opener{err: fmt.Errorf("boom")}
	a, scr := newTestApp(t, 30, 4, "old\n", Options{Open: o.open, AutoReload: true})
	a.Handle(&Changed{})
	a.Draw()
	if a.status != "" {
		t.Errorf("status = %q, want none: only the key reports", a.status)
	}
	if got := row(scr, 0); got != "old" {
		t.Errorf("text = %q", got)
	}
}

// TestReloadDuringFirstRead: a reload lands while the startup buffer
// is still being read; the old reader's later data does not come back.
func TestReloadDuringFirstRead(t *testing.T) {
	scr := testscreen.New(t, 30, 4)
	first := buffer.New()
	first.Write([]byte("one\n"))
	o := &opener{}
	a := newApp(t, scr, first, Options{Copier: clipboard.OSC52{Screen: scr}, Open: o.open, AutoReload: true})
	a.Handle(tcell.NewEventInterrupt(nil))
	a.Handle(&Changed{})
	o.finish(a, 0, "two\n")
	if got := row(scr, 0); got != "two" {
		t.Fatalf("text after reload = %q", got)
	}
	first.Write([]byte("more\n"))
	first.Finish(nil, true)
	a.Notify()
	a.Handle(tcell.NewEventInterrupt(nil))
	a.Draw()
	if got := row(scr, 0); got != "two" {
		t.Errorf("text after the old reader's data = %q, want two", got)
	}
	if got := row(scr, 3); strings.Contains(got, "reading") {
		t.Errorf("status = %q: the old reader's state must not show", got)
	}
}

func TestReloadKeyReportsFailure(t *testing.T) {
	o := &opener{err: fmt.Errorf("boom")}
	a, scr := newTestApp(t, 30, 4, "old\n", Options{Open: o.open, AutoReload: true})
	press(a, key(tcell.KeyRune, 'R', 0))
	if got := row(scr, 3); !strings.HasPrefix(got, "reload failed: boom") {
		t.Errorf("status = %q", got)
	}
}

// TestPostAfterStop: tcell closes the queue when the screen finishes.
// A timer or the reader may post after that; once the app is stopped
// the post is dropped, and not to be tried again.
func TestPostAfterStop(t *testing.T) {
	a, scr := newTestApp(t, 10, 4, "x", Options{})
	a.Stop()
	scr.Fini()
	if !a.Post(&Tick{}) {
		t.Error("Post to a stopped app reported a full queue")
	}
	a.Notify()
}

// TestRunEndsWithScreen: the loop returns when the queue closes.
func TestRunEndsWithScreen(t *testing.T) {
	a, scr := newTestApp(t, 10, 4, "x", Options{})
	done := make(chan struct{})
	go func() { a.Run(); close(done) }()
	scr.Fini()
	select {
	case <-done:
	case <-time.After(5 * time.Second):
		t.Fatal("Run did not return")
	}
}

// TestRedrawSendsOnlyChanges: drawing a screen again sends the terminal
// none of its text, wide and linked cells included, and tcell measures
// none of it again.
func TestRedrawSendsOnlyChanges(t *testing.T) {
	a, scr := newTestApp(t, 40, 6, "plain\n日本語 👍🏽 é\n\x1b]8;;http://x\x1b\\link\x1b]8;;\x1b\\\n\ttab \x01\n", Options{})
	scr.Sent()
	a.Draw()
	out := scr.Sent()
	for _, s := range []string{"plain", "日", "👍", "é", "link", "tab", "^A", "stdin", "help"} {
		if strings.Contains(out, s) {
			t.Errorf("redraw sent %q again", s)
		}
	}
	// Moving the cursor off the first row changes the status row alone.
	press(a, key(tcell.KeyDown, 0, 0))
	out = scr.Sent()
	if !strings.Contains(out, "2") {
		t.Errorf("the new line number was not sent: %q", out)
	}
	for _, s := range []string{"plain", "日", "link"} {
		if strings.Contains(out, s) {
			t.Errorf("a cursor move sent %q again", s)
		}
	}
}

// TestDrawBlanksWhatItLeaves: with nothing cleared first, cells the
// last frame drew and this one does not are blanked.
func TestDrawBlanksWhatItLeaves(t *testing.T) {
	o := &opener{}
	a, scr := newTestApp(t, 20, 4, "日本語日本語\nsecond line\nthird\n", Options{Open: o.open, AutoReload: true})
	a.Handle(&Changed{})
	o.finish(a, 0, "x\n")
	for y, want := range []string{"x", "", ""} {
		if got := row(scr, y); got != want {
			t.Errorf("row %d = %q, want %q", y, got, want)
		}
	}
	for x := range 20 {
		if got := cellStyle(scr, x, 1); got != tcell.StyleDefault {
			t.Fatalf("blanked cell %d has style %v", x, got)
		}
	}
}

// BenchmarkSearchNearLongTail searches to a match just below the
// screen, nothing laid out yet, which must not lay out a 4 MiB last
// line far below it.
func BenchmarkSearchNearLongTail(b *testing.B) {
	lines := make([]string, 0, 1001)
	for range 1000 {
		lines = append(lines, "line")
	}
	lines[100] = "foo"
	lines = append(lines, strings.Repeat("a", 4<<20))
	a := benchApp(b, lines...)
	press(a, key(tcell.KeyRune, '/', 0), key(tcell.KeyRune, 'f', 0), key(tcell.KeyRune, 'o', 0), key(tcell.KeyRune, 'o', 0), key(tcell.KeyEnter, 0, 0))
	b.ReportAllocs()
	for b.Loop() {
		press(a, key(tcell.KeyRune, 'g', 0))
		a.laidOut = nil
		press(a, key(tcell.KeyRune, 'n', 0))
	}
}
