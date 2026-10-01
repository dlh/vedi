package cli

import (
	"reflect"
	"testing"

	"go.dlh.dev/vedi/internal/app"
	"go.dlh.dev/vedi/internal/clipboard"
	"go.dlh.dev/vedi/internal/config"
	"go.dlh.dev/vedi/internal/layout"
	"go.dlh.dev/vedi/internal/testscreen"
)

func TestParse(t *testing.T) {
	tests := []struct {
		name    string
		args    []string
		want    Options
		files   []string
		wantErr bool
	}{
		{"none", nil, Options{}, nil, false},
		{"file", []string{"a.txt"}, Options{}, []string{"a.txt"}, false},
		{"nowrap short", []string{"-S", "a"}, Options{Wrap: new(false)}, []string{"a"}, false},
		{"nowrap long", []string{"--nowrap"}, Options{Wrap: new(false)}, nil, false},
		{"wrap", []string{"--wrap"}, Options{Wrap: new(true)}, nil, false},
		{"last wrap wins", []string{"-S", "--wrap"}, Options{Wrap: new(true)}, nil, false},
		{"plus G", []string{"+G"}, Options{Follow: true}, nil, false},
		{"plus N", []string{"+12", "f"}, Options{StartLine: 12}, []string{"f"}, false},
		{"clipboard cmd", []string{"--clipboard-cmd", "pbcopy"}, Options{ClipboardCmd: "pbcopy"}, nil, false},
		{"clipboard cmd eq", []string{"--clipboard-cmd=wl-copy -n"}, Options{ClipboardCmd: "wl-copy -n"}, nil, false},
		{"config", []string{"--config", "/tmp/k.conf"}, Options{Config: "/tmp/k.conf"}, nil, false},
		{"config eq", []string{"--config=k.conf", "f"}, Options{Config: "k.conf"}, []string{"f"}, false},
		{"missing config", []string{"--config"}, Options{}, nil, true},
		{"quit if one page short", []string{"-F"}, Options{QuitIfOnePage: true}, nil, false},
		{"quit if one page long", []string{"--quit-if-one-page"}, Options{QuitIfOnePage: true}, nil, false},
		{"help", []string{"-h"}, Options{Help: true}, nil, false},
		{"version short", []string{"-v"}, Options{Version: true}, nil, false},
		{"version long", []string{"--version"}, Options{Version: true}, nil, false},
		{"dash dash", []string{"--", "-S"}, Options{}, []string{"-S"}, false},
		{"stdin dash", []string{"-"}, Options{}, []string{"-"}, false},
		{"bad line", []string{"+0"}, Options{}, nil, true},
		{"bad line text", []string{"+abc"}, Options{}, nil, true},
		{"missing cmd", []string{"--clipboard-cmd"}, Options{}, nil, true},
		{"unknown flag", []string{"--bogus"}, Options{}, nil, true},
		{"scrolled by eq", []string{"--scrolled-by=0"}, Options{Screen: &app.Screen{}}, nil, false},
		{"cursor", []string{"--cursor-row", "20", "--cursor-col", "5"}, Options{Screen: &app.Screen{CursorRow: 20, CursorCol: 5}}, nil, false},
		{"bad scrolled by", []string{"--scrolled-by", "-1"}, Options{}, nil, true},
		{"bad cursor row", []string{"--cursor-row", "0"}, Options{}, nil, true},
		{"screen conflicts with plus N", []string{"+5", "--scrolled-by", "0"}, Options{}, nil, true},
		{"screen conflicts with plus G", []string{"--cursor-row", "1", "+G"}, Options{}, nil, true},
		{"plus G conflicts with plus N", []string{"+G", "+5"}, Options{}, nil, true},
		{"flag after file", []string{"a", "-S"}, Options{Wrap: new(false)}, []string{"a"}, false},
		{"clipboard cmd empty eq", []string{"--clipboard-cmd="}, Options{}, nil, false},
		{"auto reload", []string{"--auto-reload"}, Options{AutoReload: new(true)}, nil, false},
		{"no auto reload", []string{"--no-auto-reload", "f"}, Options{AutoReload: new(false)}, []string{"f"}, false},
		{"last auto reload wins", []string{"--no-auto-reload", "--auto-reload"}, Options{AutoReload: new(true)}, nil, false},
		{"tab width", []string{"--tab-width", "4", "f"}, Options{TabWidth: 4}, []string{"f"}, false},
		{"tab width eq", []string{"--tab-width=2"}, Options{TabWidth: 2}, nil, false},
		{"missing tab width", []string{"--tab-width"}, Options{}, nil, true},
		{"bad tab width", []string{"--tab-width", "0"}, Options{}, nil, true},
		{"bad tab width text", []string{"--tab-width", "four"}, Options{}, nil, true},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got, files, err := Parse(tc.args)
			if (err != nil) != tc.wantErr {
				t.Fatalf("err = %v, wantErr %v", err, tc.wantErr)
			}
			if err != nil {
				return
			}
			if !reflect.DeepEqual(got, tc.want) || !reflect.DeepEqual(files, tc.files) {
				t.Fatalf("got %+v %v, want %+v %v", got, files, tc.want, tc.files)
			}
		})
	}
}

