package cli

import (
	"reflect"
	"testing"

	"github.com/gdamore/tcell/v2"
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
		{"dash dash", []string{"--", "-S"}, Options{}, []string{"-S"}, false},
		{"stdin dash", []string{"-"}, Options{}, []string{"-"}, false},
		{"bad line", []string{"+0"}, Options{}, nil, true},
		{"bad line text", []string{"+abc"}, Options{}, nil, true},
		{"missing cmd", []string{"--clipboard-cmd"}, Options{}, nil, true},
		{"unknown flag", []string{"--bogus"}, Options{}, nil, true},
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
	scr := tcell.NewSimulationScreen("UTF-8")
	got := Options{NoWrap: true, StartLine: 3, Follow: false}.App(scr)
	if got.Mode != layout.NoWrap || got.StartLine != 3 {
		t.Errorf("App() = %+v", got)
	}
	if _, ok := got.Copier.(clipboard.OSC52); !ok {
		t.Errorf("default copier = %T, want OSC52", got.Copier)
	}
	if _, ok := (Options{ClipboardCmd: "pbcopy"}.App(scr).Copier).(clipboard.Command); !ok {
		t.Error("--clipboard-cmd should give a Command copier")
	}
}
