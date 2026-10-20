package sleeve

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"time"
)

const scopePollInterval = 10 * time.Millisecond

func (c Config) scopeDir(name string) string {
	return filepath.Join(c.CgroupRoot, c.UnitName(name)+".scope")
}

func (c Config) scopeAlive(name string) bool {
	_, err := os.Stat(c.scopeDir(name))
	return err == nil
}

// scopeRunning is true once the scope holds a process; the cgroup appears
// slightly before systemd-run moves itself in.
func (c Config) scopeRunning(name string) bool {
	b, err := os.ReadFile(filepath.Join(c.scopeDir(name), "cgroup.procs"))
	return err == nil && len(bytes.TrimSpace(b)) > 0
}

// scopeNames lists the sleeve scopes currently in the cgroup tree.
func (c Config) scopeNames() []string {
	ents, _ := os.ReadDir(c.CgroupRoot)
	var out []string
	for _, e := range ents {
		if n, ok := strings.CutPrefix(e.Name(), c.UnitPrefix); ok {
			if n, ok = strings.CutSuffix(n, ".scope"); ok {
				out = append(out, n)
			}
		}
	}
	return out
}

// Preflight reports why sleeves cannot run on this host, or nil.
func (c Config) Preflight() error {
	if runtime.GOOS != "linux" {
		return ErrUnsupported
	}
	for _, b := range []string{"bwrap", "systemd-run", "systemctl"} {
		if _, err := exec.LookPath(b); err != nil {
			return fmt.Errorf("%w: %s not found in PATH", ErrUnavailable, b)
		}
	}
	if fi, err := os.Stat(filepath.Dir(c.CgroupRoot)); err != nil || !fi.IsDir() {
		return fmt.Errorf("%w: no systemd user manager cgroup at %s", ErrUnavailable, filepath.Dir(c.CgroupRoot))
	}
	return nil
}

type rawUsage struct {
	cpuUsec, memCurrent, memPeak, pids, oomKills uint64
	peakKnown                                    bool
}

func readUint(path string) (uint64, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return 0, err
	}
	s := strings.TrimSpace(string(b))
	if s == "max" {
		return 0, nil
	}
	return strconv.ParseUint(s, 10, 64)
}

func readKeyed(path, key string) (uint64, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return 0, err
	}
	for _, line := range strings.Split(string(b), "\n") {
		if k, v, ok := strings.Cut(line, " "); ok && k == key {
			return strconv.ParseUint(strings.TrimSpace(v), 10, 64)
		}
	}
	return 0, fmt.Errorf("%s: no %s", path, key)
}

func (c Config) readUsage(name string) (rawUsage, error) {
	d := c.scopeDir(name)
	var u rawUsage
	var err error
	if u.cpuUsec, err = readKeyed(filepath.Join(d, "cpu.stat"), "usage_usec"); err != nil {
		return u, err
	}
	if u.memCurrent, err = readUint(filepath.Join(d, "memory.current")); err != nil {
		return u, err
	}
	if p, err := readUint(filepath.Join(d, "memory.peak")); err == nil {
		u.memPeak, u.peakKnown = p, true
	}
	if u.pids, err = readUint(filepath.Join(d, "pids.current")); err != nil {
		return u, err
	}
	u.oomKills, _ = readKeyed(filepath.Join(d, "memory.events"), "oom_kill")
	return u, nil
}

func (c Config) readOOMKills(name string) (uint64, bool) {
	n, err := readKeyed(filepath.Join(c.scopeDir(name), "memory.events"), "oom_kill")
	return n, err == nil
}

// killScope sends SIGKILL to every process in the scope and waits for the
// cgroup to disappear.
func (c Config) killScope(ctx context.Context, name string) error {
	if !c.scopeAlive(name) {
		return nil
	}
	unit := c.UnitName(name) + ".scope"
	for _, args := range [][]string{
		{"--user", "kill", "--signal=SIGKILL", unit},
		{"--user", "stop", unit},
	} {
		var stderr bytes.Buffer
		cmd := exec.CommandContext(ctx, "systemctl", args...)
		cmd.Stderr = &stderr
		if err := cmd.Run(); err != nil && c.scopeAlive(name) {
			return fmt.Errorf("systemctl %s: %s: %w", args[1], strings.TrimSpace(stderr.String()), err)
		}
		if c.waitGone(ctx, name, 5*time.Second) {
			return nil
		}
	}
	return fmt.Errorf("scope %s is still alive after kill", unit)
}

func (c Config) waitGone(ctx context.Context, name string, d time.Duration) bool {
	deadline := time.Now().Add(d)
	for c.scopeAlive(name) {
		if time.Now().After(deadline) || ctx.Err() != nil {
			return false
		}
		select {
		case <-ctx.Done():
			return false
		case <-time.After(scopePollInterval):
		}
	}
	return true
}

// dirSize sums regular file sizes under dir without following symlinks.
func dirSize(ctx context.Context, dir string) (uint64, error) {
	var total uint64
	err := filepath.WalkDir(dir, func(_ string, d fs.DirEntry, err error) error {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if errors.Is(err, os.ErrNotExist) {
			return nil
		}
		if err != nil {
			return err
		}
		if !d.Type().IsRegular() {
			return nil
		}
		fi, err := d.Info()
		if errors.Is(err, os.ErrNotExist) {
			return nil
		}
		if err != nil {
			return err
		}
		total += uint64(fi.Size())
		return nil
	})
	return total, err
}
