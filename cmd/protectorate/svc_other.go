//go:build !windows

package main

import (
	"context"
	"os"
)

func runMain(ctx context.Context, run func(context.Context) error) error { return run(ctx) }

func hideOwnConsole() {}

func processElevated() bool { return os.Geteuid() == 0 }
