//go:build linux

package hub

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os/exec"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/session"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

func TestSleeveRPC(t *testing.T) {
	home := t.TempDir()
	cfg, err := sleeve.DefaultConfig(home, nil)
	if err != nil {
		t.Fatal(err)
	}
	if err := cfg.Preflight(); err != nil {
		t.Skipf("sleeves unavailable: %v", err)
	}
	h := New("test", testToken)
	h.Heartbeat = 20 * time.Millisecond
	sock := fmt.Sprintf("prot-sleeve-test-%d", time.Now().UnixNano()%1000000)
	h.Sessions = session.New(sock)
	h.SessionPoll = 20 * time.Millisecond
	name := fmt.Sprintf("hubt%d", time.Now().UnixNano()%1000000)
	t.Cleanup(func() {
		exec.Command("systemctl", "--user", "kill", "--signal=SIGKILL", sleeve.UnitName(name)+".scope").Run()
		exec.Command("tmux", "-L", sock, "kill-server").Run()
	})
	opts := h.SleeveOptions(cfg)
	opts.UsageInterval = 100 * time.Millisecond
	m, err := sleeve.Open(context.Background(), opts)
	if err != nil {
		t.Fatal(err)
	}
	h.SetSleeves(m)
	srv := httptest.NewServer(h.Handler())
	t.Cleanup(srv.Close)

	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { defer close(done); m.Run(ctx) }()
	t.Cleanup(func() { cancel(); <-done })

	hc := &http.Client{Transport: bearer{testToken}}
	sl := protectoratev1connect.NewSleeveServiceClient(hc, srv.URL)
	ss := protectoratev1connect.NewSessionServiceClient(hc, srv.URL)

	evCtx, evCancel := context.WithCancel(ctx)
	defer evCancel()
	stream, err := protectoratev1connect.NewEventServiceClient(hc, srv.URL).Subscribe(evCtx, connect.NewRequest(&pb.SubscribeRequest{}))
	if err != nil {
		t.Fatal(err)
	}
	events := make(chan string, 64)
	go func() {
		for stream.Receive() {
			if e := stream.Msg(); strings.HasPrefix(e.Kind, "sleeve.") {
				events <- e.Kind + " " + e.Subject
			}
		}
	}()
	expect := func(want string) {
		t.Helper()
		for {
			select {
			case got := <-events:
				if got == want {
					return
				}
			case <-time.After(5 * time.Second):
				t.Fatalf("no event %q", want)
			}
		}
	}

	ws := t.TempDir()
	spawn := func(n string, h pb.Harness) error {
		_, err := sl.SpawnSleeve(ctx, connect.NewRequest(&pb.SpawnSleeveRequest{
			Name: n, Workspace: ws, Harness: h, Args: []string{"sh", "-c", "echo rpc-hello; sleep 600"},
			Limits: &pb.SleeveLimits{CpuPercent: 100, MemoryBytes: 256 << 20},
		}))
		return err
	}
	for _, tt := range []struct {
		name string
		n    string
		h    pb.Harness
		want connect.Code
	}{
		{"bad name", "Bad Name", pb.Harness_HARNESS_SHELL, connect.CodeInvalidArgument},
		{"no harness", name, pb.Harness_HARNESS_UNSPECIFIED, connect.CodeInvalidArgument},
	} {
		if got := connect.CodeOf(spawn(tt.n, tt.h)); got != tt.want {
			t.Errorf("%s: code = %v, want %v", tt.name, got, tt.want)
		}
	}
	if err := spawn(name, pb.Harness_HARNESS_SHELL); err != nil {
		t.Fatal(err)
	}
	expect("sleeve.spawned " + name)
	if got := connect.CodeOf(spawn(name, pb.Harness_HARNESS_SHELL)); got != connect.CodeAlreadyExists {
		t.Errorf("duplicate code = %v", got)
	}

	var s *pb.Sleeve
	waitFor(t, "usage", func() bool {
		res, err := sl.GetSleeve(ctx, connect.NewRequest(&pb.GetSleeveRequest{Name: name}))
		if err != nil {
			t.Fatal(err)
		}
		s = res.Msg.Sleeve
		return s.Usage.MemoryCurrentBytes > 0 && s.Usage.Pids > 0
	})
	if s.State != pb.SleeveState_SLEEVE_STATE_RUNNING || s.Harness != pb.Harness_HARNESS_SHELL || s.Workspace == "" ||
		s.Limits.MemoryBytes != 256<<20 || s.Session != "sleeve-"+name {
		t.Errorf("sleeve = %v", s)
	}
	list, err := sl.ListSleeves(ctx, connect.NewRequest(&pb.ListSleevesRequest{}))
	if err != nil || len(list.Msg.Sleeves) != 1 {
		t.Fatalf("list = %v, %v", list, err)
	}
	if rows := h.sleeveRows(); len(rows) != 1 || rows[0].Name != name {
		t.Errorf("node sleeve rows = %v", rows)
	}

	waitFor(t, "pane text", func() bool {
		res, err := ss.CapturePane(ctx, connect.NewRequest(&pb.CapturePaneRequest{Name: s.Session}))
		return err == nil && strings.Contains(res.Msg.Text, "rpc-hello")
	})

	if _, err := sl.KillSleeve(ctx, connect.NewRequest(&pb.KillSleeveRequest{Name: name})); err != nil {
		t.Fatal(err)
	}
	expect("sleeve.exited " + name)
	if _, err := sl.GetSleeve(ctx, connect.NewRequest(&pb.GetSleeveRequest{Name: name})); connect.CodeOf(err) != connect.CodeNotFound {
		t.Errorf("get after kill: %v", err)
	}
	if _, err := sl.KillSleeve(ctx, connect.NewRequest(&pb.KillSleeveRequest{Name: name})); connect.CodeOf(err) != connect.CodeNotFound {
		t.Errorf("second kill: %v", err)
	}
	if rows := h.sleeveRows(); len(rows) != 0 {
		t.Errorf("rows after kill: %v", rows)
	}
}
