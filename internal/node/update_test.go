package node

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"reflect"
	"sync"
	"testing"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

func TestSwapAndRollbackPlans(t *testing.T) {
	tests := []struct {
		goos     string
		swap     []fileOp
		rollback []fileOp
	}{
		{"windows",
			[]fileOp{{opRename, `C:\p\p.exe`, `C:\p\p.exe.old`}, {opRename, `C:\p\p.exe.new`, `C:\p\p.exe`}},
			[]fileOp{{opRename, `C:\p\p.exe`, `C:\p\p.exe.failed`}, {opRename, `C:\p\p.exe.old`, `C:\p\p.exe`}}},
		{"linux",
			[]fileOp{{opCopy, "/p/p", "/p/p.old"}, {opRename, "/p/p.new", "/p/p"}},
			[]fileOp{{opRename, "/p/p.old", "/p/p"}}},
		{"android",
			[]fileOp{{opCopy, "/p/p", "/p/p.old"}, {opRename, "/p/p.new", "/p/p"}},
			[]fileOp{{opRename, "/p/p.old", "/p/p"}}},
	}
	for _, tt := range tests {
		t.Run(tt.goos, func(t *testing.T) {
			exe := "/p/p"
			if tt.goos == "windows" {
				exe = `C:\p\p.exe`
			}
			if got := swapPlan(tt.goos, exe, exe+".new"); !reflect.DeepEqual(got, tt.swap) {
				t.Errorf("swap = %v, want %v", got, tt.swap)
			}
			if got := rollbackPlan(tt.goos, exe); !reflect.DeepEqual(got, tt.rollback) {
				t.Errorf("rollback = %v, want %v", got, tt.rollback)
			}
		})
	}
}

func TestDecideStartup(t *testing.T) {
	now := time.Now()
	future, past := now.Add(time.Minute), now.Add(-time.Minute)
	tests := []struct {
		name    string
		st      *updateState
		running string
		want    startup
	}{
		{"no marker", nil, "v2", startPlain},
		{"confirmed", &updateState{Phase: phaseConfirmed, To: "v2"}, "v2", startPlain},
		{"rolled back, old binary", &updateState{Phase: phaseRolledBack, From: "v1", To: "v2"}, "v1", startPlain},
		{"trial, still the old binary", &updateState{Phase: phaseTrial, From: "v1", To: "v2", Deadline: future}, "v1", startPlain},
		{"first run of the new binary", &updateState{Phase: phaseTrial, From: "v1", To: "v2", Deadline: future}, "v2", startTrial},
		{"second start without confirmation", &updateState{Phase: phaseTrial, From: "v1", To: "v2", Attempts: 1, Deadline: future}, "v2", startRollback},
		{"first run after the deadline", &updateState{Phase: phaseTrial, From: "v1", To: "v2", Deadline: past}, "v2", startRollback},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := decideStartup(tt.st, tt.running, now); got != tt.want {
				t.Errorf("got %d, want %d", got, tt.want)
			}
		})
	}
}

func TestPlanRestart(t *testing.T) {
	tests := []struct {
		name string
		s    Supervision
		want RestartKind
	}{
		{"systemd user unit", Supervision{GOOS: "linux", SystemdUnit: true}, RestartSystemd},
		{"system unit", Supervision{GOOS: "linux", Supervised: true}, RestartExit},
		{"windows logon task", Supervision{GOOS: "windows", Task: true}, RestartTask},
		{"windows service", Supervision{GOOS: "windows", Supervised: true, Task: true}, RestartExit},
		{"termux service", Supervision{GOOS: "android", TermuxService: true}, RestartTermux},
		{"hand started linux", Supervision{GOOS: "linux"}, RestartReexec},
		{"hand started windows", Supervision{GOOS: "windows"}, RestartReexec},
		{"systemd flag ignored off linux", Supervision{GOOS: "windows", SystemdUnit: true}, RestartReexec},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := PlanRestart(tt.s); got != tt.want {
				t.Errorf("got %s, want %s", got, tt.want)
			}
		})
	}
}

