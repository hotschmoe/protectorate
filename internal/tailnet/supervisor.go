package tailnet

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"syscall"
	"time"
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
	logFile, err := os.OpenFile(LogPath(s.Home), os.O_WRONLY|os.O_CREATE|os.O_APPEND, 0o600)
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
