//go:build !windows

package desk

import (
	"context"
	"time"
)

// StartInputMonitor reports ErrUnsupported: only Windows is implemented.
func StartInputMonitor(context.Context, func() time.Time) (*Tracker, error) {
	return nil, ErrUnsupported
}

// LockWorkstation reports ErrUnsupported: only Windows is implemented.
func LockWorkstation() error { return ErrUnsupported }
