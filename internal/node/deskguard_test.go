package node

import (
	"context"
	"errors"
	"fmt"
	"image"
	"os"
	"path/filepath"
	"testing"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/desk"
)

type guardDesk struct {
	ready error
	typed []string
}

func (s *guardDesk) Ready() error { return s.ready }
func (s *guardDesk) Screenshot() (*image.RGBA, error) {
	return image.NewRGBA(image.Rect(0, 0, 1, 1)), nil
}
func (s *guardDesk) Foreground() (desk.Foreground, error)    { return desk.Foreground{}, nil }
func (s *guardDesk) Windows() ([]desk.Window, error)         { return nil, nil }
func (s *guardDesk) Click(int, int, desk.Button, bool) error { return nil }
func (s *guardDesk) Move(int, int) error                     { return nil }
func (s *guardDesk) Type(t string) error                     { s.typed = append(s.typed, t); return nil }
func (s *guardDesk) Key(desk.Chord) error                    { return nil }
func (s *guardDesk) Run(string, []string) (uint32, error)    { return 1, nil }

type guardRig struct {
	g      *DeskGuard
	d      *guardDesk
	now    time.Time
	locked int
	screen desk.ScreenState
}

func newGuardRig(t *testing.T, p *pb.DeskPolicy) *guardRig {
	t.Helper()
	r := &guardRig{d: &guardDesk{}, now: time.Date(2026, 10, 19, 12, 0, 0, 0, time.UTC), screen: desk.ScreenUnlocked}
	clock := func() time.Time { return r.now }
	r.g = &DeskGuard{D: r.d, Now: clock, Screen: func() desk.ScreenState { return r.screen }, Lock: func() error { r.locked++; return nil }, Path: filepath.Join(t.TempDir(), policyFile)}
	r.g.Input = desk.NewTracker(clock)
	if err := r.g.SetPolicy(p); err != nil {
		t.Fatal(err)
	}
	return r
}

func nightPolicy(lock bool) *pb.DeskPolicy {
	return &pb.DeskPolicy{Window: "19:00-07:00", Idle: durationpb.New(15 * time.Minute), Timezone: "UTC", LockAfter: lock}
}

func (r *guardRig) at(h, m int) { r.now = time.Date(2026, 10, 20, h, m, 0, 0, time.UTC) }

func TestGuardCheck(t *testing.T) {
	tests := []struct {
		name   string
		policy *pb.DeskPolicy
		setup  func(*guardRig)
		want   pb.DeskReason
		deny   bool
	}{
		{"no policy allows", nil, func(r *guardRig) { r.at(12, 0) }, pb.DeskReason_DESK_REASON_AVAILABLE, false},
		{"away at night", nightPolicy(false), func(r *guardRig) { r.at(23, 0) }, pb.DeskReason_DESK_REASON_AVAILABLE, false},
		{"outside window", nightPolicy(false), func(r *guardRig) { r.at(12, 0) }, pb.DeskReason_DESK_REASON_OUTSIDE_WINDOW, true},
		{"user returned", nightPolicy(false), func(r *guardRig) { r.at(23, 0); r.g.Input.Event(false); r.now = r.now.Add(5 * time.Minute) }, pb.DeskReason_DESK_REASON_USER_ACTIVE, true},
		{"injected input is not the user", nightPolicy(false), func(r *guardRig) { r.at(23, 0); r.g.Input.Event(true) }, pb.DeskReason_DESK_REASON_AVAILABLE, false},
		{"locked screen", nightPolicy(false), func(r *guardRig) { r.at(23, 0); r.screen = desk.ScreenLocked }, pb.DeskReason_DESK_REASON_LOCKED, true},
		{"session 0", nightPolicy(false), func(r *guardRig) { r.at(23, 0); r.d.ready = desk.ErrNotInteractive }, pb.DeskReason_DESK_REASON_NOT_INTERACTIVE, true},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			r := newGuardRig(t, tc.policy)
			tc.setup(r)
			if got := r.g.State().Reason; got != tc.want {
				t.Errorf("state reason = %v, want %v", got, tc.want)
			}
			err := r.g.Check()
			var ce *connect.Error
			if tc.deny {
				if !errors.As(err, &ce) || ce.Code() != connect.CodeFailedPrecondition || ce.Meta().Get(DeskUnavailableHeader) != tc.want.String() {
					t.Errorf("Check = %v, want a refusal carrying %v", err, tc.want)
				}
			} else if err != nil {
				t.Errorf("Check = %v, want nil", err)
			}
		})
	}
}

