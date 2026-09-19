// Command vedi is a pager: view ANSI-colored text, select, copy.
package main

import (
	"fmt"
	"io"
	"os"

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

	buf := buffer.New()
	a := app.New(scr, buf, opts.App(scr))
	go buffer.Fill(in, buf, a.Notify)
	a.Run()
}
