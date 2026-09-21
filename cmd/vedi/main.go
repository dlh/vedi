// Command vedi is a pager: view ANSI-colored text, select, copy.
package main

import (
	"fmt"
	"io"
	"os"
	"runtime/debug"

	"github.com/gdamore/tcell/v2"
	"go.dlh.dev/vedi/internal/app"
	"go.dlh.dev/vedi/internal/buffer"
	"go.dlh.dev/vedi/internal/cli"
)

// openInput is the files, or stdin when there are none; "-" names
// stdin. src is the same bytes at random, or nil unless every file is
// a regular one: a pipe cannot be read twice.
func openInput(files []string) (in io.Reader, src io.ReaderAt, closeInput func(), err error) {
	if len(files) == 0 {
		if fi, err := os.Stdin.Stat(); err == nil && fi.Mode()&os.ModeCharDevice != 0 {
			return nil, nil, nil, fmt.Errorf("no input: give a file or pipe something in")
		}
		return os.Stdin, nil, func() {}, nil
	}
	var parts []buffer.Source
	var closers []io.Closer
	regular := true
	for _, name := range files {
		if name == "-" {
			parts = append(parts, os.Stdin)
			regular = false
			continue
		}
		f, err := os.Open(name)
		if err != nil {
			for _, c := range closers {
				c.Close()
			}
			return nil, nil, nil, err
		}
		if fi, err := f.Stat(); err != nil || !fi.Mode().IsRegular() {
			regular = false
		}
		parts = append(parts, f)
		closers = append(closers, f)
	}
	c := buffer.NewConcat(parts...)
	closeInput = func() {
		for _, c := range closers {
			c.Close()
		}
	}
	if !regular {
		return c, nil, closeInput, nil
	}
	return c, c, closeInput, nil
}

// version is set by the linker for releases; otherwise the module
// version, which go install fills in.
var version string

func main() {
	opts, files, err := cli.Parse(os.Args[1:])
	if err != nil {
		fmt.Fprintf(os.Stderr, "vedi: %v\n%s", err, cli.Usage)
		os.Exit(1)
	}
	if opts.Help {
		fmt.Print(cli.Usage)
		return
	}
	if opts.Version {
		if version == "" {
			if bi, ok := debug.ReadBuildInfo(); ok {
				version = bi.Main.Version
			}
		}
		fmt.Println("vedi", version)
		return
	}
	in, src, closeInput, err := openInput(files)
	if err != nil {
		fmt.Fprintf(os.Stderr, "vedi: %v\n", err)
		if len(files) == 0 {
			fmt.Fprint(os.Stderr, cli.Usage)
		}
		os.Exit(1)
	}
	defer closeInput()

	scr, err := tcell.NewScreen()
	if err == nil {
		err = scr.Init()
	}
	if err != nil {
		fmt.Fprintf(os.Stderr, "vedi: %v\n", err)
		os.Exit(1)
	}
	defer scr.Fini()
	scr.EnableMouse(tcell.MouseDragEvents)

	buf := buffer.New()
	if src != nil {
		buf = buffer.NewFrom(src)
	}
	a := app.New(scr, buf, opts.App(scr, files))
	go buffer.Fill(in, buf, a.Notify)
	a.Run()
	if a.PrintText() {
		scr.Fini()
		buf.WriteTo(os.Stdout)
	}
}
