package hub

import (
	"context"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/node"
)

func TestPickBinary(t *testing.T) {
	tests := []struct {
		name                   string
		goos, arch, sOS, sArch string
		want                   string
	}{
		{"own platform uses the hub executable", "linux", "amd64", "linux", "amd64", "/opt/protectorate"},
		{"other linux arch", "linux", "arm64", "linux", "amd64", filepath.Join("bin", "protectorate-linux-arm64")},
		{"windows gets .exe", "windows", "amd64", "linux", "amd64", filepath.Join("bin", "protectorate-windows-amd64.exe")},
		{"android", "android", "arm64", "linux", "amd64", filepath.Join("bin", "protectorate-android-arm64")},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := pickBinary("bin", tt.goos, tt.arch, tt.sOS, tt.sArch, "/opt/protectorate"); got != tt.want {
				t.Errorf("got %s, want %s", got, tt.want)
			}
		})
	}
}

func TestNeedsUpdate(t *testing.T) {
	tests := []struct {
		node, hub string
		force     bool
		want      bool
	}{
		{"v1", "v2", false, true},
		{"v2", "v2", false, false},
		{"v2", "v2", true, true},
		{"", "v2", false, true},
	}
	for _, tt := range tests {
		if got, _ := NeedsUpdate(tt.node, tt.hub, tt.force); got != tt.want {
			t.Errorf("NeedsUpdate(%q,%q,%v) = %v", tt.node, tt.hub, tt.force, got)
		}
	}
}

func TestUpdateStep(t *testing.T) {
	const (
		restarting = pb.UpdatePhase_UPDATE_PHASE_RESTARTING
		confirmed  = pb.UpdatePhase_UPDATE_PHASE_CONFIRMED
		rolledBack = pb.UpdatePhase_UPDATE_PHASE_ROLLED_BACK
	)
	tests := []struct {
		name          string
		version       string
		down, sawDown bool
		phase         pb.UpdatePhase
		final         bool
	}{
		{"node still down", "", true, false, restarting, false},
		{"old binary before it restarted", "v1", false, false, restarting, false},
		{"new version", "v2", false, true, confirmed, true},
		{"new version without a missed probe", "v2", false, false, confirmed, true},
		{"old binary after a restart", "v1", false, true, rolledBack, true},
		{"unrelated version", "v9", false, true, restarting, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			phase, final := updateStep("v2", "v1", tt.version, tt.down, tt.sawDown)
			if phase != tt.phase || final != tt.final {
				t.Errorf("got %s %v, want %s %v", phase, final, tt.phase, tt.final)
			}
		})
	}
}

type swapHandler struct{ h atomic.Pointer[http.Handler] }

func (s *swapHandler) set(h http.Handler) { s.h.Store(&h) }
func (s *swapHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	(*s.h.Load()).ServeHTTP(w, r)
}

type updateEnv struct {
	h   *Hub
	nc  protectoratev1connect.NodeServiceClient
	cur *swapHandler
	// next is what the agent turns into when it restarts.
	next atomic.Pointer[http.Handler]
	exe  string
}

func newUpdateEnv(t *testing.T, hubVersion string) *updateEnv {
	t.Helper()
	t.Setenv("PROTECTORATE_HOME", t.TempDir())
	dir := t.TempDir()
	files, err := node.NewFiles(filepath.Join(dir, "files"), false)
	if err != nil {
		t.Fatal(err)
	}
	e := &updateEnv{cur: &swapHandler{}, exe: filepath.Join(dir, "agent-bin")}
	if err := os.WriteFile(e.exe, []byte("old binary"), 0o755); err != nil {
		t.Fatal(err)
	}
	agentAt := func(version string, up *node.Updater) http.Handler {
		return (&node.Agent{
			Collector: node.NewCollector(node.Options{DockerHost: "unix:///nonexistent.sock", Version: version}),
			Authorize: node.TokenAuthorizer(agentToken),
			Files:     files,
			Updater:   up,
		}).Handler()
	}
	up := &node.Updater{Files: files, Exe: e.exe, Version: "v1", Supervision: node.Supervision{GOOS: runtime.GOOS},
		RunVersion: func(context.Context, string) (string, error) { return "v2", nil },
		Restart: func(node.RestartKind) {
			if n := e.next.Load(); n != nil {
				e.cur.set(*n)
			}
		}}
	e.cur.set(agentAt("v1", up))
	newer := agentAt("v2", nil)
	e.next.Store(&newer)
	srv := httptest.NewServer(e.cur)
	t.Cleanup(srv.Close)

	hubBin := filepath.Join(dir, "hub-bin")
	if err := os.WriteFile(hubBin, []byte("hub build for this platform"), 0o755); err != nil {
		t.Fatal(err)
	}
	inv, err := OpenInventory(t.TempDir(), testSelf)
	if err != nil {
		t.Fatal(err)
	}
	for _, n := range []*pb.NodeInfo{
		{Name: "peer", Address: strings.TrimPrefix(srv.URL, "http://"), Os: runtime.GOOS, Arch: runtime.GOARCH},
	} {
		if _, err := inv.Add(n); err != nil {
			t.Fatal(err)
		}
	}
	e.h = New(hubVersion, testToken)
	e.h.SelfExe, e.h.BinaryDir = hubBin, filepath.Join(dir, "empty-bin-dir")
	e.h.UpdateTimeout, e.h.updatePoll = 3*time.Second, 20*time.Millisecond
	e.h.SetNodes(NewNodeMonitor(inv, testSelf, node.NewCollector(node.Options{DockerHost: "unix:///nonexistent.sock"}),
		NodeOptions{Interval: 50 * time.Millisecond, Token: agentToken}))
	hs := httptest.NewServer(e.h.Handler())
	t.Cleanup(hs.Close)
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { defer close(done); e.h.nodes.Run(ctx) }()
	t.Cleanup(func() { cancel(); <-done })
	e.nc = protectoratev1connect.NewNodeServiceClient(&http.Client{Transport: bearer{testToken}}, hs.URL)
	waitUntil(t, 10*time.Second, "peer fresh", func() bool {
		s, err := e.h.nodes.Status("peer")
		return err == nil && s.Freshness == pb.NodeFreshness_NODE_FRESHNESS_FRESH && s.Metrics != nil && s.Metrics.AgentVersion == "v1"
	})
	return e
}

