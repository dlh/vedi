package main

import (
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"text/tabwriter"
	"time"
)

func main() {
	vedi := flag.String("vedi", "./vedi", "the vedi binary")
	lines := flag.Int("lines", 1_000_000, "lines of input")
	runs := flag.Int("runs", 5, "runs per pager; the median is reported")
	timeout := flag.Duration("timeout", time.Minute, "give up on a screen after this long")
	flag.Parse()

	dir, err := os.MkdirTemp("", "vedi-bench")
	if err != nil {
		fatal(err)
	}
	defer os.RemoveAll(dir)
	file := filepath.Join(dir, "input.txt")
	if err := writeInput(file, *lines); err != nil {
		fatal(err)
	}
	results := map[string][]sample{}
	for i := 1; i <= *runs; i++ {
		for _, p := range pagers {
			fmt.Fprintf(os.Stderr, "%s run %d/%d\n", p.name, i, *runs)
			results[p.name] = append(results[p.name], run(p, *vedi, file, *lines, *timeout))
		}
	}
	report(os.Stdout, results)
}

func fatal(err error) {
	fmt.Fprintln(os.Stderr, "bench:", err)
	os.Exit(1)
}

// report prints a row per measurement and a column per pager: the
// median over runs, or the first error.
func report(w io.Writer, results map[string][]sample) {
	tw := tabwriter.NewWriter(w, 0, 0, 2, ' ', 0)
	for _, p := range pagers {
		fmt.Fprintf(tw, "\t%s", p.name)
	}
	fmt.Fprintln(tw)
	for _, row := range rows {
		fmt.Fprint(tw, row)
		for _, p := range pagers {
			fmt.Fprintf(tw, "\t%s", summarize(results[p.name], row))
		}
		fmt.Fprintln(tw)
	}
	tw.Flush()
}

func summarize(samples []sample, row string) string {
	var vs []float64
	for _, s := range samples {
		c, ok := s[row]
		if !ok {
			continue
		}
		if c.err != nil {
			return c.err.Error()
		}
		vs = append(vs, c.v)
	}
	if len(vs) == 0 {
		return "-"
	}
	sort.Float64s(vs)
	return fmt.Sprintf("%.1f", vs[len(vs)/2])
}