func TestGuardRefusesInputCommands(t *testing.T) {
	r := newGuardRig(t, nightPolicy(false))
	r.at(12, 0)
	dsk := &Desk{D: r.d, Guard: r.g}
	acts := []*pb.DeskActionRequest{
		{Action: &pb.DeskActionRequest_Click{Click: &pb.ClickRequest{X: 1, Y: 1}}},
		{Action: &pb.DeskActionRequest_Move{Move: &pb.MoveRequest{}}},
		{Action: &pb.DeskActionRequest_Type{Type: &pb.TypeRequest{Text: "hi"}}},
		{Action: &pb.DeskActionRequest_Key{Key: &pb.KeyRequest{Chord: "enter"}}},
		{Action: &pb.DeskActionRequest_Run{Run: &pb.RunRequest{Program: "x"}}},
	}
	for _, a := range acts {
		_, err := dsk.Do(context.Background(), a)
		var ce *connect.Error
		if !errors.As(err, &ce) || ce.Meta().Get(DeskUnavailableHeader) == "" {
			t.Errorf("%T: err = %v, want a policy refusal", a.Action, err)
		}
	}
	if len(r.d.typed) != 0 {
		t.Errorf("typed %v while unavailable", r.d.typed)
	}
	for _, a := range []*pb.DeskActionRequest{
		{Action: &pb.DeskActionRequest_Screenshot{Screenshot: &pb.ScreenshotRequest{}}},
		{Action: &pb.DeskActionRequest_ListWindows{ListWindows: &pb.ListWindowsRequest{}}},
	} {
		if _, err := dsk.Do(context.Background(), a); err != nil {
			t.Errorf("%T must not be gated: %v", a.Action, err)
		}
	}
	r.at(23, 0)
	if _, err := dsk.Do(context.Background(), acts[0]); err != nil {
		t.Errorf("click while away: %v", err)
	}
}

func TestGuardStopsTypingWhenUserReturns(t *testing.T) {
	r := newGuardRig(t, nightPolicy(false))
	r.at(23, 0)
	text := string(make([]rune, 0))
	for range 3 * typeChunk {
		text += "a"
	}
	dsk := &Desk{D: &returnDesk{guardDesk: r.d, onType: func(n int) {
		if n == 1 {
			r.g.Input.Event(false)
		}
	}}, Guard: r.g}
	_, err := dsk.Do(context.Background(), &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Type{Type: &pb.TypeRequest{Text: text}}})
	if err == nil {
		t.Fatal("typing continued after the user returned")
	}
	if len(r.d.typed) != 1 || len(r.d.typed[0]) != typeChunk {
		t.Errorf("typed pieces = %d, want exactly the first chunk", len(r.d.typed))
	}
}

type returnDesk struct {
	*guardDesk
	onType func(n int)
}

func (d *returnDesk) Type(s string) error {
	err := d.guardDesk.Type(s)
	d.onType(len(d.guardDesk.typed))
	return err
}

