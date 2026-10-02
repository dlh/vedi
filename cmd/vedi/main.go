// Command vedi is a pager: view ANSI-colored text, select, copy.
package main

import (
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"os/signal"
	"runtime/debug"
	"strconv"
	"sync/atomic"
	"syscall"
	"time"

	"github.com/gdamore/tcell/v3"
	"go.dlh.dev/vedi/internal/app"
	"go.dlh.dev/vedi/internal/buffer"
	"go.dlh.dev/vedi/internal/cli"
	"go.dlh.dev/vedi/internal/config"
	"go.dlh.dev/vedi/internal/watch"
	"golang.org/x/term"
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

// reopen is the app's Open: the files read again by name, the input
// paged from disk when it still can be.
func reopen(files []string) func(func()) (*buffer.Buffer, func(), error) {
	return func(notify func()) (*buffer.Buffer, func(), error) {
		in, src, closeInput, err := openInput(files)
		if err != nil {
			return nil, nil, err
		}
		buf := buffer.New()
		if src != nil {
			buf = buffer.NewFrom(src)
		}
		go buffer.Fill(in, buf, notify)
		return buf, closeInput, nil
	}
}

// postChanged posts the watcher's change to the loop, trying again
// shortly when the queue is full so none is lost.
func postChanged(a *app.App) func() {
	ev := &app.Changed{}
	var post func()
	post = func() {
		if !a.Post(ev) {
			time.AfterFunc(50*time.Millisecond, post)
		}
	}
	return post
}

// notifier is the reader's notify, which changes hands: -F listens
// first, then the app once it exists.
type notifier struct{ f atomic.Pointer[func()] }

func (n *notifier) set(f func()) { n.f.Store(&f) }
func (n *notifier) notify()      { (*n.f.Load())() }

// termSize is the size of the terminal open on tty, /dev/tty as tcell
// will use before there is a screen; falling back to the environment
// and then 80×25 as tcell does. A pager's stdin is the pipe and its
// stdout may be too, so neither is asked.
func termSize(tty *os.File) (w, h int) {
	w, h, err := term.GetSize(int(tty.Fd()))
	if err != nil {
		w, h = 0, 0
	}
	if w == 0 {
		w, _ = strconv.Atoi(os.Getenv("COLUMNS"))
	}
	if h == 0 {
		h, _ = strconv.Atoi(os.Getenv("LINES"))
	}
	if w == 0 {
		w = 80
	}
	if h == 0 {
		h = 25
	}
	return w, h
}

// waitOnePage is -F before any screen exists: it waits on data, the
// reader's notifications, until the text outgrows the screen or ends,
// and reports whether to print it instead of paging. Deciding first
// is what keeps short text from flashing through the terminal's
// alternate screen on its way out. The terminal may be resized while
// it waits, so size is asked at every decision and a resize makes
// one.
func waitOnePage(buf *buffer.Buffer, data <-chan struct{}, resize <-chan os.Signal, size func() (w, h int)) bool {
	for {
		w, h := size()
		switch app.OnePage(buf, w, h) {
		case app.Print:
			return true
		case app.Page:
			return false
		}
		select {
		case <-data:
		case <-resize:
		}
	}
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
	// The default config may be missing; one named with --config may not.
	path := config.Path()
	if opts.Config != "" {
		path = opts.Config
	}
	cfg, err := config.Load(path)
	if err != nil && (opts.Config != "" || !errors.Is(err, fs.ErrNotExist)) {
		fmt.Fprintf(os.Stderr, "vedi: %v\n", err)
		os.Exit(1)
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

	buf := buffer.New()
	if src != nil {
		buf = buffer.NewFrom(src)
	}
	data := make(chan struct{}, 1)
	n := new(notifier)
	n.set(func() {
		select {
		case data <- struct{}{}:
		default:
		}
	})
	go buffer.Fill(in, buf, n.notify)
	if opts.QuitIfOnePage {
		// Without a terminal to size, the screen below fails the same way.
		if tty, err := os.Open("/dev/tty"); err == nil {
			resize := make(chan os.Signal, 1)
			signal.Notify(resize, syscall.SIGWINCH)
			printText := waitOnePage(buf, data, resize, func() (int, int) { return termSize(tty) })
			signal.Stop(resize)
			tty.Close()
			if printText {
				buf.WriteTo(os.Stdout)
				return
			}
		}
	}

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
	// XTSHIFTESCAPE: ask for Shift+click, which a terminal otherwise
	// keeps for its own selection.
	if tty, ok := scr.Tty(); ok {
		io.WriteString(tty, "\x1b[>1s")
		defer io.WriteString(tty, "\x1b[>0s") // before Fini closes it
	}

	appOpts := opts.App(scr, files, cfg)
	appOpts.Keys = cfg.Keymap(appOpts.MacOS)
	if src != nil {
		appOpts.Open = reopen(files)
	}
	a := app.New(scr, buf, appOpts)
	defer a.Stop() // before Fini closes the queue
	n.set(a.Notify)
	a.Notify() // for what was read, or ended, before the app existed
	if src != nil && opts.Reloads(cfg) {
		stop := watch.Files(files, postChanged(a))
		defer stop()
	}
	a.Run()
}
