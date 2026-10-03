// Package opencmd reads an input by running a command on it.
package opencmd

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"os/exec"
	"strings"
	"sync"
	"syscall"
	"time"
)

// A Cmd is a running command's output. Read returns its exit error,
// with its stderr, in place of EOF when it fails.
type Cmd struct {
	cmd    *exec.Cmd
	cancel context.CancelFunc // kills the command, and what it started
	out    io.ReadCloser
	stderr bytes.Buffer

	mu     sync.Mutex
	waited bool
	err    error
}

// Open runs argv with each %s replaced by name, and returns its stdout;
// the command reads nothing. The error is one of starting it; a
// failure after that comes from Read.
func Open(argv []string, name string) (*Cmd, error) {
	args := make([]string, len(argv))
	for i, a := range argv {
		args[i] = strings.ReplaceAll(a, "%s", name)
	}
	ctx, cancel := context.WithCancel(context.Background())
	c := &Cmd{cmd: exec.CommandContext(ctx, args[0], args[1:]...), cancel: cancel}
	c.cmd.Stderr = &c.stderr
	// Its own process group, so cancelling kills what it started too;
	// a descendant that escapes the group and keeps a pipe open holds
	// Wait at most WaitDelay.
	c.cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
	c.cmd.Cancel = func() error { return syscall.Kill(-c.cmd.Process.Pid, syscall.SIGKILL) }
	c.cmd.WaitDelay = time.Second
	out, err := c.cmd.StdoutPipe()
	if err != nil {
		cancel()
		return nil, err
	}
	c.out = out
	if err := c.cmd.Start(); err != nil {
		cancel()
		return nil, err
	}
	return c, nil
}

func (c *Cmd) Read(p []byte) (int, error) {
	n, err := c.out.Read(p)
	if err == io.EOF {
		if werr := c.wait(); werr != nil {
			return n, werr
		}
	}
	return n, err
}

// ReadAt is never called: a command's output is kept in memory, not
// read at random.
func (c *Cmd) ReadAt([]byte, int64) (int, error) { return 0, errors.ErrUnsupported }

// Close ends the command, if it still runs, and reaps it. Cancelling
// rather than killing reaches a command a reader is already waiting
// on.
func (c *Cmd) Close() error {
	c.cancel()
	c.wait()
	return nil
}

// wait reaps the command once and keeps its error: the exit status
// with what it wrote to stderr.
func (c *Cmd) wait() error {
	c.mu.Lock()
	defer c.mu.Unlock()
	if !c.waited {
		c.waited = true
		c.err = c.cmd.Wait()
		c.cancel()
		if msg := strings.TrimSpace(c.stderr.String()); c.err != nil && msg != "" {
			c.err = fmt.Errorf("%w: %s", c.err, msg)
		}
	}
	return c.err
}
