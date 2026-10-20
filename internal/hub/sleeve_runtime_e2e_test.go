//go:build linux

package hub

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"

	"connectrpc.com/connect"
	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"google.golang.org/protobuf/types/known/durationpb"
)

func TestSleeveReadOnlyMountHTTP(t *testing.T) {
	r := newChatRig(t)
	refs := filepath.Join(r.ws, "references")
	if err := os.MkdirAll(refs, 0700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(refs, "reference"), []byte("owner reference"), 0600); err != nil {
		t.Fatal(err)
	}
	res, err := r.sl.SpawnSleeve(r.ctx, connect.NewRequest(&pb.SpawnSleeveRequest{
		Name: r.name, Workspace: r.ws, Harness: pb.Harness_HARNESS_SHELL,
		ReadOnly: []*pb.ReadOnlyMount{{Source: refs, Target: "/references"}, {Source: refs}},
		Env:      map[string]string{"REFERENCE_HOST_PATH": refs, "GOMAXPROCS": "3"},
		Limits:   &pb.SleeveLimits{CpuPercent: 200, MemoryBytes: 512 << 20},
		Args: []string{"bash", "-c", `set -eu
test "$(cat /references/reference)" = 'owner reference'
test "$(cat "$REFERENCE_HOST_PATH/reference")" = 'owner reference'
if touch /references/write 2>/dev/null; then exit 1; fi
if touch "$REFERENCE_HOST_PATH/write" 2>/dev/null; then exit 1; fi
test "$GOMAXPROCS" = 3
test "$GOFLAGS" = -p=2
test "$FLUTTER_TEST_CONCURRENCY" = 2
touch "$PWD/verified"
exec sleep 600`},
	}))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _, _ = r.sl.KillSleeve(r.ctx, connect.NewRequest(&pb.KillSleeveRequest{Name: r.name})) })
	waitFor(t, "read-only sandbox verification", func() bool { _, err := os.Stat(filepath.Join(r.ws, "verified")); return err == nil })
	if got := res.Msg.Sleeve.ReadOnly; len(got) != 2 || got[0].Target != "/references" || got[1].Target != refs {
		t.Fatalf("mounts not recorded: %v", got)
	}
	if _, err := os.Stat(filepath.Join(refs, "write")); !os.IsNotExist(err) {
		t.Fatalf("host reference was changed: %v", err)
	}
	listed, err := r.sl.ListWorkspaces(r.ctx, connect.NewRequest(&pb.ListWorkspacesRequest{}))
	if err != nil {
		t.Fatal(err)
	}
	found := false
	for _, w := range listed.Msg.Workspaces {
		if w.Path == refs && w.ReadOnly {
			found = true
		}
	}
	if !found {
		t.Fatalf("reference directory missing from workspace discovery: %v", listed.Msg)
	}
	link := filepath.Join(t.TempDir(), "state-link")
	if err := os.Symlink(r.cfg.Home, link); err != nil {
		t.Fatal(err)
	}
	for _, mount := range []*pb.ReadOnlyMount{
		{Source: r.cfg.Home}, {Source: link},
		{Source: filepath.Join(r.cfg.UserHome, ".ssh")},
		{Source: refs, Target: filepath.Join(r.cfg.UserHome, ".codex")},
		{Source: refs, Target: filepath.Join(link, "missing")},
	} {
		_, err := r.sl.SpawnSleeve(r.ctx, connect.NewRequest(&pb.SpawnSleeveRequest{Name: r.name + "-denied", Workspace: r.ws, Harness: pb.Harness_HARNESS_SHELL, ReadOnly: []*pb.ReadOnlyMount{mount}}))
		if connect.CodeOf(err) != connect.CodeInvalidArgument {
			t.Fatalf("secret mount %v: %v", mount, err)
		}
	}
}

func TestSleeveIdleReapHTTP(t *testing.T) {
	r := newMailRig(t)
	_, err := r.sl.SpawnSleeve(r.ctx, connect.NewRequest(&pb.SpawnSleeveRequest{
		Name: r.name, Workspace: r.ws, Harness: pb.Harness_HARNESS_CLAUDE,
		ExitAfterIdle: durationpb.New(150 * time.Millisecond),
		Limits:        &pb.SleeveLimits{CpuPercent: 100, MemoryBytes: 512 << 20},
	}))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := r.send("slow", "runtime-idle-working"); err != nil {
		t.Fatal(err)
	}
	r.waitState(pb.ChatState_CHAT_STATE_WORKING)
	workingUntil := time.Now().Add(350 * time.Millisecond)
	for time.Now().Before(workingUntil) {
		r.h.coord.reapIdle(r.ctx)
		time.Sleep(25 * time.Millisecond)
	}
	if _, err := r.chat.Interrupt(r.ctx, connect.NewRequest(&pb.InterruptRequest{Sleeve: r.name})); err != nil {
		t.Fatal(err)
	}
	r.waitState(pb.ChatState_CHAT_STATE_IDLE)
	_, err = r.owner.Send(r.ctx, connect.NewRequest(&pb.MessageServiceSendRequest{To: r.name, Body: "pending work"}))
	if err != nil {
		t.Fatal(err)
	}
	until := time.Now().Add(350 * time.Millisecond)
	for time.Now().Before(until) {
		r.h.coord.reapIdle(r.ctx)
		time.Sleep(25 * time.Millisecond)
	}
	if len(r.h.coord.mail.Pending()[r.name]) == 0 {
		t.Fatal("mail did not stay pending")
	}
	if _, err := r.sl.GetSleeve(r.ctx, connect.NewRequest(&pb.GetSleeveRequest{Name: r.name})); err != nil {
		t.Fatalf("pending mail did not prevent reap: %v", err)
	}
	// The rig's fixed clock would keep the pending mail inside the delivery debounce forever.
	r.h.coord.now = time.Now
	r.h.coord.every = 25 * time.Millisecond
	ctx, cancel := context.WithCancel(r.ctx)
	done := make(chan struct{})
	go func() { defer close(done); r.h.coord.Run(ctx) }()
	t.Cleanup(func() { cancel(); <-done })
	waitFor(t, "idle sleeve stopped with retained disk record", func() bool {
		res, err := r.sl.GetSleeve(r.ctx, connect.NewRequest(&pb.GetSleeveRequest{Name: r.name}))
		return err == nil && res.Msg.Sleeve.Killed && res.Msg.Sleeve.State == pb.SleeveState_SLEEVE_STATE_EXITED
	})
}
