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
	argv     []string // {vedi} is the vedi binary, {file} the input, {plugins} the plugins
	stdin    string   // what stands in for {file} to read stdin; "" drops it
	drawn    string   // what follows a marker once its line is drawn
	notFound string   // what a failed search prints
	atEnd    string   // what shows once it is ready at the last of {n} lines
	end      string   // the key for the last line
	home     string   // the key for the first line
	quit     string   // the keys that exit
}

// plugins is a directory of plugin checkouts, which vim needs to show
// the colors; empty leaves it out.
var plugins string

// moor searches as each character is typed, so its search-miss time
// covers four searches, one per character. It prints nothing on a
// miss; the status line's hint changes after any search. ov draws a
// tilde row past the last line.
var pagers = []pager{
	{"vedi", []string{"{vedi}", "{file}"}, "", "", "not found:", "{n}/{n}", "G", "g", "q"},
	{"less", []string{"less", "-R", "{file}"}, "", "", "Pattern not found", "(END)", "G", "g", "q"},
	{"moor", []string{"moor", "{file}"}, "", "", "n/p to search", "100%", "G", "<", "q"},
	{"ov", []string{"ov", "{file}"}, "", "", "not found:", "~", "\x1b[F", "\x1b[H", "q"},
	{"vim", []string{"vim", "-N", "-u", "NONE", "-i", "NONE", "-n", "-R",
		"--cmd", "set rtp^={plugins}/vim-plugin-AnsiEsc ruler",
		"-c", "runtime! plugin/cecutil.vim plugin/AnsiEscPlugin.vim", "-c", "AnsiEsc", "{file}"},
		"-", filler[:10], "Pattern not found", "{n},1", "G", "gg", ":q!\r"},
}

// needsPlugins is whether p runs a plugin.
func (p pager) needsPlugins() bool {
	for _, a := range p.argv {
		if strings.Contains(a, "{plugins}") {
			return true
		}
	}
	return false
}

// ready is the at-end text for n lines.
func (p pager) ready(n int) string { return strings.ReplaceAll(p.atEnd, "{n}", strconv.Itoa(n)) }

// command is the argv for file, or for stdin when file is "". The
// plugins directory is made absolute, since vim starts in the input's
// directory.
func (p pager) command(vedi, file string) []string {
	dir := plugins
	if dir != "" {
		if abs, err := filepath.Abs(dir); err == nil {
			dir = abs
		}
	}
	var argv []string
	for _, a := range p.argv {
		switch a {
		case "{vedi}":
			argv = append(argv, vedi)
		case "{file}":
			if file != "" {
				argv = append(argv, file)
			} else if p.stdin != "" {
				argv = append(argv, p.stdin)
			}
		default:
			argv = append(argv, strings.ReplaceAll(a, "{plugins}", dir))
		}
	}
	return argv
}

// rows are the report's rows, in order.
var rows = []string{"first-screen ms", "end ms", "home ms", "search-miss ms", "stdin ms", "search-miss stdin ms", "rss file MB", "rss stdin MB"}

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
	// A pager that took the machine's memory leaves the file out of
	// the page cache, so the next one would read it from disk.
	if err := warm(file); err != nil {
		out["first-screen ms"] = cell{err: err}
		return out
	}
	runFile(p, vedi, file, n, timeout, out)
	runStdin(p, vedi, file, n, timeout, out)
	return out
}

// warm reads file through, so it is in the page cache.
func warm(file string) error {
	f, err := os.Open(file)
	if err != nil {
		return err
	}
	defer f.Close()
	_, err = io.Copy(io.Discard, f)
	return err
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
	if err := s.wait(marker(firstScreen)+p.drawn, timeout); err != nil {
		out["first-screen ms"] = cell{err: err}
		s.kill()
		return
	}
	out["first-screen ms"] = cell{v: ms(t0)}

	t := time.Now()
	if err := toEnd(s, p, n, timeout); err != nil {
		out["end ms"] = cell{err: err}
		s.kill()
		return
	}
	out["end ms"] = cell{v: ms(t)}

	out["home ms"] = home(s, p, timeout)
	if out["home ms"].err != nil {
		s.kill()
		return
	}

	// The whole file is read now, so the search covers all of it.
	out["search-miss ms"] = searchMiss(s, p, timeout)
	if out["search-miss ms"].err != nil {
		s.kill()
		return
	}

	s.send(p.quit)
	rss, err := s.finish(timeout)
	out["rss file MB"] = cell{v: mb(rss), err: err}
}

// home times the home key, from the end to the first line.
func home(s *session, p pager, timeout time.Duration) cell {
	t := time.Now()
	s.send(p.home)
	if err := s.wait(marker(1), timeout); err != nil {
		return cell{err: err}
	}
	return cell{v: ms(t)}
}

// searchMiss times a search for text that is not there.
func searchMiss(s *session, p pager, timeout time.Duration) cell {
	t := time.Now()
	s.send("/zzzz\r")
	if err := s.wait(p.notFound, timeout); err != nil {
		return cell{err: err}
	}
	return cell{v: ms(t)}
}

// endPoll is how often toEnd presses the end key again. It bounds how
// late the last line is seen after the file is read, so it must be
// small next to the time being measured.
const endPoll = 5 * time.Millisecond

// toEnd presses the end key until the last of n lines shows, then
// waits for the pager's at-end text: a pager can show the line and go
// on working before it takes the next key. The end key jumps to the
// end of what a pager has read so far, so it is pressed again until
// the file is read.
func toEnd(s *session, p pager, n int, timeout time.Duration) error {
	deadline := time.Now().Add(timeout)
	for {
		if err := s.send(p.end); err != nil {
			return err
		}
		err := s.wait(marker(n), endPoll)
		if err == nil {
			break
		}
		if err != errTimeout || !time.Now().Before(deadline) {
			return err
		}
	}
	return s.wait(p.ready(n), time.Until(deadline))
}

// runStdin pipes file into the pager and times the first screen; then
// it reads to the end and searches all of it, from the top, so the
// search and the RSS are for the whole input.
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
	if err := s.wait(marker(firstScreen)+p.drawn, timeout); err != nil {
		out["stdin ms"] = cell{err: err}
		s.kill()
		return
	}
	out["stdin ms"] = cell{v: ms(t0)}
	// The end and home are not timed again; a failure there lands on
	// the search's row.
	c := cell{err: toEnd(s, p, n, timeout)}
	if c.err == nil {
		c = home(s, p, timeout)
	}
	if c.err == nil {
		c = searchMiss(s, p, timeout)
	}
	out["search-miss stdin ms"] = c
	if c.err != nil {
		s.kill()
		return
	}
	s.send(p.quit)
	rss, err := s.finish(timeout)
	out["rss stdin MB"] = cell{v: mb(rss), err: err}
}
