package main

import (
	"errors"
	"fmt"
	"os"
	"os/exec"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/creack/pty"
)

// errTimeout is wait giving up.
var errTimeout = errors.New("timeout")

// session is a pager on an 80x24 pty. scr is what it has drawn; cond
// is signaled on every write and at exit.
type session struct {
	cmd  *exec.Cmd
	tty  *os.File
	mu   sync.Mutex
	cond *sync.Cond
	scr  *screen
	done bool
}

// replies answer the queries a pager sends at startup as a terminal
// would: the primary device attributes as a VT220, the status as OK,
// and the background color as black. A pager waits on these, since
// they come after the answers to its other queries.
var replies = map[string]string{
	"da1": "\x1b[?62;22c",
	"dsr": "\x1b[0n",
	"bg":  "\x1b]11;rgb:0000/0000/0000\x1b\\",
}

// start runs argv on a pty in dir, reading stdin from in when it is
// not nil. The controlling terminal is named by stdout, since stdin
// may be the pipe.
func start(dir string, argv []string, in *os.File) (*session, error) {
	cmd := exec.Command(argv[0], argv[1:]...)
	cmd.Dir = dir
	cmd.Env = env()
	if in != nil {
		cmd.Stdin = in
	}
	attrs := &syscall.SysProcAttr{Setsid: true, Setctty: true, Ctty: 1}
	tty, err := pty.StartWithAttrs(cmd, &pty.Winsize{Rows: 24, Cols: 80}, attrs)
	if err != nil {
		return nil, err
	}
	s := &session{cmd: cmd, tty: tty, scr: newScreen(24, 80)}
	s.cond = sync.NewCond(&s.mu)
	go s.read()
	return s, nil
}

// env is the environment with LESS* and VEDI removed and TERM set, so
// each pager runs with its defaults and less writes no history.
func env() []string {
	var e []string
	for _, kv := range os.Environ() {
		if strings.HasPrefix(kv, "LESS") || strings.HasPrefix(kv, "VEDI=") || strings.HasPrefix(kv, "TERM=") {
			continue
		}
		e = append(e, kv)
	}
	return append(e, "TERM=xterm-256color", "LESSHISTFILE=-")
}

func (s *session) read() {
	p := make([]byte, 64<<10)
	for {
		n, err := s.tty.Read(p)
		s.mu.Lock()
		if n > 0 {
			s.scr.write(p[:n])
		}
		asked := s.scr.asked()
		if err != nil {
			s.done = true
		}
		s.cond.Broadcast()
		s.mu.Unlock()
		for _, q := range asked {
			s.send(replies[q])
		}
		if err != nil {
			return
		}
	}
}

// send types keys.
func (s *session) send(keys string) error {
	_, err := s.tty.Write([]byte(keys))
	return err
}

// wait blocks until text shows on the screen, the pager exits, or
// timeout passes.
func (s *session) wait(text string, timeout time.Duration) error {
	deadline := time.Now().Add(timeout)
	t := time.AfterFunc(timeout, func() {
		s.mu.Lock()
		s.cond.Broadcast()
		s.mu.Unlock()
	})
	defer t.Stop()
	s.mu.Lock()
	defer s.mu.Unlock()
	for {
		if s.scr.contains(text) {
			return nil
		}
		if s.done {
			return fmt.Errorf("exited before %q", text)
		}
		if !time.Now().Before(deadline) {
			return errTimeout
		}
		s.cond.Wait()
	}
}

// finish waits for the pager to exit and returns its peak RSS in
// bytes; after timeout it is killed.
func (s *session) finish(timeout time.Duration) (int64, error) {
	done := make(chan error, 1)
	go func() { done <- s.cmd.Wait() }()
	var err error
	select {
	case err = <-done:
	case <-time.After(timeout):
		s.killGroup()
		<-done
		err = errors.New("did not quit")
	}
	s.tty.Close()
	if err != nil {
		return 0, err
	}
	ru, ok := s.cmd.ProcessState.SysUsage().(*syscall.Rusage)
	if !ok {
		return 0, errors.New("no rusage")
	}
	rss := int64(ru.Maxrss)
	if runtime.GOOS == "linux" {
		rss *= 1024
	}
	return rss, nil
}

// kill ends a pager after a failure.
func (s *session) kill() {
	s.killGroup()
	s.cmd.Wait()
	s.tty.Close()
}

// killGroup kills the pager and what it started: neovim runs the
// editor in a child process, in a session of its own, that would go
// on without its terminal. The pager leads its own process group,
// since start set Setsid.
func (s *session) killGroup() {
	pid := s.cmd.Process.Pid
	if out, err := exec.Command("pgrep", "-P", strconv.Itoa(pid)).Output(); err == nil {
		for f := range strings.FieldsSeq(string(out)) {
			if child, err := strconv.Atoi(f); err == nil {
				syscall.Kill(child, syscall.SIGKILL)
			}
		}
	}
	syscall.Kill(-pid, syscall.SIGKILL)
}
