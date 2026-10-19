//go:build !linux && !windows

package node

func newAccelProbe() accelProbe { return nil }
