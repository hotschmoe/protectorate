//go:build windows

package desk

import (
	"errors"
	"fmt"
	"image"
	"os/exec"
	"sync"
	"syscall"
	"unicode/utf16"
	"unsafe"

	"golang.org/x/sys/windows"
)

var (
	user32   = windows.NewLazySystemDLL("user32.dll")
	gdi32    = windows.NewLazySystemDLL("gdi32.dll")
	kernel32 = windows.NewLazySystemDLL("kernel32.dll")

	pSetProcessDPIAware       = user32.NewProc("SetProcessDPIAware")
	pGetSystemMetrics         = user32.NewProc("GetSystemMetrics")
	pGetDC                    = user32.NewProc("GetDC")
	pReleaseDC                = user32.NewProc("ReleaseDC")
	pSendInput                = user32.NewProc("SendInput")
	pSetCursorPos             = user32.NewProc("SetCursorPos")
	pEnumWindows              = user32.NewProc("EnumWindows")
	pGetWindowTextW           = user32.NewProc("GetWindowTextW")
	pGetWindowTextLengthW     = user32.NewProc("GetWindowTextLengthW")
	pGetWindowRect            = user32.NewProc("GetWindowRect")
	pIsWindowVisible          = user32.NewProc("IsWindowVisible")
	pIsIconic                 = user32.NewProc("IsIconic")
	pGetWindowThreadProcessID = user32.NewProc("GetWindowThreadProcessId")
	pGetProcessWindowStation  = user32.NewProc("GetProcessWindowStation")
	pGetUserObjectInformation = user32.NewProc("GetUserObjectInformationW")
	pOpenInputDesktop         = user32.NewProc("OpenInputDesktop")
	pCloseDesktop             = user32.NewProc("CloseDesktop")

	pCreateCompatibleDC     = gdi32.NewProc("CreateCompatibleDC")
	pCreateCompatibleBitmap = gdi32.NewProc("CreateCompatibleBitmap")
	pSelectObject           = gdi32.NewProc("SelectObject")
	pBitBlt                 = gdi32.NewProc("BitBlt")
	pGetDIBits              = gdi32.NewProc("GetDIBits")
	pDeleteObject           = gdi32.NewProc("DeleteObject")
	pDeleteDC               = gdi32.NewProc("DeleteDC")

	pProcessIDToSessionID = kernel32.NewProc("ProcessIdToSessionId")
)

const (
	smXVirtualScreen  = 76
	smYVirtualScreen  = 77
	smCXVirtualScreen = 78
	smCYVirtualScreen = 79

	srcCopy    = 0x00CC0020
	captureBlt = 0x40000000

	inputMouse    = 0
	inputKeyboard = 1

	mouseLeftDown   = 0x0002
	mouseLeftUp     = 0x0004
	mouseRightDown  = 0x0008
	mouseRightUp    = 0x0010
	mouseMiddleDown = 0x0020
	mouseMiddleUp   = 0x0040

	keyExtended = 0x0001
	keyUp       = 0x0002
	keyUnicode  = 0x0004

	uoiName = 2
)

// The INPUT union is 32 bytes on the 64-bit targets, 40 with its header.
type mouseInput struct {
	typ       uint32
	_         uint32
	dx, dy    int32
	mouseData uint32
	flags     uint32
	time      uint32
	extra     uintptr
}

type keyInput struct {
	typ   uint32
	_     uint32
	vk    uint16
	scan  uint16
	flags uint32
	time  uint32
	extra uintptr
	_     [8]byte
}

var (
	_ [40]byte = [unsafe.Sizeof(mouseInput{})]byte{}
	_ [40]byte = [unsafe.Sizeof(keyInput{})]byte{}
)

type bitmapInfoHeader struct {
	size          uint32
	width, height int32
	planes, bits  uint16
	compression   uint32
	sizeImage     uint32
	xppm, yppm    int32
	used, imp     uint32
}

type rect struct{ left, top, right, bottom int32 }

// New returns the Windows desktop driver. It never fails: Ready reports
// whether this process can actually reach a desktop.
func New() Desktop {
	pSetProcessDPIAware.Call()
	return &winDesk{}
}

type winDesk struct{}

func (winDesk) Ready() error {
	var s Session
	var perr error
	if r, _, e := pProcessIDToSessionID.Call(uintptr(windows.GetCurrentProcessId()), uintptr(unsafe.Pointer(&s.ID))); r == 0 {
		perr = e
	} else if s.WindowStation, perr = windowStation(); perr != nil {
		perr = fmt.Errorf("window station: %w", perr)
	}
	return CheckInteractive(s, perr)
}

