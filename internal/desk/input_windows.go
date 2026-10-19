//go:build windows

package desk

import (
	"context"
	"fmt"
	"runtime"
	"syscall"
	"time"
	"unsafe"
)

var (
	pSetWindowsHookEx   = user32.NewProc("SetWindowsHookExW")
	pUnhookWindowsHook  = user32.NewProc("UnhookWindowsHookEx")
	pCallNextHookEx     = user32.NewProc("CallNextHookEx")
	pGetMessage         = user32.NewProc("GetMessageW")
	pPostThreadMessage  = user32.NewProc("PostThreadMessageW")
	pLockWorkStation    = user32.NewProc("LockWorkStation")
	pGetCurrentThreadID = kernel32.NewProc("GetCurrentThreadId")
	pGetModuleHandle    = kernel32.NewProc("GetModuleHandleW")
)

const (
	whKeyboardLL = 13
	whMouseLL    = 14
	wmQuit       = 0x0012
)

type kbdllHook struct {
	vk, scan, flags, time uint32
	extra                 uintptr
}

type msllHook struct {
	x, y  int32
	data  uint32
	flags uint32
	time  uint32
	extra uintptr
}

type winMsg struct {
	hwnd           uintptr
	message        uint32
	_              uint32
	wparam, lparam uintptr
	time           uint32
	x, y           int32
	_              uint32
	_              uint32
}

// hookTracker is read by the hook callbacks, which Windows runs on the hook
// thread; there is one monitor per process.
var hookTracker *Tracker

var keyboardHook = syscall.NewCallback(func(code, wparam, lparam uintptr) uintptr {
	if int32(code) >= 0 && hookTracker != nil {
		h := *(**kbdllHook)(unsafe.Pointer(&lparam))
		hookTracker.Event(keyInjected(h.flags))
	}
	r, _, _ := pCallNextHookEx.Call(0, code, wparam, lparam)
	return r
})

var mouseHook = syscall.NewCallback(func(code, wparam, lparam uintptr) uintptr {
	if int32(code) >= 0 && hookTracker != nil {
		h := *(**msllHook)(unsafe.Pointer(&lparam))
		hookTracker.Event(mouseInjected(h.flags))
	}
	r, _, _ := pCallNextHookEx.Call(0, code, wparam, lparam)
	return r
})

// StartInputMonitor installs low-level keyboard and mouse hooks on a locked OS
// thread with its own message loop and tracks the last physical input until
// ctx is done. It must run in the interactive session; hooks installed from
// session 0 would see nothing. Call it at most once per process.
func StartInputMonitor(ctx context.Context, now func() time.Time) (*Tracker, error) {
	t := NewTracker(now)
	ready := make(chan error, 1)
	done := make(chan struct{})
	go func() {
		defer close(done)
		runtime.LockOSThread()
		defer runtime.UnlockOSThread()
		hookTracker = t
		mod, _, _ := pGetModuleHandle.Call(0)
		kb, _, e := pSetWindowsHookEx.Call(whKeyboardLL, keyboardHook, mod, 0)
		if kb == 0 {
			ready <- fmt.Errorf("install keyboard hook: %w", e)
			return
		}
		defer pUnhookWindowsHook.Call(kb)
		ms, _, e := pSetWindowsHookEx.Call(whMouseLL, mouseHook, mod, 0)
		if ms == 0 {
			ready <- fmt.Errorf("install mouse hook: %w", e)
			return
		}
		defer pUnhookWindowsHook.Call(ms)
		tid, _, _ := pGetCurrentThreadID.Call()
		go func() {
			<-ctx.Done()
			pPostThreadMessage.Call(tid, wmQuit, 0, 0)
		}()
		ready <- nil
		var m winMsg
		for {
			if r, _, _ := pGetMessage.Call(uintptr(unsafe.Pointer(&m)), 0, 0, 0); int32(r) <= 0 {
				return
			}
		}
	}()
	select {
	case err := <-ready:
		if err != nil {
			<-done
			return nil, err
		}
	case <-ctx.Done():
		return nil, ctx.Err()
	}
	return t, nil
}

// LockWorkstation locks the interactive session, like Win+L.
func LockWorkstation() error {
	if r, _, e := pLockWorkStation.Call(); r == 0 {
		return fmt.Errorf("LockWorkStation: %w", e)
	}
	return nil
}
