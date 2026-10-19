//go:build !windows

package node

import (
	"bytes"
	"context"
	"fmt"
	"os"
	"os/exec"
	"strings"
	"sync"
	"syscall"
	"time"
)

// procTree is a started command and everything it spawned: a process group.
type procTree struct{ pid int }

func startTree(cmd *exec.Cmd) (*procTree, error) {
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
	if err := cmd.Start(); err != nil {
		return nil, err
	}
	return &procTree{pid: cmd.Process.Pid}, nil
}

// interrupt asks the tree to stop politely and reports whether it could.
func (t *procTree) interrupt() bool { return syscall.Kill(-t.pid, syscall.SIGTERM) == nil }

func (t *procTree) kill() { _ = syscall.Kill(-t.pid, syscall.SIGKILL) }

func (t *procTree) close() {}

// StartDetached starts exe in its own session with no stdio, so it outlives
// this process. It does not wait.
func StartDetached(exe string, args []string) error {
	cmd := exec.Command(exe, args...)
	cmd.SysProcAttr = &syscall.SysProcAttr{Setsid: true}
	if err := cmd.Start(); err != nil {
		return err
	}
	return cmd.Process.Release()
}

var (
	adminOnce          sync.Once
	adminKind, adminNo string
)

// probeAdmin finds how admin jobs elevate: the agent is root, or sudo works
// without a password (`sudo -n true`, run once per process; it shows in the
// auth log).
func probeAdmin(note string) (admin, outNote string) {
	adminOnce.Do(func() { adminKind, adminNo = elevation() })
	if adminKind == "" {
		return "", joinNote(note, adminNo)
	}
	return adminKind, note
}

func elevation() (admin, why string) {
	if os.Geteuid() == 0 {
		return "root", ""
	}
	if _, err := exec.LookPath("sudo"); err != nil {
		return "", "no admin jobs: sudo is not installed"
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, "sudo", "-n", "true")
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		why := strings.TrimSpace(stderr.String())
		if why == "" {
			why = err.Error()
		}
		return "", "no admin jobs: sudo -n fails (" + why + ")"
	}
	return "sudo", ""
}

func joinNote(a, b string) string {
	if a == "" {
		return b
	}
	return a + "; " + b
}

// adminArgv elevates command. sudo keeps only the job's variables, by name,
// so no value appears in the process list.
func adminArgv(admin string, command, envKeys []string) ([]string, error) {
	switch admin {
	case "root":
		return command, nil
	case "sudo":
		return append([]string{"sudo", "-n", "--preserve-env=" + strings.Join(envKeys, ","), "--"}, command...), nil
	}
	return nil, fmt.Errorf("unknown elevation %q", admin)
}
