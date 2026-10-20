//go:build !windows

package main

import (
	"fmt"
	"os"
	"syscall"
)

func reexecHub(exe string, args []string) error {
	if err := syscall.Exec(exe, append([]string{exe}, args...), os.Environ()); err != nil {
		return fmt.Errorf("re-exec hub: %w", err)
	}
	return nil
}
