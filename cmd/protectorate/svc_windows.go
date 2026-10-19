//go:build windows

package main

import (
	"context"

	"golang.org/x/sys/windows/svc"
)

// runMain runs under the service control manager when started as a Windows
// service, otherwise directly. Stop and shutdown requests cancel the context,
// which `node serve` already treats like SIGTERM.
func runMain(ctx context.Context, run func(context.Context) error) error {
	isSvc, err := svc.IsWindowsService()
	if err != nil || !isSvc {
		return run(ctx)
	}
	return svc.Run("", &winService{ctx: ctx, run: run})
}

type winService struct {
	ctx context.Context
	run func(context.Context) error
}

func (w *winService) Execute(_ []string, req <-chan svc.ChangeRequest, status chan<- svc.Status) (bool, uint32) {
	ctx, cancel := context.WithCancel(w.ctx)
	defer cancel()
	done := make(chan error, 1)
	go func() { done <- w.run(ctx) }()

	status <- svc.Status{State: svc.StartPending}
	const accepted = svc.AcceptStop | svc.AcceptShutdown
	status <- svc.Status{State: svc.Running, Accepts: accepted}
	for {
		select {
		case err := <-done:
			if err != nil {
				return false, 1
			}
			return false, 0
		case c := <-req:
			switch c.Cmd {
			case svc.Interrogate:
				status <- c.CurrentStatus
			case svc.Stop, svc.Shutdown:
				status <- svc.Status{State: svc.StopPending}
				cancel()
				<-done
				return false, 0
			}
		}
	}
}
