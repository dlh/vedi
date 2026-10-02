package app_test

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/gdamore/tcell/v3"
	"go.dlh.dev/vedi/internal/app"
	"go.dlh.dev/vedi/internal/buffer"
	"go.dlh.dev/vedi/internal/cli"
	"go.dlh.dev/vedi/internal/config"
	"go.dlh.dev/vedi/internal/testscreen"
)

// stepFile is bin/spec-step's: TestSpecs runs this one scenario and
// prints it as it goes.
var stepFile = flag.String("step", "", "scenario `file` to step through")

// TestSpecs runs every scenario file under specs/. The format is
// described in specs/README.md.
func TestSpecs(t *testing.T) {
	files, err := filepath.Glob("../../specs/*/*.txt")
	if err != nil || len(files) == 0 {
		t.Fatalf("no scenario files found: %v", err)
	}
	if *stepFile != "" {
		files = []string{*stepFile}
	}
	for _, path := range files {
		name, _ := filepath.Rel("../../specs", path)
		t.Run(name, func(t *testing.T) {
			data, err := os.ReadFile(path)
			if err != nil {
				t.Fatal(err)
			}
			a := parseArchive(string(data))
			var step *stepper
			if *stepFile != "" {
				step = newStepper(a)
			}
			if err := runScenario(t, a, step); err != nil {
				t.Fatal(err)
			}
		})
	}
}

type scenario struct {
	t        *testing.T
	scr      *testscreen.Screen
	buf      *buffer.Buffer
	app      *app.App
	w, h     int
	args     []string
	macOS    bool
	config   config.Config
	nl       bool   // the last input section ended with a newline
	file     bool   // the args name files, so the app has an Open
	reloads  bool   // and a change on disk is delivered: the watcher would run
	disk     string // what the file holds: the input, then each file or reload body
	diskNL   bool   // and it ended with a newline
	hasEOF   bool   // the file has an eof section, so input stays open
	finished bool   // an eof section has run
	started  bool   // the setup is frozen: an action or assertion has run
	onePage  bool   // -F: the pager opens only once the text is known not to fit
	opts     cli.Options
	files    []string
	quit     bool
	now      time.Time // the app's clock, advanced by mouse actions
	moved    time.Time // the last motion, which arms the auto-scroll timer
	step     *stepper  // prints the scenario as it runs, or nil
}

