//go:build !windows

package node

import (
	"os/exec"
	"syscall"
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
