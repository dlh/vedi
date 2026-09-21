package main

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"runtime"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/creack/pty"
)

// errTimeout is wait giving up.
var errTimeout = errors.New("timeout")

// session is a pager on an 80x24 pty. out is what it has written,
// escape sequences stripped; cond is signaled on every append and at
// exit.
type session struct {
	cmd  *exec.Cmd
	tty  *os.File
	mu   sync.Mutex
	cond *sync.Cond
	out  []byte
	done bool
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
	s := &session{cmd: cmd, tty: tty}
	s.cond = sync.NewCond(&s.mu)
	go s.read()
	return s, nil
}

// env is the environment with LESS* removed and TERM set, so less runs
// with its defaults and writes no history.
func env() []string {
	var e []string
	for _, kv := range os.Environ() {
		if strings.HasPrefix(kv, "LESS") || strings.HasPrefix(kv, "TERM=") {
			continue
		}
		e = append(e, kv)
	}
	return append(e, "TERM=xterm-256color", "LESSHISTFILE=-")
}

func (s *session) read() {
	var st stripper
	p := make([]byte, 64<<10)
	for {
		n, err := s.tty.Read(p)
		s.mu.Lock()
		if n > 0 {
			s.out = st.strip(s.out, p[:n])
		}
		if err != nil {
			s.done = true
		}
		s.cond.Broadcast()
		s.mu.Unlock()
		if err != nil {
			return
		}
	}
}

// mark is how much output there is now: a from for wait.
func (s *session) mark() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return len(s.out)
}

// send types keys.
func (s *session) send(keys string) error {
	_, err := s.tty.Write([]byte(keys))
	return err
}

// wait blocks until text appears in the output after from, the pager
// exits, or timeout passes.
func (s *session) wait(text string, from int, timeout time.Duration) error {
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
		if bytes.Contains(s.out[from:], []byte(text)) {
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
		s.cmd.Process.Kill()
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
	s.cmd.Process.Kill()
	s.cmd.Wait()
	s.tty.Close()
}