func TestRunOpsUndoesRenames(t *testing.T) {
	dir := t.TempDir()
	exe, staged := filepath.Join(dir, "p"), filepath.Join(dir, "p.new")
	if err := os.WriteFile(exe, []byte("old"), 0o755); err != nil {
		t.Fatal(err)
	}
	// The windows plan with a missing staged file: exe must come back.
	err := runOps(swapPlan("windows", exe, staged))
	if err == nil {
		t.Fatal("swap of a missing file succeeded")
	}
	if b, rerr := os.ReadFile(exe); rerr != nil || string(b) != "old" {
		t.Fatalf("exe after failed swap = %q, %v", b, rerr)
	}
}

type updateEnv struct {
	t        *testing.T
	exe      string
	root     string
	up       *Updater
	mu       sync.Mutex
	restarts []RestartKind
	version  string
	srv      *httptest.Server
	cl       protectoratev1connect.NodeAgentServiceClient
}

func newUpdateEnv(t *testing.T, running string) *updateEnv {
	t.Helper()
	dir := t.TempDir()
	e := &updateEnv{t: t, exe: filepath.Join(dir, "bin", "protectorate"), version: "v2"}
	if err := os.MkdirAll(filepath.Dir(e.exe), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(e.exe, []byte("old binary"), 0o755); err != nil {
		t.Fatal(err)
	}
	files, err := NewFiles(filepath.Join(dir, "files"), false)
	if err != nil {
		t.Fatal(err)
	}
	e.root = files.Root()
	e.up = &Updater{Files: files, Exe: e.exe, Version: running, Supervision: Supervision{GOOS: "linux"},
		Restart:    func(k RestartKind) { e.mu.Lock(); e.restarts = append(e.restarts, k); e.mu.Unlock() },
		RunVersion: func(context.Context, string) (string, error) { return e.version, nil }}
	agent := &Agent{Collector: NewCollector(Options{DockerHost: "unix:///nonexistent.sock"}), Authorize: TokenAuthorizer("tok"), Files: files, Updater: e.up}
	e.srv = httptest.NewServer(agent.Handler())
	t.Cleanup(e.srv.Close)
	e.cl = protectoratev1connect.NewNodeAgentServiceClient(&http.Client{Transport: tokenRT("tok")}, e.srv.URL)
	return e
}

func (e *updateEnv) stage(content string) *pb.ApplyUpdateRequest {
	e.t.Helper()
	p := filepath.Join(e.root, "updates", "next")
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		e.t.Fatal(err)
	}
	if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
		e.t.Fatal(err)
	}
	sum := sha256.Sum256([]byte(content))
	return &pb.ApplyUpdateRequest{Path: "updates/next", Size: uint64(len(content)), Sha256: hex.EncodeToString(sum[:])}
}

func (e *updateEnv) gotRestarts() []RestartKind {
	e.mu.Lock()
	defer e.mu.Unlock()
	return append([]RestartKind(nil), e.restarts...)
}

func readFile(t *testing.T, p string) string {
	t.Helper()
	b, err := os.ReadFile(p)
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}

func waitFor(t *testing.T, what string, cond func() bool) {
	t.Helper()
	for end := time.Now().Add(5 * time.Second); time.Now().Before(end); time.Sleep(10 * time.Millisecond) {
		if cond() {
			return
		}
	}
	t.Fatalf("timed out waiting for %s", what)
}

func TestApplyUpdateEndToEnd(t *testing.T) {
	ctx := context.Background()
	e := newUpdateEnv(t, "v1")
	req := e.stage("new binary")
	res, err := e.cl.ApplyUpdate(ctx, connect.NewRequest(req))
	if err != nil {
		t.Fatal(err)
	}
	if res.Msg.Version != "v2" || res.Msg.PreviousVersion != "v1" || res.Msg.Restart != string(RestartReexec) {
		t.Fatalf("response = %v", res.Msg)
	}
	if got := readFile(t, e.exe); got != "new binary" {
		t.Errorf("exe = %q", got)
	}
	if got := readFile(t, e.exe+".old"); got != "old binary" {
		t.Errorf(".old = %q", got)
	}
	if st := e.up.load(); st == nil || st.Phase != phaseTrial || st.To != "v2" || st.From != "v1" {
		t.Errorf("marker = %+v", st)
	}
	waitFor(t, "restart request", func() bool { return len(e.gotRestarts()) == 1 })

	if _, err := e.cl.ApplyUpdate(ctx, connect.NewRequest(req)); connect.CodeOf(err) != connect.CodeFailedPrecondition {
		t.Errorf("update during a trial: %v", err)
	}

	t.Run("hub contact confirms the new binary", func(t *testing.T) {
		up := &Updater{Files: e.up.Files, Exe: e.exe, Version: "v2", Supervision: e.up.Supervision, Restart: e.up.Restart}
		if rolled, err := up.Start(); err != nil || rolled {
			t.Fatalf("first start: rolled=%v err=%v", rolled, err)
		}
		if st := up.load(); st.Attempts != 1 || st.Phase != phaseTrial {
			t.Fatalf("marker after start = %+v", st)
		}
		up.Touch()
		if st := up.load(); st.Phase != phaseConfirmed {
			t.Fatalf("marker after contact = %+v", st)
		}
		if rolled, _ := (&Updater{Files: up.Files, Exe: e.exe, Version: "v2", Restart: up.Restart}).Start(); rolled {
			t.Error("a confirmed binary rolled back")
		}
		if got := readFile(t, e.exe); got != "new binary" {
			t.Errorf("exe = %q", got)
		}
	})
}

