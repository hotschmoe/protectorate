package node

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/encoding/protojson"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/desk"
	"github.com/hotschmoe/protectorate/internal/fsutil"
)

const (
	// DeskUnavailableHeader is set on the error of a desk command the guard
	// refused; its value is the reason name. The hub ends the caller's lease
	// when it sees it.
	DeskUnavailableHeader = "Protectorate-Desk-Unavailable"

	lockGrace    = time.Minute
	guardTick    = 5 * time.Second
	typeChunk    = 64
	policyFile   = "desk-policy.json"
	maxInputIdle = 24 * time.Hour
)

// DeskGuard decides whether agents may drive this machine's desktop and
// enforces it on every input command, whatever the hub believes. It holds the
// policy the hub sent, watches physical input and locks the workstation when
// the policy asks.
type DeskGuard struct {
	D desk.Desktop
	// Now, Screen and Lock default to the real clock and OS.
	Now    func() time.Time
	Screen func() desk.ScreenState
	Lock   func() error
	// Path, when set, persists the policy across restarts.
	Path string
	// Input tracks physical input; Run starts the OS monitor when it is nil.
	Input *desk.Tracker

	mu       sync.Mutex
	raw      *pb.DeskPolicy
	pol      *desk.Policy
	input    *desk.Tracker
	wasIn    bool
	lastCmd  time.Time
	leaseEnd time.Time
}

// NewDeskGuard loads the persisted policy from home, if any.
func NewDeskGuard(d desk.Desktop, home string) (*DeskGuard, error) {
	g := &DeskGuard{D: d, Path: filepath.Join(home, policyFile)}
	p, err := LoadDeskPolicy(home)
	if err != nil {
		return nil, err
	}
	if err := g.SetPolicy(p); err != nil {
		return nil, fmt.Errorf("%s: %w", g.Path, err)
	}
	return g, nil
}

// LoadDeskPolicy reads the policy a node agent persisted under home; nil when
// it has none.
func LoadDeskPolicy(home string) (*pb.DeskPolicy, error) {
	path := filepath.Join(home, policyFile)
	raw, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("read desk policy: %w", err)
	}
	var p pb.DeskPolicy
	if err := protojson.Unmarshal(raw, &p); err != nil {
		return nil, fmt.Errorf("parse %s: %w", path, err)
	}
	if p.Window == "" {
		return nil, nil
	}
	return &p, nil
}

func (g *DeskGuard) now() time.Time {
	if g.Now != nil {
		return g.Now()
	}
	return time.Now()
}

func (g *DeskGuard) screen() desk.ScreenState {
	if g.Screen != nil {
		return g.Screen()
	}
	return desk.ClassifyInputDesktop(desk.InputDesktop())
}

func (g *DeskGuard) lock() error {
	if g.Lock != nil {
		return g.Lock()
	}
	return desk.LockWorkstation()
}

// PolicyFromProto validates a wire policy; nil means no policy.
func PolicyFromProto(p *pb.DeskPolicy) (*desk.Policy, error) {
	if p == nil {
		return nil, nil
	}
	idle := p.Idle.AsDuration()
	if idle > maxInputIdle {
		return nil, fmt.Errorf("%w: idle %s is over %s", desk.ErrInvalid, idle, maxInputIdle)
	}
	return desk.NewPolicy(p.Window, idle, p.Timezone, p.LockAfter)
}

// SetPolicy adopts the hub's policy; nil clears it. Setting the policy already
// in force changes nothing.
func (g *DeskGuard) SetPolicy(p *pb.DeskPolicy) error {
	pol, err := PolicyFromProto(p)
	if err != nil {
		return err
	}
	g.mu.Lock()
	defer g.mu.Unlock()
	if proto.Equal(g.raw, p) {
		return nil
	}
	g.raw, g.pol = proto.Clone(p).(*pb.DeskPolicy), pol
	g.wasIn = pol != nil && pol.InWindow(g.now())
	g.leaseEnd = time.Time{}
	return g.persistLocked()
}

func (g *DeskGuard) persistLocked() error {
	if g.Path == "" {
		return nil
	}
	if g.raw == nil {
		return fsutil.WriteFile(g.Path, []byte("{}"), 0o600)
	}
	raw, err := protojson.Marshal(g.raw)
	if err != nil {
		return fmt.Errorf("encode desk policy: %w", err)
	}
	return fsutil.WriteFile(g.Path, raw, 0o600)
}

