//go:build windows

package main

import (
	"context"
	"os"
)

// Windows consoles have no SIGWINCH; the initial size is sent and later
// resizes are not forwarded.
func resizeSignals(context.Context) <-chan os.Signal { return nil }