func windowStation() (string, error) {
	h, _, e := pGetProcessWindowStation.Call()
	if h == 0 {
		return "", e
	}
	var buf [256]uint16
	var n uint32
	if r, _, e := pGetUserObjectInformation.Call(h, uoiName, uintptr(unsafe.Pointer(&buf[0])), uintptr(len(buf)*2), uintptr(unsafe.Pointer(&n))); r == 0 {
		return "", e
	}
	return windows.UTF16ToString(buf[:]), nil
}

func metric(i uintptr) int {
	r, _, _ := pGetSystemMetrics.Call(i)
	return int(int32(r))
}

func origin() (int, int) { return metric(smXVirtualScreen), metric(smYVirtualScreen) }

func (winDesk) Screenshot() (*image.RGBA, error) {
	x0, y0 := origin()
	w, h := metric(smCXVirtualScreen), metric(smCYVirtualScreen)
	if w <= 0 || h <= 0 {
		return nil, fmt.Errorf("%w: screen has no size", ErrNotInteractive)
	}
	screen, _, e := pGetDC.Call(0)
	if screen == 0 {
		return nil, fmt.Errorf("GetDC: %w", e)
	}
	defer pReleaseDC.Call(0, screen)
	mem, _, e := pCreateCompatibleDC.Call(screen)
	if mem == 0 {
		return nil, fmt.Errorf("CreateCompatibleDC: %w", e)
	}
	defer pDeleteDC.Call(mem)
	bmp, _, e := pCreateCompatibleBitmap.Call(screen, uintptr(w), uintptr(h))
	if bmp == 0 {
		return nil, fmt.Errorf("CreateCompatibleBitmap: %w", e)
	}
	defer pDeleteObject.Call(bmp)
	old, _, _ := pSelectObject.Call(mem, bmp)
	defer pSelectObject.Call(mem, old)
	if r, _, e := pBitBlt.Call(mem, 0, 0, uintptr(w), uintptr(h), screen, uintptr(x0), uintptr(y0), srcCopy|captureBlt); r == 0 {
		return nil, fmt.Errorf("BitBlt: %w", e)
	}
	hdr := bitmapInfoHeader{size: uint32(unsafe.Sizeof(bitmapInfoHeader{})), width: int32(w), height: -int32(h), planes: 1, bits: 32}
	img := image.NewRGBA(image.Rect(0, 0, w, h))
	if r, _, e := pGetDIBits.Call(mem, bmp, 0, uintptr(h), uintptr(unsafe.Pointer(&img.Pix[0])), uintptr(unsafe.Pointer(&hdr)), 0); r == 0 {
		return nil, fmt.Errorf("GetDIBits: %w", e)
	}
	for i := 0; i+3 < len(img.Pix); i += 4 {
		img.Pix[i], img.Pix[i+2] = img.Pix[i+2], img.Pix[i]
		img.Pix[i+3] = 0xff
	}
	return img, nil
}

var (
	enumMu      sync.Mutex
	enumResults *[]Window
)

var enumProc = syscall.NewCallback(func(hwnd, _ uintptr) uintptr {
	if v, _, _ := pIsWindowVisible.Call(hwnd); v == 0 {
		return 1
	}
	n, _, _ := pGetWindowTextLengthW.Call(hwnd)
	if n == 0 {
		return 1
	}
	buf := make([]uint16, n+1)
	pGetWindowTextW.Call(hwnd, uintptr(unsafe.Pointer(&buf[0])), uintptr(len(buf)))
	var r rect
	if ok, _, _ := pGetWindowRect.Call(hwnd, uintptr(unsafe.Pointer(&r))); ok == 0 {
		return 1
	}
	var pid uint32
	pGetWindowThreadProcessID.Call(hwnd, uintptr(unsafe.Pointer(&pid)))
	iconic, _, _ := pIsIconic.Call(hwnd)
	x0, y0 := origin()
	*enumResults = append(*enumResults, Window{
		Title: windows.UTF16ToString(buf), X: int(r.left) - x0, Y: int(r.top) - y0,
		Width: int(r.right - r.left), Height: int(r.bottom - r.top), PID: pid, Minimized: iconic != 0,
	})
	return 1
})

func (winDesk) Windows() ([]Window, error) {
	enumMu.Lock()
	defer enumMu.Unlock()
	var out []Window
	enumResults = &out
	defer func() { enumResults = nil }()
	if r, _, e := pEnumWindows.Call(enumProc, 0); r == 0 {
		return nil, fmt.Errorf("EnumWindows: %w", e)
	}
	return out, nil
}

func sendInputs(first unsafe.Pointer, n int, size uintptr) error {
	if r, _, e := pSendInput.Call(uintptr(n), uintptr(first), size); int(r) != n {
		return fmt.Errorf("SendInput delivered %d of %d events (the foreground window may belong to a higher-privileged program): %v", r, n, e)
	}
	return nil
}

