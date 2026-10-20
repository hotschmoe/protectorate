package svcgen

import (
	"fmt"
	"os"
	"testing"
)

func TestMain(m *testing.M) {
	runtimeDir, err := os.MkdirTemp("", "protectorate-systemd-verify-")
	if err != nil {
		fmt.Fprintf(os.Stderr, "create systemd verification runtime: %v\n", err)
		os.Exit(1)
	}
	if err := os.Setenv("XDG_RUNTIME_DIR", runtimeDir); err != nil {
		fmt.Fprintf(os.Stderr, "isolate systemd verification runtime: %v\n", err)
		os.Exit(1)
	}
	code := m.Run()
	if err := os.RemoveAll(runtimeDir); err != nil {
		fmt.Fprintf(os.Stderr, "remove private systemd verification runtime: %v\n", err)
		code = 1
	}
	os.Exit(code)
}
