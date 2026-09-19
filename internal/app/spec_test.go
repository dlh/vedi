package app_test

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/gdamore/tcell/v2"
	"go.dlh.dev/vedi/internal/ansi"
	"go.dlh.dev/vedi/internal/app"
	"go.dlh.dev/vedi/internal/buffer"
	"go.dlh.dev/vedi/internal/cli"
)

// TestSpecs runs every scenario file under specs/. The format is
// described in specs/README.md.
func TestSpecs(t *testing.T) {
	files, err := filepath.Glob("../../specs/*/*.txt")
	if err != nil || len(files) == 0 {
		t.Fatalf("no scenario files found: %v", err)
	}
	for _, path := range files {
		name, _ := filepath.Rel("../../specs", path)
		t.Run(name, func(t *testing.T) {
			data, err := os.ReadFile(path)
			if err != nil {
				t.Fatal(err)
			}
			if err := runScenario(t, parseArchive(string(data))); err != nil {
				t.Fatal(err)
			}
		})
	}
}

type scenario struct {
	t        *testing.T
	scr      tcell.SimulationScreen
	buf      *buffer.Buffer
	parser   *ansi.Parser
	app      *app.App
	w, h     int
	args     []string
	hasEOF   bool // the file has an eof section, so input stays open
	finished bool // an eof section has run
	started  bool
	quit     bool
}

// runScenario runs one scenario and returns the first failure, a
// malformed section or a failed assertion, prefixed with its line. t
// only cleans up the screen.
func runScenario(t *testing.T, a archive) error {
	if strings.TrimSpace(a.comment) == "" {
		return fmt.Errorf("a scenario starts with prose describing the behavior")
	}
	s := &scenario{t: t, buf: buffer.New(), parser: ansi.NewParser(), w: 40, h: 6}
	for _, sec := range a.sections {
		if sec.name == "eof" {
			s.hasEOF = true
		}
	}
	inputSeen := false
	for i, sec := range a.sections {
		fail := func(format string, args ...any) error {
			return fmt.Errorf("line %d, -- %s --: %s", sec.line, sec.name, fmt.Sprintf(format, args...))
		}
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
			s.notify()
		case "eof":
			if s.finished {
				return fail("input already finished")
			}
			s.finished = true
			if err := s.start(); err != nil {
				return err
			}
			s.buf.Finish(nil)
			s.notify()
		case "keys":
			if s.quit {
				return fail("keys after the app quit")
			}
			if err := s.start(); err != nil {
				return err
			}
			keys, err := parseKeys(sec.body)
			if err != nil {
				return fail("%v", err)
			}
			for _, k := range keys {
				s.quit = s.app.Handle(k)
				s.app.Draw()
			}
			if s.quit && (i+1 >= len(a.sections) || a.sections[i+1].name != "quit") {
				return fail("a key quit; the next section must be -- quit --")
			}
		case "resize":
			if err := s.start(); err != nil {
				return err
			}
			var w, h int
			if _, err := fmt.Sscanf(strings.TrimSpace(sec.body), "%dx%d", &w, &h); err != nil {
				return fail("want WxH, got %q", sec.body)
			}
			s.scr.SetSize(w, h)
			s.app.Handle(tcell.NewEventResize(w, h))
			s.app.Draw()
		case "screen":
			if err := s.start(); err != nil {
				return err
			}
			if got := dump(s.scr); got != sec.body {
				return fail("screen differs\n%s", sideBySide(sec.body, got))
			}
		case "cursor":
			if err := s.start(); err != nil {
				return err
			}
			want := strings.TrimSpace(sec.body)
			x, y, vis := s.scr.GetCursor()
			got := "hidden"
			if vis {
				got = fmt.Sprintf("%d %d", y, x)
			}
			if got != want {
				return fail("cursor = %s, want %s", got, want)
			}
		case "clipboard":
			if err := s.start(); err != nil {
				return err
			}
			want := strings.TrimSuffix(sec.body, "\n")
			if got := string(s.scr.GetClipboardData()); got != want {
				return fail("clipboard = %q, want %q", got, want)
			}
		case "quit":
			if !s.quit {
				return fail("the last key did not quit")
			}
		default:
			return fail("unknown section")
		}
	}
	return nil
}