func mouse(flags uint32) error {
	in := mouseInput{typ: inputMouse, flags: flags}
	return sendInputs(unsafe.Pointer(&in), 1, unsafe.Sizeof(in))
}

func keyEvents(events []keyInput) error {
	if len(events) == 0 {
		return nil
	}
	return sendInputs(unsafe.Pointer(&events[0]), len(events), unsafe.Sizeof(events[0]))
}

func (winDesk) Move(x, y int) error {
	x0, y0 := origin()
	if r, _, e := pSetCursorPos.Call(uintptr(x+x0), uintptr(y+y0)); r == 0 {
		return fmt.Errorf("SetCursorPos: %w", e)
	}
	return nil
}

func (d winDesk) Click(x, y int, b Button, double bool) error {
	if err := d.Move(x, y); err != nil {
		return err
	}
	down, up := uint32(mouseLeftDown), uint32(mouseLeftUp)
	switch b {
	case Right:
		down, up = mouseRightDown, mouseRightUp
	case Middle:
		down, up = mouseMiddleDown, mouseMiddleUp
	}
	for i := 0; i < 1+btoi(double); i++ {
		if err := mouse(down); err != nil {
			return err
		}
		if err := mouse(up); err != nil {
			return err
		}
	}
	return nil
}

func btoi(b bool) int {
	if b {
		return 1
	}
	return 0
}

func vkEvent(k Key, up bool) keyInput {
	e := keyInput{typ: inputKeyboard, vk: k.VK}
	if k.Extended {
		e.flags |= keyExtended
	}
	if up {
		e.flags |= keyUp
	}
	return e
}

func (winDesk) Key(c Chord) error {
	var ev []keyInput
	for _, m := range c.Modifiers {
		ev = append(ev, vkEvent(m, false))
	}
	ev = append(ev, vkEvent(c.Key, false), vkEvent(c.Key, true))
	for i := len(c.Modifiers) - 1; i >= 0; i-- {
		ev = append(ev, vkEvent(c.Modifiers[i], true))
	}
	return keyEvents(ev)
}

func (winDesk) Type(text string) error {
	var ev []keyInput
	for _, r := range text {
		switch r {
		case '\r':
			continue
		case '\n':
			k := Key{VK: 0x0D}
			ev = append(ev, vkEvent(k, false), vkEvent(k, true))
			continue
		case '\t':
			k := Key{VK: 0x09}
			ev = append(ev, vkEvent(k, false), vkEvent(k, true))
			continue
		}
		for _, u := range utf16.Encode([]rune{r}) {
			ev = append(ev,
				keyInput{typ: inputKeyboard, scan: u, flags: keyUnicode},
				keyInput{typ: inputKeyboard, scan: u, flags: keyUnicode | keyUp})
		}
	}
	for len(ev) > 0 {
		n := min(len(ev), 256)
		if err := keyEvents(ev[:n]); err != nil {
			return err
		}
		ev = ev[n:]
	}
	return nil
}

func (winDesk) Run(program string, args []string) (uint32, error) {
	if program == "" {
		return 0, fmt.Errorf("%w: program is empty", ErrInvalid)
	}
	cmd := exec.Command(program, args...)
	cmd.SysProcAttr = &syscall.SysProcAttr{CreationFlags: windows.CREATE_NEW_PROCESS_GROUP}
	if err := cmd.Start(); err != nil {
		return 0, fmt.Errorf("start %s: %w", program, err)
	}
	pid := uint32(cmd.Process.Pid)
	go func() { _ = cmd.Wait() }()
	return pid, nil
}

const desktopReadObjects = 0x0001

// InputDesktop returns the name of the desktop that currently receives
// input, or ErrInputDesktopDenied when it cannot be opened (locked screen).
func InputDesktop() (string, error) {
	h, _, e := pOpenInputDesktop.Call(0, 0, desktopReadObjects)
	if h == 0 {
		if errors.Is(e, windows.ERROR_ACCESS_DENIED) {
			return "", ErrInputDesktopDenied
		}
		return "", fmt.Errorf("open input desktop: %w", e)
	}
	defer pCloseDesktop.Call(h)
	var buf [256]uint16
	var n uint32
	if r, _, e := pGetUserObjectInformation.Call(h, uoiName, uintptr(unsafe.Pointer(&buf[0])), uintptr(len(buf)*2), uintptr(unsafe.Pointer(&n))); r == 0 {
		return "", fmt.Errorf("name input desktop: %w", e)
	}
	return windows.UTF16ToString(buf[:]), nil
}
