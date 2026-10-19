package sleeve

import "syscall"

const syscallSignal0 = syscall.Signal(0)

func syscallKill0(pid int) error { return syscall.Kill(pid, 0) }
