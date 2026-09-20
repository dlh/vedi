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

// openInput concatenates the files, or reads stdin when there are none;
// "-" names stdin.
func openInput(files []string) (io.Reader, func(), error) {
	if len(files) == 0 {
		if fi, err := os.Stdin.Stat(); err == nil && fi.Mode()&os.ModeCharDevice != 0 {
			return nil, nil, fmt.Errorf("no input: give a file or pipe something in")
		}
		return os.Stdin, func() {}, nil
	}
	var readers []io.Reader
	var closers []io.Closer
	for _, name := range files {
		if name == "-" {
			readers = append(readers, os.Stdin)
			continue
		}
		f, err := os.Open(name)
		if err != nil {
			for _, c := range closers {
				c.Close()
			}
			return nil, nil, err
		}
		readers = append(readers, f)
		closers = append(closers, f)
	}
	return io.MultiReader(readers...), func() {
		for _, c := range closers {
			c.Close()
		}
	}, nil
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
	in, closeInput, err := openInput(files)
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
	appOpts := opts.App(scr, files)
	var rec *buffer.Recorder
	if opts.QuitIfOnePage {
		rec = buffer.Record(in)
		in, appOpts.Paging = rec, rec.Stop
	}
	a := app.New(scr, buf, appOpts)
	go buffer.Fill(in, buf, a.Notify)
	a.Run()
	if a.PrintText() {
		scr.Fini()
		os.Stdout.Write(rec.Bytes())
	}
}
