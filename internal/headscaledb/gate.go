// Package headscaledb coordinates database copies with the headscale supervisor.
package headscaledb

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"time"
)

const (
	Request  = "backup-request.lock"
	Database = "database.lock"
)

func Try(home, name string) (*os.File, error) {
	dir := filepath.Join(home, "tailnet")
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return nil, fmt.Errorf("create headscale lock directory: %w", err)
	}
	f, err := os.OpenFile(filepath.Join(dir, name), os.O_CREATE|os.O_RDWR, 0o600)
	if err != nil {
		return nil, fmt.Errorf("open headscale lock: %w", err)
	}
	ok, err := tryLock(f)
	if err != nil || !ok {
		f.Close()
		return nil, err
	}
	return f, nil
}

func Lock(ctx context.Context, home, name string) (*os.File, error) {
	for {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		f, err := Try(home, name)
		if err != nil {
			return nil, fmt.Errorf("lock headscale %s: %w", name, err)
		}
		if f != nil {
			return f, nil
		}
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-time.After(100 * time.Millisecond):
		}
	}
}

// Requested reports whether a backup holds the request lock. Locks are released
// by closing their files, including automatically on process exit.
func Requested(home string) (bool, error) {
	f, err := Try(home, Request)
	if err != nil {
		return false, err
	}
	if f == nil {
		return true, nil
	}
	f.Close()
	return false, nil
}
