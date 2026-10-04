package input

import (
	"fmt"
	"strings"
)

// Setting is a config setting cycle steps through, and its values in
// order.
type Setting struct {
	Name   string
	Values []string
}

var settings = []Setting{
	{"wrap", []string{"yes", "no"}},
	{"wrap_style", []string{"char", "word"}},
	{"edge_markers", []string{"yes", "no"}},
	{"file_separators", []string{"yes", "no"}},
	{"auto_reload", []string{"yes", "no"}},
	{"status_line", []string{"yes", "no"}},
	{"view_style", []string{"color", "plain"}},
}

// SettingNames is what cycle takes, in order.
func SettingNames() []string {
	names := make([]string, len(settings))
	for i, s := range settings {
		names[i] = s.Name
	}
	return names
}

// LookupSetting finds a setting by name.
func LookupSetting(name string) (Setting, bool) {
	for _, s := range settings {
		if s.Name == name {
			return s, true
		}
	}
	return Setting{}, false
}

// Next is the value after v, the first after the last or after a
// value not in the list.
func (s Setting) Next(v string) string {
	for i, x := range s.Values {
		if x == v {
			return s.Values[(i+1)%len(s.Values)]
		}
	}
	return s.Values[0]
}

// errCycle says what cycle takes.
func errCycle() error {
	names := SettingNames()
	last := len(names) - 1
	return fmt.Errorf("cycle takes %s or %s", strings.Join(names[:last], ", "), names[last])
}
