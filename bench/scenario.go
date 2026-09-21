package main

import (
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"time"
)

// pager names a pager and how to run it on a file, or on stdin when
// there is none.
type pager struct {
	name     string
	argv     []string // {vedi} is the vedi binary, {file} the input
	notFound string   // what a failed search prints
	atEnd    string   // what shows once it is ready at the last of {n} lines
}

var pagers = []pager{
	{"vedi", []string{"{vedi}", "{file}"}, "not found:", "{n}/{n}"},
	{"less", []string{"less", "-R", "{file}"}, "Pattern not found", "(END)"},
}

// end is the at-end text for n lines.
func (p pager) end(n int) string { return strings.ReplaceAll(p.atEnd, "{n}", strconv.Itoa(n)) }

// command is the argv for file, or for stdin when file is "".
func (p pager) command(vedi, file string) []string {
	var argv []string
	for _, a := range p.argv {
		switch a {
		case "{vedi}":
			argv = append(argv, vedi)
		case "{file}":
			if file != "" {
				argv = append(argv, file)
			}
		default:
			argv = append(argv, a)
		}
	}
	return argv
}

// rows are the report's rows, in order.
var rows = []string{"first-screen ms", "end ms", "home ms", "search-miss ms", "stdin ms", "rss file MB", "rss stdin MB"}

// firstScreen is the line whose marker means the first screen is up:
// the last of 23 text rows, above vedi's status line or less's prompt.
const firstScreen = 23

// cell is one measurement, or why there is none.
type cell struct {
	v   float64
	err error
}

// sample is one run's cells by row. Rows after a failure are absent.
type sample map[string]cell

// run measures p once on file, which has n lines: a session on the
// file for the first four rows and "rss file", and one on stdin for
// the rest. Pagers run in the file's directory and see its base name,
// so a long path does not push their status off the screen.
func run(p pager, vedi, file string, n int, timeout time.Duration) sample {
	out := sample{}
	// A bare name comes from PATH; a relative path would resolve in
	// the input's directory.
	if found, err := exec.LookPath(vedi); err == nil {
		if abs, err := filepath.Abs(found); err == nil {
			vedi = abs
		}
	}
	runFile(p, vedi, file, n, timeout, out)
	runStdin(p, vedi, file, n, timeout, out)
	return out
}

func ms(since time.Time) float64 { return float64(time.Since(since)) / float64(time.Millisecond) }

func mb(bytes int64) float64 { return float64(bytes) / (1 << 20) }

func runFile(p pager, vedi, file string, n int, timeout time.Duration, out sample) {
	t0 := time.Now()
	s, err := start(filepath.Dir(file), p.command(vedi, filepath.Base(file)), nil)
	if err != nil {
		out["first-screen ms"] = cell{err: err}
		return
	}
	if err := s.wait(marker(firstScreen), 0, timeout); err != nil {
		out["first-screen ms"] = cell{err: err}
		s.kill()
		return
	}
	out["first-screen ms"] = cell{v: ms(t0)}

	t := time.Now()
	if err := toEnd(s, n, p.end(n), timeout); err != nil {
		out["end ms"] = cell{err: err}
		s.kill()
		return
	}
	out["end ms"] = cell{v: ms(t)}

	from, t := s.mark(), time.Now()
	s.send("g")
	if err := s.wait(marker(1), from, timeout); err != nil {
		out["home ms"] = cell{err: err}
		s.kill()
		return
	}
	out["home ms"] = cell{v: ms(t)}

	// The whole file is read now, so the search covers all of it.
	from, t = s.mark(), time.Now()
	s.send("/zzzz\r")
	if err := s.wait(p.notFound, from, timeout); err != nil {
		out["search-miss ms"] = cell{err: err}
		s.kill()
		return
	}
	out["search-miss ms"] = cell{v: ms(t)}

	s.send("q")
	rss, err := s.finish(timeout)
	out["rss file MB"] = cell{v: mb(rss), err: err}
}

// endPoll is how often toEnd presses G again. It bounds how late the
// last line is seen after the file is read, so it must be small next
// to the time being measured.
const endPoll = 5 * time.Millisecond

// toEnd presses G until the last of n lines shows, then waits for
// ready, the pager's at-end text: a pager can show the line and go on
// working before it takes the next key. G jumps to the end of what a
// pager has read so far, so it is pressed again until the file is
// read.
func toEnd(s *session, n int, ready string, timeout time.Duration) error {
	from := s.mark()
	deadline := time.Now().Add(timeout)
	for {
		if err := s.send("G"); err != nil {
			return err
		}
		err := s.wait(marker(n), from, endPoll)
		if err == nil {
			break
		}
		if err != errTimeout || !time.Now().Before(deadline) {
			return err
		}
	}
	return s.wait(ready, from, time.Until(deadline))
}

// runStdin pipes file into the pager and times the first screen; then
// it reads to the end, so the RSS is for the whole input.
func runStdin(p pager, vedi, file string, n int, timeout time.Duration, out sample) {
	f, err := os.Open(file)
	if err != nil {
		out["stdin ms"] = cell{err: err}
		return
	}
	defer f.Close()
	r, w, err := os.Pipe()
	if err != nil {
		out["stdin ms"] = cell{err: err}
		return
	}
	go func() {
		io.Copy(w, f)
		w.Close()
	}()
	t0 := time.Now()
	s, err := start(filepath.Dir(file), p.command(vedi, ""), r)
	r.Close()
	if err != nil {
		out["stdin ms"] = cell{err: err}
		return
	}
	if err := s.wait(marker(firstScreen), 0, timeout); err != nil {
		out["stdin ms"] = cell{err: err}
		s.kill()
		return
	}
	out["stdin ms"] = cell{v: ms(t0)}
	if err := toEnd(s, n, p.end(n), timeout); err != nil {
		out["rss stdin MB"] = cell{err: err}
		s.kill()
		return
	}
	s.send("q")
	rss, err := s.finish(timeout)
	out["rss stdin MB"] = cell{v: mb(rss), err: err}
}