// TestReloads: the flag decides; without one the config does; without
// either a changed file is read again.
func TestReloads(t *testing.T) {
	for _, tc := range []struct {
		flag *bool
		cfg  config.Config
		want bool
	}{
		{nil, config.Config{}, true},
		{nil, config.Config{NoAutoReload: true}, false},
		{new(false), config.Config{}, false},
		{new(true), config.Config{NoAutoReload: true}, true},
	} {
		if got := (Options{AutoReload: tc.flag}).Reloads(tc.cfg); got != tc.want {
			t.Errorf("Options{AutoReload: %v}.Reloads(%+v) = %v, want %v", tc.flag, tc.cfg, got, tc.want)
		}
	}
}

func TestApp(t *testing.T) {
	t.Setenv("TERM_PROGRAM", "")
	scr := testscreen.New(t, 80, 24)
	none := config.Config{}
	got := Options{Wrap: new(false), StartLine: 3, Follow: false, Screen: &app.Screen{CursorRow: 2}}.App(scr, nil, none)
	if got.Mode != layout.NoWrap || got.StartLine != 3 || got.Screen.CursorRow != 2 {
		t.Errorf("App() = %+v", got)
	}
	if _, ok := got.Copier.(clipboard.OSC52); !ok {
		t.Errorf("default copier = %T, want OSC52", got.Copier)
	}
	if _, ok := (Options{ClipboardCmd: "pbcopy"}.App(scr, nil, none).Copier).(clipboard.Command); !ok {
		t.Error("--clipboard-cmd should give a Command copier")
	}
	t.Setenv("TERM_PROGRAM", "Apple_Terminal")
	if c, ok := (Options{}.App(scr, nil, none).Copier).(clipboard.Command); !ok || c.Cmd != "pbcopy" {
		t.Error("Terminal.app should default to pbcopy")
	}
}

// TestAppConfig: the flag decides the wrap mode and clipboard command;
// without one the config does.
func TestAppConfig(t *testing.T) {
	t.Setenv("TERM_PROGRAM", "")
	scr := testscreen.New(t, 80, 24)
	cfg := config.Config{NoWrap: true, ClipboardCmd: "wl-copy"}
	for _, tc := range []struct {
		opts Options
		cfg  config.Config
		mode layout.Mode
		cmd  string
	}{
		{Options{}, config.Config{}, layout.Wrap, ""},
		{Options{}, cfg, layout.NoWrap, "wl-copy"},
		{Options{Wrap: new(true), ClipboardCmd: "pbcopy"}, cfg, layout.Wrap, "pbcopy"},
		{Options{Wrap: new(false)}, config.Config{}, layout.NoWrap, ""},
	} {
		got := tc.opts.App(scr, nil, tc.cfg)
		cmd := ""
		if c, ok := got.Copier.(clipboard.Command); ok {
			cmd = c.Cmd
		}
		if got.Mode != tc.mode || cmd != tc.cmd {
			t.Errorf("%+v.App(%+v) = mode %v cmd %q, want %v %q", tc.opts, tc.cfg, got.Mode, cmd, tc.mode, tc.cmd)
		}
	}
}

// TestAppTabWidth: the flag decides the tab width; without one the
// config does; without either it is zero, the app's default.
func TestAppTabWidth(t *testing.T) {
	t.Setenv("TERM_PROGRAM", "")
	scr := testscreen.New(t, 80, 24)
	for _, tc := range []struct {
		opts Options
		cfg  config.Config
		want int
	}{
		{Options{}, config.Config{}, 0},
		{Options{}, config.Config{TabWidth: 4}, 4},
		{Options{TabWidth: 2}, config.Config{TabWidth: 4}, 2},
		{Options{TabWidth: 2}, config.Config{}, 2},
	} {
		if got := tc.opts.App(scr, nil, tc.cfg).TabWidth; got != tc.want {
			t.Errorf("%+v.App(%+v).TabWidth = %d, want %d", tc.opts, tc.cfg, got, tc.want)
		}
	}
}

// TestEdgeMarkersFlag: --edge-markers and --no-edge-markers decide;
// without one the config does; without either the markers are off.
func TestEdgeMarkersFlag(t *testing.T) {
	scr := testscreen.New(t, 80, 24)
	for _, tc := range []struct {
		flag *bool
		cfg  config.Config
		want bool
	}{
		{nil, config.Config{}, false},
		{nil, config.Config{EdgeMarkers: true}, true},
		{new(true), config.Config{}, true},
		{new(false), config.Config{EdgeMarkers: true}, false},
	} {
		if got := (Options{EdgeMarkers: tc.flag}).App(scr, nil, tc.cfg).EdgeMarkers; got != tc.want {
			t.Errorf("Options{EdgeMarkers: %v}.App(%+v).EdgeMarkers = %v, want %v", tc.flag, tc.cfg, got, tc.want)
		}
	}
	for _, tc := range []struct {
		arg  string
		want bool
	}{{"--edge-markers", true}, {"--no-edge-markers", false}} {
		o, _, err := Parse([]string{tc.arg})
		if err != nil || o.EdgeMarkers == nil || *o.EdgeMarkers != tc.want {
			t.Errorf("Parse(%q) = %+v, %v; want EdgeMarkers %v", tc.arg, o, err, tc.want)
		}
	}
}
