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
	"github.com/hotschmoe/protectorate/internal/headscaledb"
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
	bin, logFile, err := s.prepare()
	if err != nil {
		return err
	}
	defer logFile.Close()

	backoff := backoffMin
	for ctx.Err() == nil {
		requested, err := headscaledb.Requested(s.Home)
		if err != nil {
			return fmt.Errorf("check backup request: %w", err)
		}
		if requested {
			select {
			case <-ctx.Done():
				return nil
			case <-time.After(100 * time.Millisecond):
			}
			continue
		}
		gate, err := headscaledb.Lock(ctx, s.Home, headscaledb.Database)
		if err != nil {
			if ctx.Err() != nil {
				return nil
			}
			return err
		}
		start, paused, err := s.runHeadscale(ctx, bin, logFile)
		gate.Close()
		if ctx.Err() != nil {
			s.logf("headscale stopped")
			return nil
		}
		if paused {
			s.logf("headscale stopped for backup")
			continue
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

func (s *Supervisor) prepare() (string, *rotatingLog, error) {
	bin := BinaryPath(s.Home, HeadscaleVersion)
	if _, err := os.Stat(bin); err != nil {
		return "", nil, fmt.Errorf("headscale binary: %w (run `protectorate hub tailnet install`)", err)
	}
	if err := s.Cfg.WriteFiles(s.Home); err != nil {
		return "", nil, err
	}
	f, err := openRotatingLog(LogPath(s.Home), logMaxSize, logKeep)
	if err != nil {
		return "", nil, fmt.Errorf("open headscale log: %w", err)
	}
	return bin, f, nil
}

func (s *Supervisor) runHeadscale(ctx context.Context, bin string, logFile *rotatingLog) (time.Time, bool, error) {
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	cmd := exec.CommandContext(ctx, bin, "serve", "-c", filepath.Join(Dir(s.Home), "headscale.yaml"))
	cmd.Stdout, cmd.Stderr = logFile, logFile
	cmd.Cancel = func() error { return cmd.Process.Signal(syscall.SIGTERM) }
	cmd.WaitDelay = s.StopTimeout
	if cmd.WaitDelay == 0 {
		cmd.WaitDelay = 10 * time.Second
	}
	start := time.Now()
	if err := cmd.Start(); err != nil {
		return start, false, err
	}
	s.logf("headscale started (pid %d)", cmd.Process.Pid)
	done := make(chan error, 1)
	go func() { done <- cmd.Wait() }()
	tick := time.NewTicker(100 * time.Millisecond)
	defer tick.Stop()
	for {
		select {
		case err := <-done:
			return start, false, err
		case <-ctx.Done():
			return start, false, <-done
		case <-tick.C:
			requested, err := headscaledb.Requested(s.Home)
			if err != nil {
				s.logf("check backup request: %v", err)
			}
			if requested {
				cancel()
				return start, true, <-done
			}
		}
	}
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
