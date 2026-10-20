//go:build !unix

package sleeve

import (
	"os/exec"
	"time"
)

func configurePruneGit(cmd *exec.Cmd) { cmd.WaitDelay = time.Second }
