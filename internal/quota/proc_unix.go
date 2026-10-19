//go:build unix

package quota

import (
	"os/exec"
	"syscall"
	"time"
)

// configureProc puts the child in its own process group so wrapper scripts
// (the codex npm shim) and their native child are terminated together.
func configureProc(cmd *exec.Cmd) {
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
	cmd.Cancel = func() error { return syscall.Kill(-cmd.Process.Pid, syscall.SIGTERM) }
	cmd.WaitDelay = 2 * time.Second
}

func killProc(cmd *exec.Cmd) {
	if cmd.Process != nil {
		_ = syscall.Kill(-cmd.Process.Pid, syscall.SIGKILL)
	}
}
