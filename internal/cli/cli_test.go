package cli

import (
	"reflect"
	"testing"

	"github.com/gdamore/tcell/v2"
	"go.dlh.dev/vedi/internal/app"
	"go.dlh.dev/vedi/internal/clipboard"
	"go.dlh.dev/vedi/internal/layout"
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
		{"nowrap short", []string{"-S", "a"}, Options{NoWrap: true}, []string{"a"}, false},
		{"nowrap long", []string{"--nowrap"}, Options{NoWrap: true}, nil, false},
		{"plus G", []string{"+G"}, Options{Follow: true}, nil, false},
		{"plus N", []string{"+12", "f"}, Options{StartLine: 12}, []string{"f"}, false},
		{"clipboard cmd", []string{"--clipboard-cmd", "pbcopy"}, Options{ClipboardCmd: "pbcopy"}, nil, false},
		{"clipboard cmd eq", []string{"--clipboard-cmd=wl-copy -n"}, Options{ClipboardCmd: "wl-copy -n"}, nil, false},
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
		{"flag after file", []string{"a", "-S"}, Options{NoWrap: true}, []string{"a"}, false},
		{"clipboard cmd empty eq", []string{"--clipboard-cmd="}, Options{}, nil, false},
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

func TestApp(t *testing.T) {
	t.Setenv("TERM_PROGRAM", "")
	scr := tcell.NewSimulationScreen("UTF-8")
	got := Options{NoWrap: true, StartLine: 3, Follow: false, Screen: &app.Screen{CursorRow: 2}}.App(scr, nil)
	if got.Mode != layout.NoWrap || got.StartLine != 3 || got.Screen.CursorRow != 2 {
		t.Errorf("App() = %+v", got)
	}
	if _, ok := got.Copier.(clipboard.OSC52); !ok {
		t.Errorf("default copier = %T, want OSC52", got.Copier)
	}
	if _, ok := (Options{ClipboardCmd: "pbcopy"}.App(scr, nil).Copier).(clipboard.Command); !ok {
		t.Error("--clipboard-cmd should give a Command copier")
	}
	t.Setenv("TERM_PROGRAM", "Apple_Terminal")
	if c, ok := (Options{}.App(scr, nil).Copier).(clipboard.Command); !ok || c.Cmd != "pbcopy" {
		t.Error("Terminal.app should default to pbcopy")
	}
}