// runScenario runs one scenario and returns the first failure, a
// malformed section or a failed assertion, prefixed with its line. t
// only cleans up the screen. step, if not nil, is told of each section
// once it has run.
func runScenario(t *testing.T, a archive, step *stepper) (err error) {
	// The copier must not depend on the terminal running the tests.
	t.Setenv("TERM_PROGRAM", "")
	if strings.TrimSpace(a.comment) == "" {
		return fmt.Errorf("a scenario starts with prose describing the behavior")
	}
	s := &scenario{t: t, buf: buffer.New(), w: 40, h: 6, step: step}
	for _, sec := range a.sections {
		if sec.name == "eof" {
			s.hasEOF = true
		}
	}
	inputSeen := false
	ran := -1
	if step != nil {
		defer func() {
			if ran >= 0 {
				step.after(s, err)
			}
		}()
	}
	for i, sec := range a.sections {
		if step != nil && i > 0 {
			step.after(s, nil)
		}
		ran = i
		fail := func(format string, args ...any) error {
			return fmt.Errorf("line %d, -- %s --: %s", sec.line, sec.name, fmt.Sprintf(format, args...))
		}
		if s.quit && actions[sec.name] {
			return fail("%s after the app quit", sec.name)
		}
		started := s.started
		switch sec.name {
		case "size":
			if s.started {
				return fail("must come before the app starts")
			}
			if _, err := fmt.Sscanf(strings.TrimSpace(sec.body), "%dx%d", &s.w, &s.h); err != nil {
				return fail("want WxH, got %q", sec.body)
			}
		case "args":
			if s.started {
				return fail("must come before the app starts")
			}
			s.args = strings.Fields(sec.body)
		case "os":
			if s.started {
				return fail("must come before the app starts")
			}
			switch os := strings.TrimSpace(sec.body); os {
			case "macos", "linux":
				s.macOS = os == "macos"
			default:
				return fail("want macos or linux, got %q", os)
			}
		case "config":
			if s.started {
				return fail("must come before the app starts")
			}
			cfg, err := config.Parse("config", []byte(sec.body))
			if err != nil {
				return fail("%v", err)
			}
			s.config = cfg
		case "input":
			if s.finished {
				return fail("input after eof")
			}
			if !s.started && !inputSeen {
				inputSeen = true
				s.input(sec.body)
				continue
			}
			if err := s.start(); err != nil {
				return err
			}
			s.input(sec.body)
			if err := s.deliver(); err != nil {
				return err
			}
		case "eof":
			if s.finished {
				return fail("input already finished")
			}
			s.finished = true
			if err := s.start(); err != nil {
				return err
			}
			s.buf.Finish(nil, s.nl)
			if err := s.deliver(); err != nil {
				return err
			}
		case "file", "reload":
			if err := s.ready(); err != nil {
				return fail("%v", err)
			}
			if !s.file {
				return fail("%s needs a file in -- args --", sec.name)
			}
			s.disk, s.diskNL = lines(sec.body)
			if sec.name == "reload" && s.reloads {
				s.app.Handle(&app.Changed{})
				// The reload's reader notifies at EOF, as the real one does.
				s.notify()
			}
		case "keys":
			if err := s.ready(); err != nil {
				return fail("%v", err)
			}
			keys, err := parseKeys(sec.body)
			if err != nil {
				return fail("%v", err)
			}
			for _, k := range keys {
				s.quit = s.app.Handle(k)
				s.app.Draw()
			}
		case "mouse":
			if err := s.ready(); err != nil {
				return fail("%v", err)
			}
			acts, err := parseMouse(sec.body)
			if err != nil {
				return fail("%v", err)
			}
			for _, a := range acts {
				s.mouse(a)
			}
		case "resize":
			if err := s.ready(); err != nil {
				return fail("%v", err)
			}
			var w, h int
			if _, err := fmt.Sscanf(strings.TrimSpace(sec.body), "%dx%d", &w, &h); err != nil {
				return fail("want WxH, got %q", sec.body)
			}
			s.scr.SetTermSize(s.t, w, h)
			s.app.Handle(tcell.NewEventResize(w, h))
			s.app.Draw()
		case "screen":
			if err := s.ready(); err != nil {
				return fail("%v", err)
			}
			if got := dump(s.scr); got != sec.body {
				return fail("screen differs\n%s", sideBySide(sec.body, got))
			}
		case "cursor":
			if err := s.ready(); err != nil {
				return fail("%v", err)
			}
			want := strings.TrimSpace(sec.body)
			x, y, vis := s.scr.Cursor()
			got := "hidden"
			if vis {
				got = fmt.Sprintf("%d %d", y, x)
			}
			if got != want {
				return fail("cursor = %s, want %s", got, want)
			}
		case "clipboard":
			if err := s.ready(); err != nil {
				return fail("%v", err)
			}
			want := strings.TrimSuffix(sec.body, "\n")
			if got := s.scr.Clipboard(); got != want {
				return fail("clipboard = %q, want %q", got, want)
			}
		case "quit":
			if err := s.start(); err != nil {
				return err
			}
			if !s.quit {
				return fail("the app did not quit")
			}
		default:
			return fail("unknown section")
		}
		// Starting the app can quit it too: -F at EOF before the first draw.
		if s.quit && sec.name != "quit" && (actions[sec.name] || !started) && (i+1 >= len(a.sections) || a.sections[i+1].name != "quit") {
			return fail("the app quit; the next section must be -- quit --")
		}
	}
	return nil
}

// actions are the sections that drive the app, so none may follow a
// quit.
var actions = map[string]bool{"input": true, "eof": true, "file": true, "reload": true, "keys": true, "mouse": true, "resize": true}

// input appends the body to the buffer and to the disk text.
func (s *scenario) input(body string) {
	text, nl := lines(body)
	if text == "" {
		return
	}
	s.nl = nl
	s.buf.Write([]byte(text))
	s.disk, s.diskNL = s.disk+text, nl
}

// lines is a section body as file bytes. txtar's trailing newline is
// stripped; a second one means the text itself ended with a newline,
// which Finish is told. Every line is ended, so the buffer sees each
// as a line at once.
func lines(body string) (text string, nl bool) {
	if body == "" {
		return "", false
	}
	text = strings.TrimSuffix(body, "\n")
	nl = strings.HasSuffix(text, "\n")
	return strings.TrimSuffix(text, "\n") + "\n", nl
}

// start freezes the setup the first time an action or assertion needs
// the app, and opens the pager. Without an eof section the input is
// complete. With -F the pager opens only once the text is known not
// to fit, so it may not open yet: see deliver.
func (s *scenario) start() error {
	if s.started {
		return nil
	}
	s.started = true
	if !s.hasEOF {
		s.buf.Finish(nil, s.nl)
	}
	var err error
	if s.opts, s.files, err = cli.Parse(s.args); err != nil {
		return fmt.Errorf("args %q: %v", s.args, err)
	}
	s.onePage = s.opts.QuitIfOnePage
	return s.deliver()
}

