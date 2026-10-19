//go:build windows

package main

import (
	"context"
	"unsafe"

	"golang.org/x/sys/windows"
	"golang.org/x/sys/windows/svc"
)

var (
	pGetConsoleWindow      = windows.NewLazySystemDLL("kernel32.dll").NewProc("GetConsoleWindow")
	pGetConsoleProcessList = windows.NewLazySystemDLL("kernel32.dll").NewProc("GetConsoleProcessList")
	pShowWindow            = windows.NewLazySystemDLL("user32.dll").NewProc("ShowWindow")
)

const swHide = 0

// hideOwnConsole hides the console window a scheduled task opened for this
// process. A console shared with another process (a terminal the user typed
// into) is left alone.
func hideOwnConsole() {
	hwnd, _, _ := pGetConsoleWindow.Call()
	if hwnd == 0 {
		return
	}
	var pids [2]uint32
	if n, _, _ := pGetConsoleProcessList.Call(uintptr(unsafe.Pointer(&pids[0])), uintptr(len(pids))); n != 1 {
		return
	}
	pShowWindow.Call(hwnd, swHide)
}

func processElevated() bool { return windows.GetCurrentProcessToken().IsElevated() }

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
