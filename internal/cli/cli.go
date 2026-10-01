// Package cli parses the command line.
package cli

import (
	"fmt"
	"os"
	"runtime"
	"strconv"
	"strings"

	"github.com/gdamore/tcell/v3"
	"go.dlh.dev/vedi/internal/app"
	"go.dlh.dev/vedi/internal/clipboard"
	"go.dlh.dev/vedi/internal/config"
	"go.dlh.dev/vedi/internal/layout"
)

const Usage = `usage: vedi [flags] [file...]

  -S, --nowrap            start in nowrap mode
  --wrap                  start in wrap mode (the default)
  -F, --quit-if-one-page  print the text and quit if it fits the screen
  --auto-reload           read a file again when it changes on disk (the default)
  --no-auto-reload        leave a changed file as it was read; R still reloads
  +G                      start at the last line and follow until EOF
  +N                      start with line N at the top
  --scrolled-by N         start on the last screenful, scrolled N rows up
  --cursor-row N          put the cursor on row N of the last screenful
  --cursor-col N          put the cursor in column N of the last screenful
  --clipboard-cmd CMD     pipe copied text to CMD instead of OSC 52
  --tab-width N           draw a tab as N cells (8)
  --edge-markers          in nowrap mode, mark text off the sides with < and >
  --no-edge-markers       leave the edges bare (the default)
  --config FILE           read the config from FILE, not ~/.config/vedi/vedi.conf
  -h, --help              show this help
  -v, --version           print the version

Keys: arrows move, Shift+arrows select, Ctrl+C/y copy, Enter copy and
quit, / search, n/N next/prev, w toggle wrap, q quit.
`

type Options struct {
	Wrap          *bool // --wrap, -S; nil leaves it to the config
	QuitIfOnePage bool
	StartLine     int // 1-based; 0 for none
	Follow        bool
	Screen        *app.Screen // set by any of --scrolled-by, --cursor-row, --cursor-col
	ClipboardCmd  string      // --clipboard-cmd; "" leaves it to the config
	TabWidth      int         // --tab-width; 0 leaves it to the config
	Config        string      // "" for the default location
	AutoReload    *bool       // --auto-reload, --no-auto-reload; nil leaves it to the config
	EdgeMarkers   *bool       // --edge-markers, --no-edge-markers; nil leaves it to the config
	Help          bool
	Version       bool
}

// App converts the options to the app's: a flag, else the config,
// decides the wrap mode, the clipboard command, the tab width and the
// edge markers. The
// input is named after files, each as given; "-" and no files are
// "<stdin>".
func (o Options) App(scr tcell.Screen, files []string, cfg config.Config) app.Options {
	mode := layout.Wrap
	if o.Wrap != nil && !*o.Wrap || o.Wrap == nil && cfg.NoWrap {
		mode = layout.NoWrap
	}
	cmd := o.ClipboardCmd
	if cmd == "" {
		cmd = cfg.ClipboardCmd
	}
	tab := o.TabWidth
	if tab == 0 {
		tab = cfg.TabWidth
	}
	marks := cfg.EdgeMarkers
	if o.EdgeMarkers != nil {
		marks = *o.EdgeMarkers
	}
	return app.Options{
		Name:        inputName(files),
		Mode:        mode,
		StartLine:   o.StartLine,
		Follow:      o.Follow,
		Screen:      o.Screen,
		Copier:      clipboard.New(scr, cmd, os.Getenv("TERM_PROGRAM")),
		MacOS:       macOS,
		TabWidth:    tab,
		EdgeMarkers: marks,
	}
}

var macOS = runtime.GOOS == "darwin"

// Reloads says whether a file that changes on disk is read again: as
// the flag says, else the config, else yes.
func (o Options) Reloads(cfg config.Config) bool {
	if o.AutoReload != nil {
		return *o.AutoReload
	}
	return !cfg.NoAutoReload
}

func inputName(files []string) string {
	if len(files) == 0 {
		return "<stdin>"
	}
	names := make([]string, len(files))
	for i, f := range files {
		if f == "-" {
			f = "<stdin>"
		}
		names[i] = f
	}
	return strings.Join(names, " ")
}

// valueFlags take an argument, as "--flag N" or "--flag=N", and set it.
var valueFlags = map[string]func(*Options, string) error{
	"--scrolled-by":   screenInt(0, func(s *app.Screen) *int { return &s.ScrolledBy }),
	"--cursor-row":    screenInt(1, func(s *app.Screen) *int { return &s.CursorRow }),
	"--cursor-col":    screenInt(1, func(s *app.Screen) *int { return &s.CursorCol }),
	"--clipboard-cmd": func(o *Options, v string) error { o.ClipboardCmd = v; return nil },
	"--config":        func(o *Options, v string) error { o.Config = v; return nil },
	"--tab-width": func(o *Options, v string) error {
		n, err := strconv.Atoi(v)
		if err != nil || n < 1 {
			return fmt.Errorf("%q", v)
		}
		o.TabWidth = n
		return nil
	},
}

// screenInt returns a setter that parses an integer of at least min
// into a Screen field, allocating the Screen on first use.
func screenInt(min int, field func(*app.Screen) *int) func(*Options, string) error {
	return func(o *Options, v string) error {
		n, err := strconv.Atoi(v)
		if err != nil || n < min {
			return fmt.Errorf("%q", v)
		}
		if o.Screen == nil {
			o.Screen = &app.Screen{}
		}
		*field(o.Screen) = n
		return nil
	}
}

// Parse parses the command line by hand: package flag does not know
// +N and +G.
func Parse(args []string) (Options, []string, error) {
	var o Options
	var files []string
	for i := 0; i < len(args); i++ {
		a := args[i]
		name, val, hasVal := strings.Cut(a, "=")
		if set, ok := valueFlags[name]; ok {
			if !hasVal {
				if i+1 >= len(args) {
					return o, nil, fmt.Errorf("%s needs an argument", name)
				}
				i++
				val = args[i]
			}
			if err := set(&o, val); err != nil {
				return o, nil, fmt.Errorf("bad %s %v", name, err)
			}
			continue
		}
		switch {
		case a == "-S" || a == "--nowrap":
			o.Wrap = new(false)
		case a == "--wrap":
			o.Wrap = new(true)
		case a == "-F" || a == "--quit-if-one-page":
			o.QuitIfOnePage = true
		case a == "--auto-reload":
			o.AutoReload = new(true)
		case a == "--no-auto-reload":
			o.AutoReload = new(false)
		case a == "--edge-markers":
			o.EdgeMarkers = new(true)
		case a == "--no-edge-markers":
			o.EdgeMarkers = new(false)
		case a == "+G":
			o.Follow = true
		case strings.HasPrefix(a, "+"):
			n, err := strconv.Atoi(a[1:])
			if err != nil || n < 1 {
				return o, nil, fmt.Errorf("bad line number %q", a)
			}
			o.StartLine = n
		case a == "-h" || a == "--help":
			o.Help = true
		case a == "-v" || a == "--version":
			o.Version = true
		case a == "--":
			files = append(files, args[i+1:]...)
			i = len(args)
		case strings.HasPrefix(a, "-") && a != "-":
			return o, nil, fmt.Errorf("unknown flag %q", a)
		default:
			files = append(files, a)
		}
	}
	if o.Screen != nil && (o.StartLine > 0 || o.Follow) {
		return o, nil, fmt.Errorf("--scrolled-by and --cursor-* cannot be combined with +N or +G")
	}
	if o.StartLine > 0 && o.Follow {
		return o, nil, fmt.Errorf("+N and +G cannot be combined")
	}
	return o, files, nil
}
