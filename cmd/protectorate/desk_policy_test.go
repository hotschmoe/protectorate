package main

import (
	"strings"
	"testing"
	"time"

	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/desk"
	"github.com/hotschmoe/protectorate/internal/node"
)

func TestDeskStatusLabel(t *testing.T) {
	st := func(r pb.DeskReason) *pb.DeskState { return &pb.DeskState{Reason: r} }
	tests := []struct {
		name   string
		state  *pb.DeskState
		holder string
		policy bool
		want   string
	}{
		{"never reported", nil, "", false, "-"},
		{"no policy", st(pb.DeskReason_DESK_REASON_AVAILABLE), "", false, "available"},
		{"away", st(pb.DeskReason_DESK_REASON_AVAILABLE), "", true, "away"},
		{"in use", st(pb.DeskReason_DESK_REASON_AVAILABLE), "auto:token:sleeve:a", true, "in use by token:sleeve:a"},
		{"user active", st(pb.DeskReason_DESK_REASON_USER_ACTIVE), "", true, "user active"},
		{"outside window", st(pb.DeskReason_DESK_REASON_OUTSIDE_WINDOW), "", true, "outside window"},
		{"session 0", st(pb.DeskReason_DESK_REASON_NOT_INTERACTIVE), "", true, "unavailable: not an interactive session"},
		{"locked", st(pb.DeskReason_DESK_REASON_LOCKED), "", true, "unavailable: screen locked"},
		{"linux node", st(pb.DeskReason_DESK_REASON_UNSUPPORTED), "", false, "-"},
		{"unknown", st(pb.DeskReason_DESK_REASON_UNKNOWN), "", true, "unknown"},
	}
	for _, tc := range tests {
		if got := deskStatusLabel(tc.state, tc.holder, tc.policy); got != tc.want {
			t.Errorf("%s: %q, want %q", tc.name, got, tc.want)
		}
	}
}

func TestPolicyCheck(t *testing.T) {
	home := t.TempDir()
	noon := time.Date(2026, 10, 20, 12, 0, 0, 0, time.UTC)
	night := time.Date(2026, 10, 20, 23, 0, 0, 0, time.UTC)
	tests := []struct {
		name   string
		goos   string
		policy *pb.DeskPolicy
		now    time.Time
		ready  error
		want   []string
	}{
		{"linux", "linux", nil, noon, nil, []string{"not applicable"}},
		{"no policy", "windows", nil, noon, nil, []string{"always available"}},
		{"outside window", "windows", &pb.DeskPolicy{Window: "19:00-07:00", Idle: durationpb.New(15 * time.Minute), Timezone: "UTC", LockAfter: true}, noon, nil, []string{"19:00-07:00", "15m0s", "lock-after true", "unavailable now", "outside the window"}},
		{"inside window", "windows", &pb.DeskPolicy{Window: "19:00-07:00", Idle: durationpb.New(15 * time.Minute), Timezone: "UTC"}, night, nil, []string{"available now"}},
		{"session 0", "windows", &pb.DeskPolicy{Window: "19:00-07:00", Idle: durationpb.New(15 * time.Minute), Timezone: "UTC"}, night, desk.ErrNotInteractive, []string{"unavailable now", "not an interactive session"}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			g, err := node.NewDeskGuard(nil, home)
			if err != nil {
				t.Fatal(err)
			}
			if err := g.SetPolicy(tc.policy); err != nil {
				t.Fatal(err)
			}
			c := policyCheck(home, tc.goos, tc.now, tc.ready, desk.ScreenUnlocked)
			if c.Level == levelFail {
				t.Fatalf("failed: %s", c.Detail)
			}
			for _, w := range tc.want {
				if !strings.Contains(c.Detail, w) {
					t.Errorf("detail %q lacks %q", c.Detail, w)
				}
			}
		})
	}
}