// input appends the body's lines to the buffer through one ANSI parser
// shared across sections. txtar's trailing newline is stripped; a
// second one means the input itself ended with a newline.
func (s *scenario) input(body string) {
	if body == "" {
		return
	}
	text := strings.TrimSuffix(strings.TrimSuffix(body, "\n"), "\n")
	for _, line := range strings.Split(text, "\n") {
		line = strings.TrimSuffix(line, "\r") // as buffer.Fill drops a "\r" before "\n"
		t, runs := s.parser.Parse([]byte(line))
		s.buf.Append(buffer.Line{Text: t, Runs: runs})
	}
}

// start builds the screen and app the first time an action or
// assertion needs them. Without an eof section the input is complete.
func (s *scenario) start() error {
	if s.started {
		return nil
	}
	s.started = true
	if !s.hasEOF {
		s.buf.Finish(nil)
	}
	opts, files, err := cli.Parse(s.args)
	if err != nil || len(files) > 0 {
		return fmt.Errorf("args %q: %v (files %q)", s.args, err, files)
	}
	s.scr = tcell.NewSimulationScreen("UTF-8")
	if err := s.scr.Init(); err != nil {
		return err
	}
	s.t.Cleanup(s.scr.Fini)
	s.scr.SetSize(s.w, s.h)
	s.app = app.New(s.scr, s.buf, opts.App(s.scr))
	s.notify()
	return nil
}

// notify delivers the reader's data event, as buffer.Fill would.
func (s *scenario) notify() {
	s.app.Handle(tcell.NewEventInterrupt(nil))
	s.app.Draw()
}

// dump renders the screen as the -- screen -- section expects it:
// reverse-video runs in brackets, the status row plain, trailing
// spaces trimmed, one line per row.
func dump(scr tcell.SimulationScreen) string {
	cells, w, h := scr.GetContents()
	var sb strings.Builder
	for y := 0; y < h; y++ {
		var row strings.Builder
		status := h >= 2 && y == h-1
		rev := false
		for x := 0; x < w; x++ {
			c := cells[y*w+x]
			if len(c.Runes) == 0 {
				continue // the second cell of a wide rune
			}
			if !status {
				_, _, attr := c.Style.Decompose()
				r := attr&tcell.AttrReverse != 0
				if r && !rev {
					row.WriteByte('[')
				} else if !r && rev {
					row.WriteByte(']')
				}
				rev = r
			}
			row.WriteString(string(c.Runes))
		}
		if rev {
			row.WriteByte(']')
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
	const ok = "Text.\n-- input --\nhi\n-- screen --\nhi\n\n\n\n\nvedi  line 1/1  wrap\n"
	if err := runScenario(t, parseArchive(ok)); err != nil {
		t.Fatalf("control scenario failed: %v", err)
	}
	cases := []struct{ name, text, want string }{
		{"wrong screen", "T\n-- input --\nhi\n-- screen --\nho\n", "line 4, -- screen --: screen differs"},
		{"wrong cursor", "T\n-- input --\nhi\n-- cursor --\n0 1\n", "line 4, -- cursor --: cursor = 0 0, want 0 1"},
		{"wrong clipboard", "T\n-- input --\nhi\n-- keys --\nCtrl+A Enter\n-- quit --\n-- clipboard --\nho\n", `line 7, -- clipboard --: clipboard = "hi", want "ho"`},
		{"quit without section", "T\n-- input --\nhi\n-- keys --\nq\n", "line 4, -- keys --: a key quit; the next section must be -- quit --"},
		{"quit section without quit", "T\n-- input --\nhi\n-- keys --\nDown\n-- quit --\n", "line 6, -- quit --: the last key did not quit"},
		{"keys after quit", "T\n-- input --\nhi\n-- keys --\nq\n-- quit --\n-- keys --\nDown\n", "line 7, -- keys --: keys after the app quit"},
		{"unknown section", "T\n-- input --\nhi\n-- screeen --\nho\n", "line 4, -- screeen --: unknown section"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			err := runScenario(t, parseArchive(c.text))
			if err == nil {
				t.Fatal("scenario passed, want failure")
			}
			if !strings.Contains(err.Error(), c.want) {
				t.Errorf("error %q\nwant it to contain %q", err, c.want)
			}
		})
	}
}
