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
  --wrap-style STYLE      wrap at the screen's edge (char) or at words (word)
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
  --edge-markers          mark text off the sides with < and >, a wrapped row with \
  --no-edge-markers       leave the edges bare (the default)
  --config FILE           read the config from FILE, not ~/.config/vedi/vedi.conf
  -h, --help              show this help
  -v, --version           print the version

Flags in the VEDI environment variable apply to every run; the
command line wins.

Keys: arrows move, Shift+arrows select, Ctrl+C/y copy, Enter copy and
quit, / search, n/N next/prev, w toggle wrap, q quit.
`

type Options struct {
	Wrap          *bool             // --wrap, -S; nil leaves it to the config
	WrapStyle     *layout.WrapStyle // --wrap-style; nil leaves it to the config
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
// decides the wrap mode, the wrap style, the clipboard
// command, the tab width, the edge markers and auto-reload. The
// inputs are named after files, each as given; "-" and no files are
// "<stdin>".
func (o Options) App(scr tcell.Screen, files []string, cfg config.Config) app.Options {
	mode := layout.Wrap
	if o.Wrap != nil && !*o.Wrap || o.Wrap == nil && cfg.NoWrap {
		mode = layout.NoWrap
	}
	style := cfg.WrapStyle
	if o.WrapStyle != nil {
		style = *o.WrapStyle
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
		Names:       inputNames(files),
		Mode:        mode,
		WrapStyle:   style,
		StartLine:   o.StartLine,
		Follow:      o.Follow,
		Screen:      o.Screen,
		Copier:      clipboard.New(scr, cmd, os.Getenv("TERM_PROGRAM")),
		MacOS:       macOS,
		TabWidth:    tab,
		EdgeMarkers: marks,
		AutoReload:  o.Reloads(cfg),
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

func inputNames(files []string) []string {
	if len(files) == 0 {
		return []string{app.Stdin}
	}
	names := make([]string, len(files))
	for i, f := range files {
		if f == "-" {
			f = app.Stdin
		}
		names[i] = f
	}
	return names
}

// valueFlags take an argument, as "--flag N" or "--flag=N", and set it.
var valueFlags = map[string]func(*Options, string) error{
	"--scrolled-by":   screenInt(0, func(s *app.Screen) *int { return &s.ScrolledBy }),
	"--cursor-row":    screenInt(1, func(s *app.Screen) *int { return &s.CursorRow }),
	"--cursor-col":    screenInt(1, func(s *app.Screen) *int { return &s.CursorCol }),
	"--clipboard-cmd": func(o *Options, v string) error { o.ClipboardCmd = v; return nil },
	"--config":        func(o *Options, v string) error { o.Config = v; return nil },
	"--wrap-style": func(o *Options, v string) error {
		switch v {
		case "char":
			o.WrapStyle = new(layout.WrapStyleChar)
		case "word":
			o.WrapStyle = new(layout.WrapStyleWord)
		default:
			return fmt.Errorf("%q", v)
		}
		return nil
	},
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
// +N and +G. env is $VEDI, flags for every run, parsed first so the
// command line wins; a start position there gives way to one on the
// command line.
func Parse(args []string, env string) (Options, []string, error) {
	var o Options
	words, err := split(env)
	if err == nil {
		_, err = o.parse(words, true)
	}
	if err != nil {
		return o, nil, fmt.Errorf("VEDI: %v", err)
	}
	fromEnv := o
	o.StartLine, o.Follow, o.Screen = 0, false, nil
	files, err := o.parse(args, false)
	if err != nil {
		return o, nil, err
	}
	if o.StartLine == 0 && !o.Follow && o.Screen == nil {
		o.StartLine, o.Follow, o.Screen = fromEnv.StartLine, fromEnv.Follow, fromEnv.Screen
	}
	return o, files, nil
}

// split splits s into words as a shell would: on blanks, with '...'
// taken as written, "..." too but for \" and \\, and a \ outside
// quotes standing for the character after it.
func split(s string) ([]string, error) {
	var words []string
	var w strings.Builder
	inWord := false
	var quote byte
	for i := 0; i < len(s); i++ {
		c := s[i]
		switch {
		case quote != 0 && c == quote:
			quote = 0
		case quote == '\'':
			w.WriteByte(c)
		case c == '\\':
			if i+1 == len(s) {
				return nil, fmt.Errorf("trailing \\")
			}
			if quote == '"' && s[i+1] != '"' && s[i+1] != '\\' {
				w.WriteByte(c)
				continue
			}
			i++
			w.WriteByte(s[i])
			inWord = true
		case quote == '"':
			w.WriteByte(c)
		case c == '\'' || c == '"':
			quote = c
			inWord = true
		case c == ' ' || c == '\t' || c == '\n':
			if inWord {
				words = append(words, w.String())
				w.Reset()
				inWord = false
			}
		default:
			w.WriteByte(c)
			inWord = true
		}
	}
	if quote != 0 {
		return nil, fmt.Errorf("unclosed %c", quote)
	}
	if inWord {
		words = append(words, w.String())
	}
	return words, nil
}

// parse lays args over o and returns the files. env is the words of
// $VEDI, which may only be flags, and not -h or -v.
func (o *Options) parse(args []string, env bool) ([]string, error) {
	var files []string
	for i := 0; i < len(args); i++ {
		a := args[i]
		name, val, hasVal := strings.Cut(a, "=")
		if set, ok := valueFlags[name]; ok {
			if !hasVal {
				if i+1 >= len(args) {
					return nil, fmt.Errorf("%s needs an argument", name)
				}
				i++
				val = args[i]
			}
			if err := set(o, val); err != nil {
				return nil, fmt.Errorf("bad %s %v", name, err)
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
				return nil, fmt.Errorf("bad line number %q", a)
			}
			o.StartLine = n
		case a == "-h" || a == "--help":
			if env {
				return nil, fmt.Errorf("%q is not allowed", a)
			}
			o.Help = true
		case a == "-v" || a == "--version":
			if env {
				return nil, fmt.Errorf("%q is not allowed", a)
			}
			o.Version = true
		case env && (a == "--" || a == "-" || !strings.HasPrefix(a, "-")):
			return nil, fmt.Errorf("%q is not a flag", a)
		case a == "--":
			files = append(files, args[i+1:]...)
			i = len(args)
		case strings.HasPrefix(a, "-") && a != "-":
			return nil, fmt.Errorf("unknown flag %q", a)
		default:
			files = append(files, a)
		}
	}
	if o.Screen != nil && (o.StartLine > 0 || o.Follow) {
		return nil, fmt.Errorf("--scrolled-by and --cursor-* cannot be combined with +N or +G")
	}
	if o.StartLine > 0 && o.Follow {
		return nil, fmt.Errorf("+N and +G cannot be combined")
	}
	return files, nil
}
