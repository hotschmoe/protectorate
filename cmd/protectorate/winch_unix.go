//go:build !windows

package main

import (
	"context"
	"os"
	"os/signal"
	"syscall"
)

func resizeSignals(ctx context.Context) <-chan os.Signal {
	ch := make(chan os.Signal, 1)
	signal.Notify(ch, syscall.SIGWINCH)
	go func() { <-ctx.Done(); signal.Stop(ch) }()
	return ch
}
