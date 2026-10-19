package tailnet

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sync"
	"syscall"
	"time"

	"github.com/hotschmoe/protectorate/internal/fsutil"
)

// Supervisor runs headscale as a child process and restarts it with backoff.
type Supervisor struct {
	Home string
	Cfg  Config
	// Logf receives lifecycle events; headscale's own output goes to
	// tailnet/headscale.log.
	Logf func(format string, args ...any)

	// StopTimeout is how long a SIGTERMed headscale gets before SIGKILL.
	StopTimeout time.Duration
}

const (
	backoffMin = time.Second
	backoffMax = 30 * time.Second
	// A run this long counts as healthy and resets the backoff.
	healthyRun = time.Minute

	logMaxSize = 10 << 20
	logKeep    = 3
)

func (s *Supervisor) logf(format string, args ...any) {
	if s.Logf != nil {
		s.Logf(format, args...)
	}
}

// LogPath is where headscale's stdout and stderr are appended.
func LogPath(home string) string { return filepath.Join(Dir(home), "headscale.log") }

// Run regenerates headscale's config and policy, then keeps headscale alive
// until ctx is cancelled, at which point it stops it cleanly. It returns an
// error only for setup failures.
func (s *Supervisor) Run(ctx context.Context) error {
	bin := BinaryPath(s.Home, HeadscaleVersion)
	if _, err := os.Stat(bin); err != nil {
		return fmt.Errorf("headscale binary: %w (run `protectorate hub tailnet install`)", err)
	}
	if err := s.Cfg.WriteFiles(s.Home); err != nil {
		return err
	}
	logFile, err := openRotatingLog(LogPath(s.Home), logMaxSize, logKeep)
	if err != nil {
		return fmt.Errorf("open headscale log: %w", err)
	}
	defer logFile.Close()

	stop := s.StopTimeout
	if stop == 0 {
		stop = 10 * time.Second
	}
	cfgPath := filepath.Join(Dir(s.Home), "headscale.yaml")
	backoff := backoffMin
	for ctx.Err() == nil {
		cmd := exec.CommandContext(ctx, bin, "serve", "-c", cfgPath)
		cmd.Stdout, cmd.Stderr = logFile, logFile
		cmd.Cancel = func() error { return cmd.Process.Signal(syscall.SIGTERM) }
		cmd.WaitDelay = stop
		start := time.Now()
		err := cmd.Start()
		if err == nil {
			s.logf("headscale started (pid %d)", cmd.Process.Pid)
			err = cmd.Wait()
		}
		if ctx.Err() != nil {
			s.logf("headscale stopped")
			return nil
		}
		if time.Since(start) >= healthyRun {
			backoff = backoffMin
		}
		s.logf("headscale exited (%v); restarting in %s", err, backoff)
		select {
		case <-ctx.Done():
			return nil
		case <-time.After(backoff):
		}
		backoff = min(backoff*2, backoffMax)
	}
	return nil
}

// rotatingLog appends to path and, once a write would push it past max bytes,
// shifts path -> path.1 -> ... -> path.<keep>. Rotation only renames: the
// oldest rotated file is replaced by rename and no file is ever unlinked, so
// at most keep+1 files exist and only files this writer created are touched.
type rotatingLog struct {
	path string
	max  int64
	keep int

	mu   sync.Mutex
	f    *os.File
	size int64
}

func openRotatingLog(path string, max int64, keep int) (*rotatingLog, error) {
	l := &rotatingLog{path: path, max: max, keep: keep}
	if err := l.open(); err != nil {
		return nil, err
	}
	return l, nil
}

func (l *rotatingLog) open() error {
	f, err := os.OpenFile(l.path, os.O_WRONLY|os.O_CREATE|os.O_APPEND, 0o600)
	if err != nil {
		return err
	}
	fi, err := f.Stat()
	if err != nil {
		f.Close()
		return err
	}
	l.f, l.size = f, fi.Size()
	return nil
}

func (l *rotatingLog) Write(p []byte) (int, error) {
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.size > 0 && l.size+int64(len(p)) > l.max {
		if err := l.rotate(); err != nil {
			return 0, err
		}
	}
	n, err := l.f.Write(p)
	l.size += int64(n)
	return n, err
}

func (l *rotatingLog) rotate() error {
	if err := l.f.Close(); err != nil {
		return err
	}
	for i := l.keep; i >= 1; i-- {
		from := l.path
		if i > 1 {
			from = fmt.Sprintf("%s.%d", l.path, i-1)
		}
		if err := fsutil.Rename(from, fmt.Sprintf("%s.%d", l.path, i)); err != nil && !errors.Is(err, os.ErrNotExist) {
			return fmt.Errorf("rotate headscale log: %w", err)
		}
	}
	return l.open()
}

func (l *rotatingLog) Close() error {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.f.Close()
}
