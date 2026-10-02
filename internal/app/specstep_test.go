package app_test

import (
	"bufio"
	"fmt"
	"os"
	"strings"

	"go.dlh.dev/vedi/internal/app"
	"go.dlh.dev/vedi/internal/testscreen"
	"golang.org/x/term"
)

// stepper prints a scenario as it runs, for bin/spec-step: each
// section, then the screen it left if that changed, in a frame the
// screen's size. It waits for Enter before each action and clears the
// terminal for it, so the frame changes in place; with stdin not a
// terminal it prints straight through, a rule between the steps.
type stepper struct {
	a     archive
	i     int // the section running
	in    *bufio.Reader
	shown string // the last frame printed
	gone  bool   // and the terminal was cleared since
	done  bool   // stdin is not a terminal or has ended, or q was typed
}

func newStepper(a archive) *stepper {
	p := &stepper{a: a, in: bufio.NewReader(os.Stdin)}
	// A pipe left open would never answer.
	p.done = !term.IsTerminal(int(os.Stdin.Fd()))
	p.page()
	return p
}

// page starts a step with the prose, on a cleared terminal when the
// steps wait.
func (p *stepper) page() {
	if !p.done {
		fmt.Print("\x1b[H\x1b[2J")
		p.gone = true
	}
	fmt.Printf("\n%s\n", strings.TrimSpace(p.a.comment))
}

// drives are the actions that need the pager open first; the others,
// input and eof, may be what opens it.
var drives = map[string]bool{"keys": true, "mouse": true, "resize": true, "file": true, "reload": true}

// opened shows the first screen and waits, when the section that
// opened the pager goes on to change it.
func (p *stepper) opened(s *scenario) {
	if drives[p.a.sections[p.i].name] {
		fmt.Println()
		p.frame(s)
		p.wait()
	}
}

// after prints the section that just ran and what it did, and waits
// when an action comes next.
func (p *stepper) after(s *scenario, err error) {
	sec := p.a.sections[p.i]
	p.i++
	fmt.Printf("\n-- %s --\n", sec.name)
	if sec.name != "screen" {
		fmt.Print(clip(sec.body, 10))
	}
	changed := s.scr != nil && p.frame(s)
	switch {
	case err != nil:
	case !actions[sec.name] && s.started:
		fmt.Println("ok")
	case !changed && s.scr != nil:
		fmt.Println("(screen unchanged)")
	}
	if s.quit && actions[sec.name] {
		fmt.Println("(the app quit)")
	}
	if err == nil && s.started && p.i < len(p.a.sections) && actions[p.a.sections[p.i].name] {
		p.wait()
	}
}

// frame prints the screen and the cursor's place, unless they are as
// last printed and still on the terminal, and reports whether they
// changed.
func (p *stepper) frame(s *scenario) bool {
	f := frame(s.scr)
	changed := f != p.shown
	if changed || p.gone {
		fmt.Print(f)
	}
	p.shown, p.gone = f, false
	return changed
}

// wait ends a step. It reads a line: Enter clears the terminal for the
// next step, under the prose again; q stops waiting. Once nothing
// waits, a rule ends the step.
func (p *stepper) wait() {
	if !p.done {
		fmt.Print("\n[Enter] next, [q] run to the end ")
		line, err := p.in.ReadString('\n')
		if err != nil {
			fmt.Println()
		}
		p.done = err != nil || strings.TrimSpace(line) == "q"
		if !p.done {
			p.page()
			return
		}
	}
	fmt.Println("\n" + strings.Repeat("━", 42))
}

// frame draws the screen in a box, with reverse video and search
// matches as a terminal shows them and the cursor's cell underlined.
// Other colors are left out.
func frame(scr *testscreen.Screen) string {
	w, h := scr.Size()
	cx, cy, vis := scr.Cursor()
	var sb strings.Builder
	sb.WriteString("┌" + strings.Repeat("─", w) + "┐\n")
	for y := range h {
		sb.WriteString("│")
		for x := 0; x < w; {
			str, st, cw := scr.Get(x, y)
			var sgr []string
			if st.HasReverse() {
				sgr = append(sgr, "7")
			}
			if st == app.MatchStyle {
				sgr = append(sgr, "30", "103")
			}
			if vis && x == cx && y == cy {
				sgr = append(sgr, "4")
			}
			if sgr != nil {
				str = "\x1b[" + strings.Join(sgr, ";") + "m" + str + "\x1b[m"
			}
			sb.WriteString(str)
			x += cw
		}
		sb.WriteString("│\n")
	}
	sb.WriteString("└" + strings.Repeat("─", w) + "┘\n")
	if vis {
		fmt.Fprintf(&sb, "cursor %d %d\n", cy, cx)
	} else {
		sb.WriteString("cursor hidden\n")
	}
	return sb.String()
}

// clip is body cut to n lines, saying how many it left out.
func clip(body string, n int) string {
	ls := strings.SplitAfter(body, "\n")
	if len(ls) <= n+1 {
		return body
	}
	return strings.Join(ls[:n], "") + fmt.Sprintf("… %d more lines\n", strings.Count(body, "\n")-n)
}