// deliver is the reader's notification, as main handles it: to the app
// once the pager is open, else to -F, which opens the pager when the
// text outgrows the screen and quits to print it when the input ends
// first.
func (s *scenario) deliver() error {
	if s.app != nil {
		s.notify()
		return nil
	}
	if s.onePage {
		switch app.OnePage(s.buf, s.w, s.h) {
		case app.Undecided:
			return nil
		case app.Print:
			s.quit = true
			return nil
		}
	}
	return s.open()
}

// ready starts the scenario and reports an error when there is no
// pager to act on or assert about.
func (s *scenario) ready() error {
	if err := s.start(); err != nil {
		return err
	}
	switch {
	case s.app != nil:
		return nil
	case s.quit:
		return fmt.Errorf("-F printed the text; the pager never opened")
	}
	return fmt.Errorf("-F is waiting for EOF; the pager has not opened")
}

// open builds the screen and app and delivers the first notification.
func (s *scenario) open() error {
	opts, files := s.opts, s.files
	s.scr = testscreen.New(s.t, s.w, s.h)
	appOpts := opts.App(s.scr, files, s.config)
	appOpts.Now = func() time.Time { return s.now }
	appOpts.MacOS = s.macOS
	appOpts.Keys = s.config.Keymap(s.macOS)
	s.reloads = opts.Reloads(s.config)
	if s.file = len(files) > 0 && !slices.Contains(files, "-"); s.file {
		// Open is the disk text, finished: a reload lands at once.
		appOpts.Open = func(func()) (*buffer.Buffer, func(), error) {
			b := buffer.New()
			b.Write([]byte(s.disk))
			b.Finish(nil, s.diskNL)
			return b, func() {}, nil
		}
	}
	s.app = app.New(s.scr, s.buf, appOpts)
	s.t.Cleanup(s.app.Stop)
	s.notify()
	if s.step != nil {
		s.step.opened(s)
	}
	return nil
}

// mouse delivers one action as the events a terminal would send, or
// for tick the auto-scroll timer's, armed at the last motion. The clock
// moves on a second before every click or press, so two in a row are
// never a double-click; dblclick and tripleclick press two or three
// times without moving it.
func (s *scenario) mouse(a mouseAction) {
	var mod tcell.ModMask
	if a.shift {
		mod = tcell.ModShift
	}
	send := func(x, y int, btn tcell.ButtonMask) {
		s.app.Handle(tcell.NewEventMouse(x, y, btn, mod))
		s.app.Draw()
	}
	switch a.kind {
	case "click", "dblclick", "tripleclick":
		s.now = s.now.Add(time.Second)
		for i := 0; i < map[string]int{"click": 1, "dblclick": 2, "tripleclick": 3}[a.kind]; i++ {
			send(a.x, a.y, tcell.Button1)
			send(a.x, a.y, tcell.ButtonNone)
		}
	case "press":
		s.now = s.now.Add(time.Second)
		send(a.x, a.y, tcell.Button1)
	case "release":
		send(a.x, a.y, tcell.ButtonNone)
	case "move":
		s.moved = s.now
		send(a.x, a.y, tcell.Button1)
	case "drag":
		s.now = s.now.Add(time.Second)
		s.moved = s.now
		send(a.x, a.y, tcell.Button1)
		send(a.x2, a.y2, tcell.Button1)
		send(a.x2, a.y2, tcell.ButtonNone)
	case "wheel":
		btn := map[string]tcell.ButtonMask{
			"up": tcell.WheelUp, "down": tcell.WheelDown,
			"left": tcell.WheelLeft, "right": tcell.WheelRight,
		}[a.dir]
		for i := 0; i < a.n; i++ {
			s.app.Handle(tcell.NewEventMouse(0, 0, btn, mod))
			s.app.Draw()
		}
	case "tick":
		for i := 0; i < a.n; i++ {
			t := &app.Tick{}
			t.SetEventTime(s.moved)
			s.app.Handle(t)
			s.app.Draw()
		}
	}
}

// notify delivers the reader's data event, as buffer.Fill would.
func (s *scenario) notify() {
	s.quit = s.app.Handle(tcell.NewEventInterrupt(nil))
	s.app.Draw()
}

// dump renders the screen as the -- screen -- section expects it:
// reverse-video runs in brackets, search matches in braces, the status
// row plain, trailing spaces trimmed, one line per row.
func dump(scr *testscreen.Screen) string {
	w, h := scr.Size()
	var sb strings.Builder
	for y := range h {
		var row strings.Builder
		status := h >= 2 && y == h-1
		rev, match := false, false
		for x := 0; x < w; {
			str, st, cw := scr.Get(x, y)
			x += cw // a wide cell's second half holds nothing of its own
			if !status {
				r, m := st.HasReverse(), st == app.MatchStyle
				if r && !rev {
					row.WriteByte('[')
				} else if !r && rev {
					row.WriteByte(']')
				}
				if m && !match {
					row.WriteByte('{')
				} else if !m && match {
					row.WriteByte('}')
				}
				rev, match = r, m
			}
			row.WriteString(str)
		}
		if rev {
			row.WriteByte(']')
		}
		if match {
			row.WriteByte('}')
		}
		sb.WriteString(strings.TrimRight(row.String(), " "))
		sb.WriteByte('\n')
	}
	return sb.String()
}

