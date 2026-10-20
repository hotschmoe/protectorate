package node

import (
	"bytes"
	"context"
	"crypto/rand"
	"fmt"
	"os"
	"strings"
	"sync"
	"time"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

// A warm worktree's config and filters are job-controlled. Git must have
// the same authority as the job, including on Windows runner nodes.
func (j *Jobs) gitOut(ctx context.Context, dir, jobDir string, args ...string) (string, error) {
	ctx, cancel := context.WithTimeout(ctx, 20*time.Second)
	defer cancel()
	if err := os.MkdirAll(jobDir, 0o700); err != nil {
		return "", fmt.Errorf("prepare git job directory: %w", err)
	}
	req := &pb.RunJobRequest{
		JobId: "git-" + rand.Text(),
		Command: append([]string{"git", "--no-pager", "-c", "core.autocrlf=false", "-c", "core.longpaths=true",
			"-c", "gc.auto=0", "-c", "core.hooksPath=" + os.DevNull, "-c", "core.fsmonitor=false"}, args...),
		Env: map[string]string{"GIT_TERMINAL_PROMPT": "0", "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.DevNull},
	}
	argv, env, _, err := j.commandFor(req, dir, jobDir)
	if err != nil {
		return "", fmt.Errorf("prepare git %s: %w", args[0], err)
	}
	var out gitOutput
	p, err := j.launch(req, argv, env, dir, jobDir, &out)
	if err != nil {
		return "", fmt.Errorf("start git %s: %w", args[0], err)
	}
	defer p.close()
	defer p.kill()
	done := make(chan int, 1)
	go func() { done <- p.wait() }()
	select {
	case code := <-done:
		if code != 0 {
			return "", fmt.Errorf("git %s exited %d: %s", args[0], code, strings.TrimSpace(out.String()))
		}
		return out.String(), nil
	case <-ctx.Done():
		p.kill()
		<-done
		return "", fmt.Errorf("git %s: %w", args[0], ctx.Err())
	}
}

type gitOutput struct {
	mu     sync.Mutex
	buffer bytes.Buffer
}

func (w *gitOutput) String() string {
	w.mu.Lock()
	defer w.mu.Unlock()
	return w.buffer.String()
}

func (w *gitOutput) Write(p []byte) (int, error) {
	w.mu.Lock()
	defer w.mu.Unlock()
	const maxOutput = 1 << 20
	_, _ = w.buffer.Write(p[:min(len(p), maxOutput-w.buffer.Len())])
	return len(p), nil
}
