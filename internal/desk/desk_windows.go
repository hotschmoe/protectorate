//go:build windows

package desk

import (
	"errors"
	"fmt"
	"image"
	"os/exec"
	"strings"
	"sync"
	"syscall"
	"time"
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
	pGetForegroundWindow      = user32.NewProc("GetForegroundWindow")
	pWindowFromPoint          = user32.NewProc("WindowFromPoint")
	pGetAncestor              = user32.NewProc("GetAncestor")
	pGetCursorPos             = user32.NewProc("GetCursorPos")
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

	inputMouse = 0

	mouseLeftDown   = 0x0002
	mouseLeftUp     = 0x0004
	mouseRightDown  = 0x0008
	mouseRightUp    = 0x0010
	mouseMiddleDown = 0x0020
	mouseMiddleUp   = 0x0040

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

type point struct{ x, y int32 }

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

func windowTitle(hwnd uintptr) string {
	n, _, _ := pGetWindowTextLengthW.Call(hwnd)
	if n == 0 {
		return ""
	}
	buf := make([]uint16, n+1)
	pGetWindowTextW.Call(hwnd, uintptr(unsafe.Pointer(&buf[0])), uintptr(len(buf)))
	return windows.UTF16ToString(buf)
}

var enumProc = syscall.NewCallback(func(hwnd, _ uintptr) uintptr {
	if v, _, _ := pIsWindowVisible.Call(hwnd); v == 0 {
		return 1
	}
	title := windowTitle(hwnd)
	if title == "" {
		return 1
	}
	var r rect
	if ok, _, _ := pGetWindowRect.Call(hwnd, uintptr(unsafe.Pointer(&r))); ok == 0 {
		return 1
	}
	var pid uint32
	pGetWindowThreadProcessID.Call(hwnd, uintptr(unsafe.Pointer(&pid)))
	iconic, _, _ := pIsIconic.Call(hwnd)
	x0, y0 := origin()
	*enumResults = append(*enumResults, Window{
		Title: title, X: int(r.left) - x0, Y: int(r.top) - y0,
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
		return fmt.Errorf("SendInput delivered %d of %d events (the foreground window may belong to a higher-privileged program): %w", r, n, e)
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
	w, h := metric(smCXVirtualScreen), metric(smCYVirtualScreen)
	if x < 0 || y < 0 || x >= w || y >= h {
		return fmt.Errorf("%w: (%d, %d) is outside the %dx%d screen", ErrInvalid, x, y, w, h)
	}
	x0, y0 := origin()
	if r, _, e := pSetCursorPos.Call(uintptr(x+x0), uintptr(y+y0)); r == 0 {
		return fmt.Errorf("SetCursorPos: %w", e)
	}
	var p point
	if r, _, e := pGetCursorPos.Call(uintptr(unsafe.Pointer(&p))); r == 0 {
		return fmt.Errorf("GetCursorPos: %w", e)
	}
	if int(p.x) != x+x0 || int(p.y) != y+y0 {
		return fmt.Errorf("%w: asked for the pointer at (%d, %d) but it is at (%d, %d); something else is holding or moving it", ErrBlocked, x, y, int(p.x)-x0, int(p.y)-y0)
	}
	return nil
}

func (d winDesk) Click(x, y int, b Button, double bool) error {
	if err := d.reach(true, x, y); err != nil {
		return err
	}
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

func (d winDesk) Key(c Chord) error {
	if err := d.reach(false, 0, 0); err != nil {
		return err
	}
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

// typePacing separates the characters of Type so the target keeps up.
const typePacing = 5 * time.Millisecond

func (d winDesk) Type(text string) error {
	if err := d.reach(false, 0, 0); err != nil {
		return err
	}
	for i, g := range typeGroups(text, foregroundLayout()) {
		if i > 0 {
			time.Sleep(typePacing)
		}
		if err := keyEvents(g); err != nil {
			return err
		}
	}
	return nil
}

// Run starts program as if the user had launched it: no inherited handles, and
// console programs get a console window of their own rather than the agent's
// windowless one.
func (winDesk) Run(program string, args []string) (uint32, error) {
	if program == "" {
		return 0, fmt.Errorf("%w: program is empty", ErrInvalid)
	}
	path := program
	if p, err := exec.LookPath(program); err == nil {
		path = p
	}
	cmdline, err := windows.UTF16PtrFromString(windows.ComposeCommandLine(append([]string{path}, args...)))
	if err != nil {
		return 0, fmt.Errorf("%w: %w", ErrInvalid, err)
	}
	si := windows.StartupInfo{Cb: uint32(unsafe.Sizeof(windows.StartupInfo{}))}
	var pi windows.ProcessInformation
	if err := windows.CreateProcess(nil, cmdline, nil, nil, false, windows.CREATE_NEW_CONSOLE|windows.CREATE_NEW_PROCESS_GROUP, nil, nil, &si, &pi); err != nil {
		return 0, fmt.Errorf("start %s: %w", program, err)
	}
	windows.CloseHandle(pi.Thread)
	windows.CloseHandle(pi.Process)
	return pi.ProcessId, nil
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

var ownIntegrity = sync.OnceValue(func() Integrity {
	i, err := tokenIntegrity(windows.GetCurrentProcessToken())
	if err != nil {
		return IntegrityMedium
	}
	return i
})

func tokenIntegrity(t windows.Token) (Integrity, error) {
	var n uint32
	if err := windows.GetTokenInformation(t, windows.TokenIntegrityLevel, nil, 0, &n); !errors.Is(err, windows.ERROR_INSUFFICIENT_BUFFER) {
		return 0, fmt.Errorf("size of the integrity label: %w", err)
	}
	buf := make([]byte, n)
	if err := windows.GetTokenInformation(t, windows.TokenIntegrityLevel, &buf[0], n, &n); err != nil {
		return 0, fmt.Errorf("read the integrity label: %w", err)
	}
	sid := (*windows.Tokenmandatorylabel)(unsafe.Pointer(&buf[0])).Label.Sid
	return Integrity(sid.SubAuthority(uint32(sid.SubAuthorityCount()) - 1)), nil
}

// processInfo reads the executable name and integrity level of a process.
// Only a process of a higher integrity level than the agent refuses both
// handles, so access denied marks the result unreadable.
func processInfo(pid uint32) (name string, level Integrity, unreadable bool) {
	h, err := windows.OpenProcess(windows.PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
	if err != nil {
		return "", 0, errors.Is(err, windows.ERROR_ACCESS_DENIED)
	}
	defer windows.CloseHandle(h)
	var buf [windows.MAX_PATH * 4]uint16
	size := uint32(len(buf))
	if windows.QueryFullProcessImageName(h, 0, &buf[0], &size) == nil {
		full := windows.UTF16ToString(buf[:size])
		name = full[strings.LastIndexByte(full, '\\')+1:]
	}
	var tok windows.Token
	if err := windows.OpenProcessToken(h, windows.TOKEN_QUERY, &tok); err != nil {
		return name, 0, errors.Is(err, windows.ERROR_ACCESS_DENIED)
	}
	defer tok.Close()
	level, err = tokenIntegrity(tok)
	return name, level, errors.Is(err, windows.ERROR_ACCESS_DENIED)
}

func windowInfo(hwnd uintptr) Foreground {
	f := Foreground{AgentIntegrity: ownIntegrity()}
	if hwnd == 0 {
		return f
	}
	pGetWindowThreadProcessID.Call(hwnd, uintptr(unsafe.Pointer(&f.PID)))
	if f.PID == 0 {
		return f
	}
	f.Title = windowTitle(hwnd)
	f.Process, f.Integrity, f.Unreadable = processInfo(f.PID)
	return f
}

func (winDesk) Foreground() (Foreground, error) {
	hwnd, _, _ := pGetForegroundWindow.Call()
	return windowInfo(hwnd), nil
}

const gaRoot = 2

// reach fails before any input is injected when Windows would drop it: the
// screen is locked or on a secure desktop, the foreground window is above
// the agent's integrity level, or, for a click, so is the window under the
// target point. SendInput reports success in all of these cases.
func (d winDesk) reach(mouse bool, x, y int) error {
	name, err := InputDesktop()
	if ClassifyInputDesktop(name, err) == ScreenLocked {
		return fmt.Errorf("%w: the input desktop is not the user's (locked screen, logon screen or a UAC prompt on the secure desktop); the user must unlock or answer it first", ErrBlocked)
	}
	fg, _ := d.Foreground()
	if err := CheckReach(fg, "foreground window"); err != nil {
		return err
	}
	if !mouse {
		return nil
	}
	x0, y0 := origin()
	pt := uintptr(uint32(int32(x+x0))) | uintptr(uint32(int32(y+y0)))<<32
	hwnd, _, _ := pWindowFromPoint.Call(pt)
	if hwnd != 0 {
		if root, _, _ := pGetAncestor.Call(hwnd, gaRoot); root != 0 {
			hwnd = root
		}
	}
	return CheckReach(windowInfo(hwnd), "window under the pointer")
}

var (
	pGetKeyboardLayout = user32.NewProc("GetKeyboardLayout")
	pVkKeyScanEx       = user32.NewProc("VkKeyScanExW")
	pMapVirtualKeyEx   = user32.NewProc("MapVirtualKeyExW")
	pGetKeyState       = user32.NewProc("GetKeyState")
)

const (
	vkCapital = 0x14
	shiftBit  = 1
)

// foregroundLayout is the keyboard layout of the foreground window's thread,
// which decides what the keys we press produce.
func foregroundLayout() layout {
	var tid uintptr
	if hwnd, _, _ := pGetForegroundWindow.Call(); hwnd != 0 {
		tid, _, _ = pGetWindowThreadProcessID.Call(hwnd, 0)
	}
	hkl, _, _ := pGetKeyboardLayout.Call(tid)
	caps, _, _ := pGetKeyState.Call(vkCapital)
	capsOn := caps&1 != 0
	return layout{
		key: func(r rune) (uint16, bool, bool) {
			if r > 0xFFFF {
				return 0, false, false
			}
			v, _, _ := pVkKeyScanEx.Call(uintptr(r), hkl)
			res := int16(v)
			if res == -1 {
				return 0, false, false
			}
			vk, state := uint16(res&0xFF), uint8(res>>8)
			if state&^shiftBit != 0 {
				return 0, false, false
			}
			shift := state&shiftBit != 0
			if capsOn && vk >= 'A' && vk <= 'Z' {
				shift = !shift
			}
			return vk, shift, true
		},
		scan: func(vk uint16) uint16 {
			s, _, _ := pMapVirtualKeyEx.Call(uintptr(vk), 0, hkl)
			return uint16(s)
		},
	}
}
