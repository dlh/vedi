package buffer

import (
	"bufio"
	"io"

	"go.dlh.dev/vedi/internal/ansi"
)

// Fill splits r on "\n" (dropping a "\r" before it), parses each line
// into buf, then calls buf.Finish with the read error. notify runs
// after every line and after Finish; callers should coalesce.
func Fill(r io.Reader, buf *Buffer, notify func()) {
	br := bufio.NewReaderSize(r, 64*1024)
	p := ansi.NewParser()
	var err error
	for {
		var raw []byte
		raw, err = br.ReadBytes('\n')
		if len(raw) > 0 {
			if raw[len(raw)-1] == '\n' {
				raw = raw[:len(raw)-1]
				if len(raw) > 0 && raw[len(raw)-1] == '\r' {
					raw = raw[:len(raw)-1]
				}
			}
			text, runs := p.Parse(raw)
			buf.Append(Line{Text: text, Runs: runs})
			notify()
		}
		if err != nil {
			break
		}
	}
	if err == io.EOF {
		err = nil
	}
	buf.Finish(err)
	notify()
}
