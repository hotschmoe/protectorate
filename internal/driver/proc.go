package driver

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"sync"
	"time"
)

const maxLine = 64 << 20

// tailBuf keeps the last n bytes written to it.
type tailBuf struct {
	mu  sync.Mutex
	n   int
	buf []byte
}

func (t *tailBuf) Write(p []byte) (int, error) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.buf = append(t.buf, p...)
	if len(t.buf) > t.n {
		t.buf = append([]byte(nil), t.buf[len(t.buf)-t.n:]...)
	}
	return len(p), nil
}

func (t *tailBuf) String() string {
	t.mu.Lock()
	defer t.mu.Unlock()
	return string(t.buf)
}

// proc is a supervised child speaking line-delimited JSON on stdio. It runs
// in its own process group so wrapper scripts and their children die with it.
type proc struct {
	cmd    *exec.Cmd
	stdin  io.WriteCloser
	stdout io.ReadCloser
	stderr *tailBuf
	wmu    sync.Mutex
	done   chan struct{}
}

func startProc(bin string, args []string, cwd string, opts Options) (*proc, error) {
	limit := opts.StderrLimit
	if limit <= 0 {
		limit = 8 << 10
	}
	if opts.Command != nil {
		argv, err := opts.Command(bin, args)
		if err != nil {
			return nil, fmt.Errorf("build %s command: %w", bin, err)
		}
		if len(argv) == 0 {
			return nil, fmt.Errorf("build %s command: empty argv", bin)
		}
		bin, args = argv[0], argv[1:]
	}
	cmd := exec.Command(bin, args...)
	cmd.Dir = cwd
	cmd.Env = append(os.Environ(), opts.Env...)
	p := &proc{cmd: cmd, stderr: &tailBuf{n: limit}, done: make(chan struct{})}
	cmd.Stderr = p.stderr
	configureProc(cmd)
	var err error
	if p.stdin, err = cmd.StdinPipe(); err != nil {
		return nil, fmt.Errorf("stdin pipe: %w", err)
	}
	if p.stdout, err = cmd.StdoutPipe(); err != nil {
		return nil, fmt.Errorf("stdout pipe: %w", err)
	}
	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("start %s: %w", bin, err)
	}
	return p, nil
}

// writeJSON writes one message line; writes are serialized.
func (p *proc) writeJSON(v any) error {
	b, err := json.Marshal(v)
	if err != nil {
		return err
	}
	p.wmu.Lock()
	defer p.wmu.Unlock()
	_, err = p.stdin.Write(append(b, '\n'))
	return err
}

// run feeds every stdout line to handle until EOF, reaps the child and
// finishes the session. It must be started as a goroutine exactly once.
func (p *proc) run(c *core, handle func(line []byte)) {
	sc := bufio.NewScanner(p.stdout)
	sc.Buffer(make([]byte, 64<<10), maxLine)
	for sc.Scan() {
		if line := sc.Bytes(); len(line) > 0 {
			handle(line)
		}
	}
	scanErr := sc.Err()
	if scanErr != nil {
		killProc(p.cmd)
	}
	err := p.cmd.Wait()
	code := 0
	if err != nil {
		code = -1
		if ee, ok := err.(*exec.ExitError); ok {
			code = ee.ExitCode()
		}
	}
	tail := p.stderr.String()
	if scanErr != nil {
		tail = fmt.Sprintf("read stdout: %v; %s", scanErr, tail)
	}
	close(p.done)
	c.finish(code, tail)
}

func (c *core) closeProc(p *proc) error {
	if c.markShut() {
		p.stop(c.opts.stopTimeout())
	}
	<-c.done
	return nil
}

// stop closes stdin and waits for a graceful exit, then escalates to
// SIGTERM and SIGKILL on the process group.
func (p *proc) stop(timeout time.Duration) {
	_ = p.stdin.Close()
	if waitDone(p.done, timeout) {
		return
	}
	termProc(p.cmd)
	if waitDone(p.done, 2*time.Second) {
		return
	}
	killProc(p.cmd)
	<-p.done
}

func waitDone(ch <-chan struct{}, d time.Duration) bool {
	t := time.NewTimer(d)
	defer t.Stop()
	select {
	case <-ch:
		return true
	case <-t.C:
		return false
	}
}