func TestGuardLocking(t *testing.T) {
	tests := []struct {
		name  string
		lock  bool
		steps func(*guardRig)
		locks int
	}{
		{"window ends while away", true, func(r *guardRig) { r.at(6, 59); r.g.Step(); r.at(7, 1); r.g.Step() }, 1},
		{"window end without lock-after", false, func(r *guardRig) { r.at(6, 59); r.g.Step(); r.at(7, 1); r.g.Step() }, 0},
		{"window ends but the user is active", true, func(r *guardRig) {
			r.at(6, 50)
			r.g.Step()
			r.at(6, 59)
			r.g.Input.Event(false)
			r.at(7, 1)
			r.g.Step()
		}, 0},
		{"window end locks once", true, func(r *guardRig) {
			r.at(6, 59)
			r.g.Step()
			r.at(7, 1)
			r.g.Step()
			r.at(7, 2)
			r.g.Step()
		}, 1},
		{"lease ends, quiet minute", true, func(r *guardRig) {
			r.at(23, 0)
			r.g.Step()
			r.g.LeaseEnded()
			r.now = r.now.Add(30 * time.Second)
			r.g.Step()
			r.now = r.now.Add(31 * time.Second)
			r.g.Step()
		}, 1},
		{"another command follows the lease", true, func(r *guardRig) {
			r.at(23, 0)
			r.g.Step()
			r.g.LeaseEnded()
			r.now = r.now.Add(30 * time.Second)
			if err := r.g.Check(); err != nil {
				panic(err)
			}
			r.now = r.now.Add(2 * time.Minute)
			r.g.Step()
		}, 0},
		{"lease ends but the user is active", true, func(r *guardRig) {
			r.at(23, 0)
			r.g.Step()
			r.g.LeaseEnded()
			r.now = r.now.Add(time.Minute)
			r.g.Input.Event(false)
			r.now = r.now.Add(time.Second)
			r.g.Step()
		}, 0},
		{"lease ends outside the window", true, func(r *guardRig) {
			r.at(12, 0)
			r.g.Step()
			r.g.LeaseEnded()
			r.now = r.now.Add(2 * time.Minute)
			r.g.Step()
		}, 0},
		{"already locked", true, func(r *guardRig) {
			r.at(6, 59)
			r.g.Step()
			r.screen = desk.ScreenLocked
			r.at(7, 1)
			r.g.Step()
		}, 0},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			r := newGuardRig(t, nightPolicy(tc.lock))
			tc.steps(r)
			if r.locked != tc.locks {
				t.Errorf("locked %d times, want %d", r.locked, tc.locks)
			}
		})
	}
}

func TestGuardPolicyPersistence(t *testing.T) {
	home := t.TempDir()
	g, err := NewDeskGuard(&guardDesk{}, home)
	if err != nil {
		t.Fatal(err)
	}
	if err := g.SetPolicy(nightPolicy(true)); err != nil {
		t.Fatal(err)
	}
	g2, err := NewDeskGuard(&guardDesk{}, home)
	if err != nil {
		t.Fatal(err)
	}
	if p := g2.State().Policy; p == nil || p.Window != "19:00-07:00" || !p.LockAfter || p.Idle.AsDuration() != 15*time.Minute {
		t.Errorf("reloaded policy = %v", p)
	}
	if err := g2.SetPolicy(nil); err != nil {
		t.Fatal(err)
	}
	g3, err := NewDeskGuard(&guardDesk{}, home)
	if err != nil {
		t.Fatal(err)
	}
	if g3.State().Policy != nil {
		t.Error("cleared policy came back")
	}
	if err := g3.SetPolicy(&pb.DeskPolicy{Window: "nonsense", Idle: durationpb.New(time.Minute)}); !errors.Is(err, desk.ErrInvalid) {
		t.Errorf("bad window: %v", err)
	}
	if err := os.WriteFile(filepath.Join(home, policyFile), []byte("{not json"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := NewDeskGuard(&guardDesk{}, home); err == nil {
		t.Error("corrupt policy file accepted")
	}
}

func TestBlockedInputIsFailedPrecondition(t *testing.T) {
	err := deskErr(fmt.Errorf("%w: prompt", desk.ErrBlocked))
	if connect.CodeOf(err) != connect.CodeFailedPrecondition {
		t.Errorf("code = %v", connect.CodeOf(err))
	}
}
