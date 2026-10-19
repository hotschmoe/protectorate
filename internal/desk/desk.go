// Package desk drives the interactive desktop of the machine the node agent
// runs on. Only Windows has an implementation, built on syscalls alone.
package desk

import (
	"bytes"
	"errors"
	"fmt"
	"image"
	"image/png"
	"strings"
)

// Errors the node agent maps to Connect codes.
var (
	// ErrNotInteractive means the process is not in a logged-in user's
	// interactive session.
	ErrNotInteractive = errors.New("not in an interactive desktop session")
	// ErrUnsupported means this OS has no desktop driver.
	ErrUnsupported = errors.New("desktop control is not implemented on this OS")
	// ErrInvalid marks bad arguments.
	ErrInvalid = errors.New("invalid argument")
	// ErrBlocked means input would not reach its target: Windows drops it
	// without an error when the target is a higher-integrity window (UIPI),
	// and the secure desktop and a locked screen take none.
	ErrBlocked = errors.New("input would not reach its target")
)

// Window is a visible top-level window in screenshot coordinates.
type Window struct {
	Title         string
	X, Y          int
	Width, Height int
	PID           uint32
	Minimized     bool
}

// Integrity is a Windows mandatory integrity level (the RID of the token's
// integrity SID).
type Integrity uint32

const (
	IntegrityUntrusted Integrity = 0x0000
	IntegrityLow       Integrity = 0x1000
	IntegrityMedium    Integrity = 0x2000
	IntegrityHigh      Integrity = 0x3000
	IntegritySystem    Integrity = 0x4000
	IntegrityProtected Integrity = 0x5000
)

func (i Integrity) String() string {
	switch {
	case i >= IntegrityProtected:
		return "protected"
	case i >= IntegritySystem:
		return "system"
	case i >= IntegrityHigh:
		return "high"
	case i >= IntegrityMedium:
		return "medium"
	case i >= IntegrityLow:
		return "low"
	}
	return "untrusted"
}

// Foreground is the top-level window that has the keyboard focus, or the one
// under a point, with the process that owns it. PID is 0 when there is no such
// window. Unreadable means the owner's token could not be opened, which only a
// process of a higher integrity level does to the agent. AgentIntegrity is the
// level of the process that injects input.
type Foreground struct {
	Title          string
	PID            uint32
	Process        string
	Integrity      Integrity
	Unreadable     bool
	AgentIntegrity Integrity
}

// IntegrityName is the level as shown to users.
func (f Foreground) IntegrityName() string {
	if f.Unreadable {
		return "unreadable"
	}
	return f.Integrity.String()
}

// Blocks reports whether Windows drops input the agent injects into f.
func (f Foreground) Blocks() bool {
	return f.PID != 0 && (f.Unreadable || f.Integrity > f.AgentIntegrity)
}

// CheckReach returns an ErrBlocked error naming the window and the fix when
// the agent's input cannot reach f. what says how f was found, for example
// "foreground window".
func CheckReach(f Foreground, what string) error {
	if !f.Blocks() {
		return nil
	}
	return fmt.Errorf("%w: the %s %q (%s, pid %d, integrity %s) is above this agent (integrity %s), and Windows silently drops input injected into it; dismiss or close it on the machine (for example a Windows Security or firewall prompt), or reinstall the agent elevated with `protectorate node install --elevated` from a terminal opened with Run as administrator",
		ErrBlocked, what, f.Title, f.Process, f.PID, f.IntegrityName(), f.AgentIntegrity)
}

// Button is a mouse button.
type Button int

const (
	Left Button = iota
	Right
	Middle
)

// Desktop is one machine's interactive desktop. Coordinates are pixels of the
// image Screenshot returns.
type Desktop interface {
	// Ready reports why the desktop cannot be driven, nil when it can.
	Ready() error
	Screenshot() (*image.RGBA, error)
	Windows() ([]Window, error)
	// Foreground reports the window that receives keystrokes now.
	Foreground() (Foreground, error)
	Click(x, y int, b Button, double bool) error
	Move(x, y int) error
	Type(text string) error
	Key(chord Chord) error
	Run(program string, args []string) (pid uint32, err error)
}

// EncodePNG encodes a screenshot.
func EncodePNG(img *image.RGBA) ([]byte, error) {
	var buf bytes.Buffer
	enc := png.Encoder{CompressionLevel: png.BestSpeed}
	if err := enc.Encode(&buf, img); err != nil {
		return nil, fmt.Errorf("encode png: %w", err)
	}
	return buf.Bytes(), nil
}

// ParseButton accepts "", "left", "right" or "middle".
func ParseButton(s string) (Button, error) {
	switch strings.ToLower(strings.TrimSpace(s)) {
	case "", "left":
		return Left, nil
	case "right":
		return Right, nil
	case "middle":
		return Middle, nil
	}
	return 0, fmt.Errorf("%w: button %q, want left, right or middle", ErrInvalid, s)
}

// Session describes the process's place on the Windows session model.
type Session struct {
	ID            uint32
	WindowStation string
}

// CheckInteractive decides whether a process in s can drive a desktop. A
// service, and anything started by sshd, lives in session 0 on a
// non-interactive window station; the logged-in user's session is 1 or
// higher on "WinSta0".
func CheckInteractive(s Session, probeErr error) error {
	switch {
	case probeErr != nil:
		return fmt.Errorf("%w: cannot inspect the session: %v", ErrNotInteractive, probeErr)
	case s.ID == 0:
		return fmt.Errorf("%w: this process runs in session 0 (a service or an SSH session), which has no visible desktop; start the node agent in the logged-in user's session with `protectorate node install` (a logon task)", ErrNotInteractive)
	case !strings.EqualFold(s.WindowStation, "WinSta0"):
		return fmt.Errorf("%w: this process runs on window station %q, not the interactive WinSta0; start the node agent in the logged-in user's session with `protectorate node install` (a logon task)", ErrNotInteractive, s.WindowStation)
	}
	return nil
}

// ErrInputDesktopDenied means the desktop receiving input could not be
// opened. A locked screen, the logon screen and UAC prompts all do that.
var ErrInputDesktopDenied = errors.New("input desktop is not accessible")

// ScreenState says whether input can reach the user's desktop.
type ScreenState int

const (
	ScreenUnknown ScreenState = iota
	ScreenUnlocked
	ScreenLocked
)

// ClassifyInputDesktop reads the result of InputDesktop: only the "Default"
// desktop takes input for the logged-in user's programs.
func ClassifyInputDesktop(name string, err error) ScreenState {
	switch {
	case errors.Is(err, ErrInputDesktopDenied):
		return ScreenLocked
	case err != nil:
		return ScreenUnknown
	case strings.EqualFold(name, "Default"):
		return ScreenUnlocked
	case strings.EqualFold(name, "Winlogon"), strings.EqualFold(name, "Screen-saver"):
		return ScreenLocked
	}
	return ScreenUnknown
}