// Run starts the input monitor when this process can drive a desktop and runs
// the lock timers until ctx is done. A monitor that cannot start leaves the
// policy enforced with the agent's start as the last input, so a node whose
// hooks fail never offers its desktop while the user may be there.
func (g *DeskGuard) Run(ctx context.Context) error {
	var monErr error
	g.mu.Lock()
	start := g.Input == nil && g.D.Ready() == nil
	g.mu.Unlock()
	if start {
		t, err := desk.StartInputMonitor(ctx, g.now)
		g.mu.Lock()
		g.Input = t
		g.mu.Unlock()
		monErr = err
	}
	tick := time.NewTicker(guardTick)
	defer tick.Stop()
	for {
		select {
		case <-ctx.Done():
			return monErr
		case <-tick.C:
			g.Step()
		}
	}
}

func (g *DeskGuard) lastInputLocked() time.Time {
	if g.Input != nil {
		return g.Input.Last()
	}
	return g.startedLocked()
}

// startedLocked is the stand-in for the last input of a node that cannot see
// any: the process start, so the idle period is measured from there.
func (g *DeskGuard) startedLocked() time.Time { return processStart }

var processStart = time.Now()

func (g *DeskGuard) evaluateLocked(now time.Time) desk.Verdict {
	return desk.Evaluate(g.pol, desk.Inputs{Now: now, LastInput: g.lastInputLocked(), Ready: g.D.Ready(), Screen: g.screenForLocked()})
}

func (g *DeskGuard) screenForLocked() desk.ScreenState {
	if g.pol == nil || g.D.Ready() != nil {
		return desk.ScreenUnknown
	}
	return g.screen()
}

var reasonToProto = map[desk.Reason]pb.DeskReason{
	desk.Available:      pb.DeskReason_DESK_REASON_AVAILABLE,
	desk.OutsideWindow:  pb.DeskReason_DESK_REASON_OUTSIDE_WINDOW,
	desk.UserActive:     pb.DeskReason_DESK_REASON_USER_ACTIVE,
	desk.NotInteractive: pb.DeskReason_DESK_REASON_NOT_INTERACTIVE,
	desk.Locked:         pb.DeskReason_DESK_REASON_LOCKED,
	desk.Unsupported:    pb.DeskReason_DESK_REASON_UNSUPPORTED,
}

// State evaluates the policy now.
func (g *DeskGuard) State() *pb.DeskState {
	g.mu.Lock()
	defer g.mu.Unlock()
	now := g.now()
	v := g.evaluateLocked(now)
	s := &pb.DeskState{
		Available: v.Available, Reason: reasonToProto[v.Reason], Detail: v.Detail,
		EvaluatedAt: timestamppb.New(now), Policy: proto.Clone(g.raw).(*pb.DeskPolicy),
	}
	if g.Input != nil {
		s.LastInput = timestamppb.New(g.Input.Last())
	}
	return s
}

// Check refuses an input command when the policy does not allow agents now,
// and otherwise notes that a command ran.
func (g *DeskGuard) Check() error {
	g.mu.Lock()
	defer g.mu.Unlock()
	if g.pol == nil {
		return nil
	}
	now := g.now()
	if v := g.evaluateLocked(now); !v.Available {
		return deskUnavailable(v)
	}
	g.lastCmd = now
	return nil
}

func deskUnavailable(v desk.Verdict) error {
	err := connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("desktop unavailable: %s", v.Detail))
	err.Meta().Set(DeskUnavailableHeader, reasonToProto[v.Reason].String())
	return err
}

// LeaseEnded notes that an agent's desktop lease ended.
func (g *DeskGuard) LeaseEnded() {
	g.mu.Lock()
	defer g.mu.Unlock()
	if g.pol != nil && g.pol.LockAfter {
		g.leaseEnd = g.now()
	}
}

// Step runs the lock timers once: at the end of the window, and a minute after
// a lease ended with no command since.
func (g *DeskGuard) Step() {
	g.mu.Lock()
	defer g.mu.Unlock()
	if g.pol == nil || !g.pol.LockAfter {
		return
	}
	now := g.now()
	in := g.pol.InWindow(now)
	lock := g.wasIn && !in
	g.wasIn = in
	leaseQuiet := !g.leaseEnd.IsZero() && now.Sub(g.leaseEnd) >= lockGrace
	if leaseQuiet {
		quiet := !g.lastCmd.After(g.leaseEnd)
		g.leaseEnd = time.Time{}
		lock = lock || (quiet && in)
	}
	if lock {
		g.lockLocked(now)
	}
}

func (g *DeskGuard) lockLocked(now time.Time) {
	if g.D.Ready() != nil || now.Sub(g.lastInputLocked()) < g.pol.Idle || g.screen() == desk.ScreenLocked {
		return
	}
	if err := g.lock(); err != nil {
		fmt.Fprintf(os.Stderr, "desk: lock workstation: %v\n", err)
	}
}