func TestUnconfirmedUpdateRollsBack(t *testing.T) {
	e := newUpdateEnv(t, "v1")
	if _, err := e.cl.ApplyUpdate(context.Background(), connect.NewRequest(e.stage("new binary"))); err != nil {
		t.Fatal(err)
	}
	waitFor(t, "restart request", func() bool { return len(e.gotRestarts()) == 1 })

	crashed := &Updater{Files: e.up.Files, Exe: e.exe, Version: "v2", Supervision: e.up.Supervision, Restart: e.up.Restart}
	if rolled, err := crashed.Start(); err != nil || rolled {
		t.Fatalf("first start: rolled=%v err=%v", rolled, err)
	}
	// The new binary dies before the hub reaches it; its next start rolls back.
	again := &Updater{Files: e.up.Files, Exe: e.exe, Version: "v2", Supervision: e.up.Supervision, Restart: e.up.Restart}
	rolled, err := again.Start()
	if err != nil || !rolled {
		t.Fatalf("second start: rolled=%v err=%v", rolled, err)
	}
	if got := readFile(t, e.exe); got != "old binary" {
		t.Errorf("exe after rollback = %q", got)
	}
	if st := again.load(); st.Phase != phaseRolledBack || st.Reason == "" {
		t.Errorf("marker = %+v", st)
	}
	if n := len(e.gotRestarts()); n != 2 {
		t.Errorf("restarts = %d, want 2 (update, rollback)", n)
	}
	old := &Updater{Files: e.up.Files, Exe: e.exe, Version: "v1", Restart: e.up.Restart}
	if rolled, _ := old.Start(); rolled {
		t.Error("the old binary rolled itself back")
	}
}

func TestApplyUpdateRefusals(t *testing.T) {
	ctx := context.Background()
	tests := []struct {
		name   string
		mutate func(*updateEnv, *pb.ApplyUpdateRequest)
		want   connect.Code
	}{
		{"wrong hash", func(_ *updateEnv, r *pb.ApplyUpdateRequest) { r.Sha256 = "00" }, connect.CodeFailedPrecondition},
		{"wrong size", func(_ *updateEnv, r *pb.ApplyUpdateRequest) { r.Size++ }, connect.CodeFailedPrecondition},
		{"missing file", func(_ *updateEnv, r *pb.ApplyUpdateRequest) { r.Path = "updates/none" }, connect.CodeNotFound},
		{"outside the file root", func(_ *updateEnv, r *pb.ApplyUpdateRequest) { r.Path = "../x" }, connect.CodePermissionDenied},
		{"binary does not run", func(e *updateEnv, _ *pb.ApplyUpdateRequest) {
			e.up.RunVersion = func(context.Context, string) (string, error) { return "", errors.New("bad exec") }
		}, connect.CodeFailedPrecondition},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			e := newUpdateEnv(t, "v1")
			req := e.stage("new binary")
			tt.mutate(e, req)
			if _, err := e.cl.ApplyUpdate(ctx, connect.NewRequest(req)); connect.CodeOf(err) != tt.want {
				t.Fatalf("err = %v, want %s", err, tt.want)
			}
			if got := readFile(t, e.exe); got != "old binary" {
				t.Errorf("a refused update changed the binary: %q", got)
			}
			if len(e.gotRestarts()) != 0 {
				t.Error("a refused update restarted the node")
			}
		})
	}
}
