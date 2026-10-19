//go:build !unix

package driver

import (
	"os/exec"
	"time"
)

func configureProc(cmd *exec.Cmd) { cmd.WaitDelay = 2 * time.Second }

func termProc(cmd *exec.Cmd) { killProc(cmd) }

func killProc(cmd *exec.Cmd) {
	if cmd.Process != nil {
		_ = cmd.Process.Kill()
	}
}
