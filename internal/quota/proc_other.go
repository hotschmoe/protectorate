//go:build !unix

package quota

import (
	"os/exec"
	"time"
)

func configureProc(cmd *exec.Cmd) { cmd.WaitDelay = 2 * time.Second }

func killProc(cmd *exec.Cmd) {
	if cmd.Process != nil {
		_ = cmd.Process.Kill()
	}
}
