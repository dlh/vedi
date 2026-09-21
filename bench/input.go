// Command bench compares vedi with less on a large file: time to the
// first screen, to the end and back, for a failed search, to the first
// screen from stdin, and peak memory.
package main

import (
	"bufio"
	"fmt"
	"os"
)

// marker begins line i (1-based) and is what the harness waits for.
// The colon keeps "00000001:" from matching inside a longer number,
// and is never skipped as a blank cell the way a space is.
func marker(i int) string { return fmt.Sprintf("%08d:", i) }

// writeInput writes n lines to path: a marker, then colored filler, 54
// columns in all so nothing wraps at 80.
func writeInput(path string, n int) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	w := bufio.NewWriterSize(f, 1<<20)
	for i := 1; i <= n; i++ {
		fmt.Fprintf(w, "%s\x1b[32msome plain text about\x1b[0m sixty \x1b[1mbold\x1b[0m columns wide\n", marker(i))
	}
	if err := w.Flush(); err != nil {
		f.Close()
		return err
	}
	return f.Close()
}