// sideBySide lists want and got rows, marking the ones that differ.
func sideBySide(want, got string) string {
	w := strings.Split(strings.TrimSuffix(want, "\n"), "\n")
	g := strings.Split(strings.TrimSuffix(got, "\n"), "\n")
	var sb strings.Builder
	for i := 0; i < max(len(w), len(g)); i++ {
		var a, b string
		if i < len(w) {
			a = w[i]
		}
		if i < len(g) {
			b = g[i]
		}
		mark := " "
		if a != b {
			mark = "!"
		}
		fmt.Fprintf(&sb, "%s %2d want %-40q got %q\n", mark, i, a, b)
	}
	return sb.String()
}

// TestRunScenarioRejects checks that a failed assertion, a broken quit
// contract and a misspelled section each fail the scenario, so a green
// suite means the assertions ran.
func TestRunScenarioRejects(t *testing.T) {
	const ok = "Text.\n-- input --\nhi\n-- screen --\nhi\n\n\n\n\n<stdin>  line 1/1  wrap           h help\n"
	if err := runScenario(t, parseArchive(ok), nil); err != nil {
		t.Fatalf("control scenario failed: %v", err)
	}
	cases := []struct{ name, text, want string }{
		{"wrong screen", "T\n-- input --\nhi\n-- screen --\nho\n", "line 4, -- screen --: screen differs"},
		{"wrong cursor", "T\n-- input --\nhi\n-- cursor --\n0 1\n", "line 4, -- cursor --: cursor = 0 0, want 0 1"},
		{"wrong clipboard", "T\n-- input --\nhi\n-- keys --\nCtrl+A Enter\n-- quit --\n-- clipboard --\nho\n", `line 7, -- clipboard --: clipboard = "hi", want "ho"`},
		{"quit without section", "T\n-- input --\nhi\n-- keys --\nq\n", "line 4, -- keys --: the app quit; the next section must be -- quit --"},
		{"quit section without quit", "T\n-- input --\nhi\n-- keys --\nDown\n-- quit --\n", "line 6, -- quit --: the app did not quit"},
		{"keys after quit", "T\n-- input --\nhi\n-- keys --\nq\n-- quit --\n-- keys --\nDown\n", "line 7, -- keys --: keys after the app quit"},
		{"mouse after quit", "T\n-- input --\nhi\n-- keys --\nq\n-- quit --\n-- mouse --\nclick 0 0\n", "line 7, -- mouse --: mouse after the app quit"},
		{"bad mouse action", "T\n-- input --\nhi\n-- mouse --\ntap 0 0\n", `line 4, -- mouse --: unknown mouse action "tap"`},
		{"unknown section", "T\n-- input --\nhi\n-- screeen --\nho\n", "line 4, -- screeen --: unknown section"},
		{"bad os", "T\n-- os --\nwindows\n-- input --\nhi\n", `line 2, -- os --: want macos or linux, got "windows"`},
		{"os after start", "T\n-- input --\nhi\n-- keys --\nDown\n-- os --\nmacos\n", "line 6, -- os --: must come before the app starts"},
		{"config after start", "T\n-- input --\nhi\n-- keys --\nDown\n-- config --\nmap q none\n", "line 6, -- config --: must come before the app starts"},
		{"bad config", "T\n-- config --\nmap q nope\n-- input --\nhi\n", `line 2, -- config --: config:1: unknown action "nope"`},
		{"reload without file", "T\n-- input --\nhi\n-- reload --\nho\n", "line 4, -- reload --: reload needs a file in -- args --"},
		{"screen before -F opens", "T\n-- args --\n-F\n-- input --\nhi\n-- screen --\nhi\n-- eof --\n", "line 6, -- screen --: -F is waiting for EOF; the pager has not opened"},
		{"keys before -F opens", "T\n-- args --\n-F\n-- input --\nhi\n-- keys --\nDown\n-- eof --\n", "line 6, -- keys --: -F is waiting for EOF; the pager has not opened"},
		{"screen after -F prints", "T\n-- args --\n-F\n-- input --\nhi\n-- quit --\n-- screen --\nhi\n", "line 7, -- screen --: -F printed the text; the pager never opened"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			err := runScenario(t, parseArchive(c.text), nil)
			if err == nil {
				t.Fatal("scenario passed, want failure")
			}
			if !strings.Contains(err.Error(), c.want) {
				t.Errorf("error %q\nwant it to contain %q", err, c.want)
			}
		})
	}
}
