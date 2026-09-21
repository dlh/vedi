package main

import (
	"bytes"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// TestRun: every row is measured on vedi built from this tree, and on
// less when it is installed.
func TestRun(t *testing.T) {
	dir := t.TempDir()
	build := exec.Command("go", "build", "-C", "..", "-o", filepath.Join(dir, "vedi"), "./cmd/vedi")
	if out, err := build.CombinedOutput(); err != nil {
		t.Fatalf("build: %v\n%s", err, out)
	}
	// A bare -vedi found on PATH, from a directory that is neither
	// the binary's nor the input's.
	t.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH"))
	t.Chdir(t.TempDir())
	bin := "vedi"
	const n = 200
	file := filepath.Join(t.TempDir(), "in")
	if err := writeInput(file, n); err != nil {
		t.Fatal(err)
	}
	for _, p := range pagers {
		if _, err := exec.LookPath(p.command(bin, "in")[0]); err != nil {
			t.Logf("skipping %s: %v", p.name, err)
			continue
		}
		got := run(p, bin, file, n, 20*time.Second)
		for _, row := range rows {
			c, ok := got[row]
			if !ok || c.err != nil || c.v <= 0 {
				t.Errorf("%s %s = %.1f, %v", p.name, row, c.v, c.err)
			}
		}
	}
}

func TestReport(t *testing.T) {
	results := map[string][]sample{
		"vedi": {{"end ms": {v: 3}}, {"end ms": {v: 1}}, {"end ms": {v: 2}}},
		"less": {{"end ms": {err: errTimeout}}},
	}
	var out bytes.Buffer
	report(&out, results)
	for _, want := range []string{"vedi", "less", "end ms", "2.0", "timeout", "rss stdin MB"} {
		if !strings.Contains(out.String(), want) {
			t.Errorf("report lacks %q:\n%s", want, out.String())
		}
	}
}

// TestEndPoll: re-pressing G adds at most endPoll to "end ms", so it
// must be small next to the hundreds of ms a pager takes to read the
// file.
func TestEndPoll(t *testing.T) {
	if endPoll > 10*time.Millisecond {
		t.Errorf("endPoll = %v, want at most 10ms", endPoll)
	}
}

// TestToEndReady: end waits for the pager's at-end text, not just the
// last line, since a pager can show the line and go on working.
func TestToEndReady(t *testing.T) {
	file := filepath.Join(t.TempDir(), "in")
	if err := writeInput(file, 3); err != nil {
		t.Fatal(err)
	}
	s, err := start("", []string{"sh", "-c", `cat "$1"; sleep 0.1; echo READY`, "sh", file}, nil)
	if err != nil {
		t.Fatal(err)
	}
	t0 := time.Now()
	if err := toEnd(s, 3, "READY", 5*time.Second); err != nil {
		s.kill()
		t.Fatal(err)
	}
	if d := time.Since(t0); d < 100*time.Millisecond {
		t.Errorf("toEnd returned after %v, before READY", d)
	}
	s.kill()
}
