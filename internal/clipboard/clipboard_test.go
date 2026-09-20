package clipboard

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/gdamore/tcell/v2"
)

func TestOSC52(t *testing.T) {
	scr := tcell.NewSimulationScreen("UTF-8")
	if err := scr.Init(); err != nil {
		t.Fatal(err)
	}
	defer scr.Fini()
	if err := (OSC52{Screen: scr}).Copy("héllo\nworld"); err != nil {
		t.Fatal(err)
	}
	if got := string(scr.GetClipboardData()); got != "héllo\nworld" {
		t.Fatalf("clipboard = %q", got)
	}
}

func TestCommandPipesText(t *testing.T) {
	out := filepath.Join(t.TempDir(), "clip")
	if err := (Command{Cmd: "cat > " + out}).Copy("a b\nc"); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(out)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != "a b\nc" {
		t.Fatalf("file = %q", data)
	}
}

func TestCommandFailure(t *testing.T) {
	err := (Command{Cmd: "echo nope >&2; exit 3"}).Copy("x")
	if err == nil {
		t.Fatal("expected an error")
	}
	if !strings.Contains(err.Error(), "exit status 3") || !strings.Contains(err.Error(), "nope") {
		t.Fatalf("error = %q, want exit status and stderr", err)
	}
}

// A tool that forks a daemon to serve the selection (xclip) leaves a
// child holding stdout and stderr; Copy must return when the command
// exits, not when the child does.
func TestCommandReturnsWhileChildHoldsPipes(t *testing.T) {
	done := make(chan error, 1)
	go func() { done <- (Command{Cmd: "sleep 5 & exit 0"}).Copy("x") }()
	select {
	case err := <-done:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("Copy waited for the child")
	}
}

func TestNew(t *testing.T) {
	scr := tcell.NewSimulationScreen("UTF-8")
	if _, ok := New(scr, "").(OSC52); !ok {
		t.Error("New with empty cmd should be OSC52")
	}
	if c, ok := New(scr, "pbcopy").(Command); !ok || c.Cmd != "pbcopy" {
		t.Error("New with cmd should be Command")
	}
}
