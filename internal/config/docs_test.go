package config

import (
	"os"
	"strings"
	"testing"
)

// blocks returns the first fenced code block after each "## " heading
// of a markdown file, by heading.
func blocks(t *testing.T, path string) map[string]string {
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	out := map[string]string{}
	heading := ""
	var body *strings.Builder
	for line := range strings.SplitSeq(string(data), "\n") {
		switch {
		case strings.HasPrefix(line, "## "):
			heading = strings.TrimPrefix(line, "## ")
		case strings.HasPrefix(line, "```") && body != nil:
			if _, seen := out[heading]; !seen {
				out[heading] = body.String()
			}
			body = nil
		case strings.HasPrefix(line, "```"):
			body = &strings.Builder{}
		case body != nil:
			body.WriteString(line + "\n")
		}
	}
	return out
}

// TestConfigurationDoc keeps docs/configuration.md true: its Defaults
// block is the embedded file, and its examples parse.
func TestConfigurationDoc(t *testing.T) {
	b := blocks(t, "../../docs/configuration.md")
	if b["Defaults"] != string(defaults) {
		t.Error("the Defaults block differs from defaults.conf")
	}
	for _, name := range []string{"vim", "emacs"} {
		src, ok := b[name]
		if !ok {
			t.Fatalf("no %s block", name)
		}
		c, err := Parse(name, []byte(src))
		if err != nil {
			t.Errorf("%s example: %v", name, err)
		}
		if len(c.Keys) == 0 {
			t.Errorf("%s example binds nothing", name)
		}
	}
}
