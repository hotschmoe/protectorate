package desk

import (
	"errors"
	"fmt"
	"strconv"
	"strings"
	"time"
	_ "time/tzdata" // IANA zones on machines without a zoneinfo database (Windows)
)

// Policy is when agents may drive the desktop. A nil *Policy means always.
type Policy struct {
	// Start and End are minutes after midnight; Start > End spans midnight.
	Start, End int
	Idle       time.Duration
	// Loc is the policy's time zone.
	Loc       *time.Location
	LockAfter bool
}

// Reason says why the desktop is or is not available.
type Reason int

const (
	Available Reason = iota
	OutsideWindow
	UserActive
	NotInteractive
	Locked
	Unsupported
)

// ParseWindow reads "HH:MM-HH:MM" into minutes after midnight.
func ParseWindow(s string) (start, end int, err error) {
	a, b, ok := strings.Cut(strings.TrimSpace(s), "-")
	if !ok {
		return 0, 0, fmt.Errorf("%w: window %q, want HH:MM-HH:MM", ErrInvalid, s)
	}
	if start, err = parseClock(a); err != nil {
		return 0, 0, err
	}
	if end, err = parseClock(b); err != nil {
		return 0, 0, err
	}
	if start == end {
		return 0, 0, fmt.Errorf("%w: window %q is empty; to allow every hour remove the policy or use 00:00-23:59", ErrInvalid, s)
	}
	return start, end, nil
}

func parseClock(s string) (int, error) {
	h, m, ok := strings.Cut(strings.TrimSpace(s), ":")
	hh, err1 := strconv.Atoi(h)
	mm, err2 := strconv.Atoi(m)
	if !ok || err1 != nil || err2 != nil || hh < 0 || hh > 23 || mm < 0 || mm > 59 || len(m) != 2 {
		return 0, fmt.Errorf("%w: time %q, want HH:MM between 00:00 and 23:59", ErrInvalid, s)
	}
	return hh*60 + mm, nil
}

// NewPolicy validates a policy. An empty timezone means local, the zone of the
// machine that evaluates it.
func NewPolicy(window string, idle time.Duration, timezone string, lockAfter bool) (*Policy, error) {
	start, end, err := ParseWindow(window)
	if err != nil {
		return nil, err
	}
	if idle <= 0 {
		return nil, fmt.Errorf("%w: idle %s must be positive", ErrInvalid, idle)
	}
	loc := time.Local
	if timezone != "" {
		if loc, err = time.LoadLocation(timezone); err != nil {
			return nil, fmt.Errorf("%w: time zone %q: %w", ErrInvalid, timezone, err)
		}
	}
	return &Policy{Start: start, End: end, Idle: idle, Loc: loc, LockAfter: lockAfter}, nil
}

// InWindow reports whether t falls inside the window.
func (p *Policy) InWindow(t time.Time) bool {
	t = t.In(p.Loc)
	m := t.Hour()*60 + t.Minute()
	if p.Start < p.End {
		return m >= p.Start && m < p.End
	}
	return m >= p.Start || m < p.End
}

// Inputs is everything an evaluation depends on, so it can be tested with a
// fixed clock.
type Inputs struct {
	Now time.Time
	// LastInput is the last physical input; the monitor's start time when it
	// has seen none, because the user may have left just before.
	LastInput time.Time
	// Ready is the desktop's Ready error: nil, ErrNotInteractive or
	// ErrUnsupported.
	Ready  error
	Screen ScreenState
}

// Verdict is the result of an evaluation.
type Verdict struct {
	Available bool
	Reason    Reason
	Detail    string
}

// Evaluate decides availability. A nil policy only requires a desktop that can
// be driven at all; with a policy, the screen must be unlocked, the time
// inside the window and the user idle for the policy's period.
func Evaluate(p *Policy, in Inputs) Verdict {
	switch {
	case errors.Is(in.Ready, ErrUnsupported):
		return Verdict{Reason: Unsupported, Detail: in.Ready.Error()}
	case in.Ready != nil:
		return Verdict{Reason: NotInteractive, Detail: "not an interactive session: " + in.Ready.Error()}
	case p == nil:
		return Verdict{Available: true, Reason: Available, Detail: "no policy: always available"}
	case !p.InWindow(in.Now):
		return Verdict{Reason: OutsideWindow, Detail: fmt.Sprintf("outside the window %s-%s", clock(p.Start), clock(p.End))}
	}
	if idle := in.Now.Sub(in.LastInput); idle < p.Idle {
		return Verdict{Reason: UserActive, Detail: fmt.Sprintf("user active: physical input %s ago, %s of idle required", idle.Round(time.Second), p.Idle)}
	}
	if in.Screen == ScreenLocked {
		return Verdict{Reason: Locked, Detail: "the screen is locked; agents cannot drive a locked desktop, unlock it"}
	}
	return Verdict{Available: true, Reason: Available, Detail: "away: inside the window and idle"}
}

func clock(m int) string { return fmt.Sprintf("%02d:%02d", m/60, m%60) }

// FormatWindow renders a policy's window the way ParseWindow reads it.
func (p *Policy) FormatWindow() string { return clock(p.Start) + "-" + clock(p.End) }
