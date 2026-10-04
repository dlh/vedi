package cli

import (
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

var shells = []string{"bash", "zsh", "fish"}

// usageFlags is every flag the usage text names, a --[no-] pair as
// both.
func usageFlags() []string {
	var flags []string
	for line := range strings.SplitSeq(Usage, "\n") {
		if !strings.HasPrefix(line, "  -") {
			continue
		}
		spec, _, _ := strings.Cut(strings.TrimSpace(line), "  ")
		for f := range strings.SplitSeq(spec, ", ") {
			f, _, _ = strings.Cut(f, " ")
			if rest, ok := strings.CutPrefix(f, "--[no-]"); ok {
				flags = append(flags, "--"+rest, "--no-"+rest)
			} else {
				flags = append(flags, f)
			}
		}
	}
	return flags
}

func TestUsageFlags(t *testing.T) {
	got := strings.Join(usageFlags(), " ")
	for _, f := range []string{"-S", "--wrap", "--no-wrap", "--wrap-style", "-F", "--no-quit-if-one-page", "--completion", "-h", "--version"} {
		if !strings.Contains(" "+got+" ", " "+f+" ") {
			t.Errorf("%s missing from %s", f, got)
		}
	}
}

// TestCompletionFlags: a flag added to the usage text fails here until
// each script completes it.
func TestCompletionFlags(t *testing.T) {
	for _, sh := range shells {
		script := Completion(sh)
		if script == "" {
			t.Fatalf("no %s script", sh)
		}
		for _, f := range usageFlags() {
			pat := `(^|[^a-z-])` + regexp.QuoteMeta(f) + `([^a-z-]|$)`
			if sh == "fish" {
				if name, long := strings.CutPrefix(f, "--"); long {
					pat = `-l ` + regexp.QuoteMeta(name) + `( |$)`
				} else {
					pat = `-s ` + f[1:] + `( |$)`
				}
			}
			if !regexp.MustCompile(`(?m)` + pat).MatchString(script) {
				t.Errorf("%s: no %s", sh, f)
			}
		}
	}
}

func TestCompletionUnknownShell(t *testing.T) {
	if got := Completion("tcsh"); got != "" {
		t.Fatalf("got %q", got)
	}
}

// TestCompletionBash runs the script in bash, and in macOS's 3.2 where
// there is one.
func TestCompletionBash(t *testing.T) {
	bins := map[string]bool{}
	if p, err := exec.LookPath("bash"); err == nil {
		bins[p] = true
	}
	if _, err := os.Stat("/bin/bash"); err == nil {
		bins["/bin/bash"] = true
	}
	if len(bins) == 0 {
		t.Skip("no bash")
	}
	dir := t.TempDir()
	script := filepath.Join(dir, "vedi.bash")
	if err := os.WriteFile(script, []byte(Completion("bash")), 0o644); err != nil {
		t.Fatal(err)
	}
	work := filepath.Join(dir, "work")
	if err := os.Mkdir(work, 0o755); err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"a log.txt", "-x.txt", "report[1].txt", "report1.txt"} {
		if err := os.WriteFile(filepath.Join(work, name), nil, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	// words is the command line as bash splits it: = is a word of its own.
	for _, tc := range []struct {
		name  string
		words []string
		want  string
	}{
		{"flag prefix", []string{"--wr"}, "--wrap\n--wrap-style"},
		{"no- form", []string{"--no-w"}, "--no-wrap"},
		{"short flags", []string{"-F"}, "-F"},
		{"wrap style", []string{"--wrap-style", ""}, "char\nword"},
		{"wrap style prefix", []string{"--wrap-style", "w"}, "word"},
		{"wrap style eq", []string{"--wrap-style", "="}, "char\nword"},
		{"wrap style eq prefix", []string{"--wrap-style", "=", "c"}, "char"},
		{"view style", []string{"--view-style", ""}, "color\nplain\nraw"},
		{"shells", []string{"--completion", ""}, "bash\nzsh\nfish"},
		{"number", []string{"--tab-width", ""}, ""},
		{"number eq", []string{"--tab-width", "="}, ""},
		{"config", []string{"--config", "a"}, "a log.txt"},
		{"command", []string{"--open-cmd", "prin"}, "printf"},
		{"file", []string{"-S", "a"}, "a log.txt"},
		{"file after dash dash", []string{"--", "-"}, "-x.txt"},
		{"file with a glob character", []string{"report["}, "report[1].txt"},
	} {
		for bash := range bins {
			t.Run(tc.name+" "+bash, func(t *testing.T) {
				args := append([]string{"--norc", "--noprofile", "-c", `
source "$1"; shift
COMP_WORDS=(vedi "$@"); COMP_CWORD=$((${#COMP_WORDS[@]} - 1))
_vedi
[ ${#COMPREPLY[@]} -eq 0 ] || printf '%s\n' "${COMPREPLY[@]}"`, "_", script}, tc.words...)
				cmd := exec.Command(bash, args...)
				cmd.Dir = work
				out, err := cmd.CombinedOutput()
				if err != nil {
					t.Fatalf("%v: %s", err, out)
				}
				got := strings.TrimSuffix(string(out), "\n")
				// Commands vary by machine; printf is always among them.
				if tc.name == "command" {
					if !strings.Contains("\n"+got+"\n", "\n"+tc.want+"\n") {
						t.Fatalf("got %q, want %q among them", got, tc.want)
					}
					return
				}
				if got != tc.want {
					t.Fatalf("got %q, want %q", got, tc.want)
				}
			})
		}
	}
}

// TestCompletionSyntax: zsh and fish parse their scripts.
func TestCompletionSyntax(t *testing.T) {
	for _, sh := range []string{"zsh", "fish"} {
		t.Run(sh, func(t *testing.T) {
			bin, err := exec.LookPath(sh)
			if err != nil {
				t.Skip("no " + sh)
			}
			path := filepath.Join(t.TempDir(), "vedi."+sh)
			if err := os.WriteFile(path, []byte(Completion(sh)), 0o644); err != nil {
				t.Fatal(err)
			}
			if out, err := exec.Command(bin, "-n", path).CombinedOutput(); err != nil {
				t.Fatalf("%v: %s", err, out)
			}
		})
	}
}