func (e *updateEnv) run(t *testing.T, req *pb.UpdateNodeRequest) ([]*pb.UpdateNodeProgress, error) {
	t.Helper()
	stream, err := e.nc.UpdateNode(context.Background(), connect.NewRequest(req))
	if err != nil {
		return nil, err
	}
	defer stream.Close()
	var got []*pb.UpdateNodeProgress
	for stream.Receive() {
		got = append(got, stream.Msg())
	}
	return got, stream.Err()
}

func phases(ps []*pb.UpdateNodeProgress) []pb.UpdatePhase {
	var out []pb.UpdatePhase
	for _, p := range ps {
		if len(out) == 0 || out[len(out)-1] != p.Phase {
			out = append(out, p.Phase)
		}
	}
	return out
}

func TestUpdateNodeStreamsProgress(t *testing.T) {
	const (
		sending    = pb.UpdatePhase_UPDATE_PHASE_SENDING
		verifying  = pb.UpdatePhase_UPDATE_PHASE_VERIFYING
		swapping   = pb.UpdatePhase_UPDATE_PHASE_SWAPPING
		restarting = pb.UpdatePhase_UPDATE_PHASE_RESTARTING
		confirmed  = pb.UpdatePhase_UPDATE_PHASE_CONFIRMED
		rolledBack = pb.UpdatePhase_UPDATE_PHASE_ROLLED_BACK
	)
	t.Run("confirmed", func(t *testing.T) {
		e := newUpdateEnv(t, "v2")
		got, err := e.run(t, &pb.UpdateNodeRequest{Name: "peer"})
		if err != nil {
			t.Fatal(err)
		}
		want := []pb.UpdatePhase{sending, verifying, swapping, restarting, confirmed}
		if g := phases(got); !equalPhases(g, want) {
			t.Fatalf("phases = %v, want %v", g, want)
		}
		last := got[len(got)-1]
		if last.Version != "v2" || last.PreviousVersion != "v1" {
			t.Errorf("final = %v", last)
		}
		sent := got[1]
		if sent.TotalBytes != uint64(len("hub build for this platform")) || sent.SentBytes != sent.TotalBytes {
			t.Errorf("sending progress = %v", sent)
		}
		if b, err := os.ReadFile(e.exe); err != nil || string(b) != "hub build for this platform" {
			t.Errorf("node binary = %q, %v", b, err)
		}
	})
	t.Run("already current is skipped", func(t *testing.T) {
		e := newUpdateEnv(t, "v1")
		got, err := e.run(t, &pb.UpdateNodeRequest{Name: "peer"})
		if err != nil || len(got) != 1 || got[0].Phase != pb.UpdatePhase_UPDATE_PHASE_SKIPPED {
			t.Fatalf("got %v, %v", got, err)
		}
		if b, _ := os.ReadFile(e.exe); string(b) != "old binary" {
			t.Errorf("a skipped update touched the binary: %q", b)
		}
	})
	t.Run("rolled back", func(t *testing.T) {
		e := newUpdateEnv(t, "v2")
		old := *e.cur.h.Load()
		var downUntil atomic.Int64
		back := http.Handler(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if downUntil.CompareAndSwap(0, time.Now().Add(300*time.Millisecond).UnixNano()) || time.Now().UnixNano() < downUntil.Load() {
				http.Error(w, "restarting", http.StatusServiceUnavailable)
				return
			}
			old.ServeHTTP(w, r)
		}))
		e.next.Store(&back)
		got, err := e.run(t, &pb.UpdateNodeRequest{Name: "peer"})
		if err != nil {
			t.Fatal(err)
		}
		if g := phases(got); g[len(g)-1] != rolledBack {
			t.Fatalf("phases = %v, want to end rolled back", g)
		}
	})
	t.Run("refusals", func(t *testing.T) {
		e := newUpdateEnv(t, "v2")
		tests := []struct {
			name string
			node string
			want connect.Code
		}{
			{"hub's own node", "hubhost", connect.CodeFailedPrecondition},
			{"unknown node", "ghost", connect.CodeNotFound},
		}
		for _, tt := range tests {
			t.Run(tt.name, func(t *testing.T) {
				if _, err := e.run(t, &pb.UpdateNodeRequest{Name: tt.node}); connect.CodeOf(err) != tt.want {
					t.Fatalf("err = %v, want %s", err, tt.want)
				}
			})
		}
		t.Run("no binary to send", func(t *testing.T) {
			e.h.SelfExe = filepath.Join(t.TempDir(), "missing")
			if _, err := e.run(t, &pb.UpdateNodeRequest{Name: "peer"}); connect.CodeOf(err) != connect.CodeFailedPrecondition {
				t.Fatalf("err = %v", err)
			}
		})
	})
}

func equalPhases(a, b []pb.UpdatePhase) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
