// Package clipboard sends text to the system clipboard.
package clipboard

import (
	"fmt"
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

// Command pipes text to a shell command's stdin.
type Command struct{ Cmd string }

func (c Command) Copy(text string) error {
	cmd := exec.Command("sh", "-c", c.Cmd)
	cmd.Stdin = strings.NewReader(text)
	out, err := cmd.CombinedOutput()
	if err != nil {
		if msg := strings.TrimSpace(string(out)); msg != "" {
			return fmt.Errorf("%w: %s", err, msg)
		}
		return err
	}
	return nil
}

// New returns Command when cmd is set, otherwise OSC52 on scr.
func New(scr tcell.Screen, cmd string) Copier {
	if cmd != "" {
		return Command{Cmd: cmd}
	}
	return OSC52{Screen: scr}
}
