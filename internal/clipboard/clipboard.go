// Package clipboard sends text to the system clipboard.
package clipboard

import (
	"fmt"
	"io"
	"os"
	"os/exec"
	"strings"

	"github.com/gdamore/tcell/v2"
)

type Copier interface {
	Copy(text string) error
}

// OSC52 asks the terminal to set the clipboard. tcell sends it only for
// XTerm-like terminals and gets no acknowledgement.
type OSC52 struct{ Screen tcell.Screen }

func (c OSC52) Copy(text string) error {
	c.Screen.SetClipboard([]byte(text))
	return nil
}

// Command pipes text to a shell command's stdin. A failure's error
// carries what the command wrote to stderr.
type Command struct{ Cmd string }

func (c Command) Copy(text string) error {
	cmd := exec.Command("sh", "-c", c.Cmd)
	cmd.Stdin = strings.NewReader(text)
	// stderr goes to a file, not a pipe: a tool that forks a daemon to
	// serve the selection (xclip) leaves it holding a pipe, and Wait
	// would not return until the selection is lost.
	stderr, ferr := os.CreateTemp("", "vedi-clip-")
	if ferr == nil {
		defer os.Remove(stderr.Name())
		defer stderr.Close()
		cmd.Stderr = stderr
	}
	err := cmd.Run()
	if err != nil && ferr == nil {
		stderr.Seek(0, io.SeekStart)
		out, _ := io.ReadAll(stderr)
		if msg := strings.TrimSpace(string(out)); msg != "" {
			return fmt.Errorf("%w: %s", err, msg)
		}
	}
	return err
}

// New returns Command when cmd is set, otherwise OSC52 on scr.
func New(scr tcell.Screen, cmd string) Copier {
	if cmd != "" {
		return Command{Cmd: cmd}
	}
	return OSC52{Screen: scr}
}
