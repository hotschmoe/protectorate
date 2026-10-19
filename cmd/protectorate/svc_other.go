//go:build !windows

package main

import "context"

func runMain(ctx context.Context, run func(context.Context) error) error { return run(ctx) }
