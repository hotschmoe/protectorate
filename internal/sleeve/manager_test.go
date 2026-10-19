//go:build linux

package sleeve

import (
	"context"
	"errors"
	"fmt"
	"math/rand"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/hotschmoe/protectorate/internal/session"
)

type recorder struct {
	mu     sync.Mutex
	events []string
}

func (r *recorder) emit(kind, subject string, _ map[string]any) {
	r.mu.Lock()
	r.events = append(r.events, kind+" "+subject)
	r.mu.Unlock()
}

func (r *recorder) has(e string) bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	for _, v := range r.events {
		if v == e {
			return true
		}
	}
	return false
}

type rig struct {
	m    *Manager
	cfg  Config
	sess *session.Manager
	rec  *recorder
	pfx  string
}

func newRig(t *testing.T) *rig {
	t.Helper()
	home := t.TempDir()
	cfg, err := DefaultConfig(home, nil)
	if err != nil {
		t.Fatal(err)
	}
	id := fmt.Sprintf("%06d", rand.Intn(1000000))
	cfg.UnitPrefix = "prot-test-" + id + "-"
	if err := cfg.Preflight(); err != nil {
		t.Skipf("sleeve runtime unavailable: %v", err)
	}
	sock := "prot-sleeve-test-" + id
	r := &rig{cfg: cfg, sess: session.New(sock), rec: &recorder{}, pfx: "t" + id}
	t.Cleanup(func() {
		for _, n := range r.cfg.scopeNames() {
			if strings.HasPrefix(n, r.pfx) {
				r.cfg.killScope(context.Background(), n)
			}
		}
		exec.Command("tmux", "-L", sock, "kill-server").Run()
	})
	r.open(t)
	return r
}

func (r *rig) open(t *testing.T) {
	t.Helper()
	m, err := Open(context.Background(), Options{
		Config: r.cfg, Sessions: r.sess, Emit: r.rec.emit, UsageInterval: 150 * time.Millisecond,
	})
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { defer close(done); m.Run(ctx) }()
	t.Cleanup(func() { cancel(); <-done })
	r.m = m
}

func (r *rig) workspace(t *testing.T, name string) string {
	t.Helper()
	ws := filepath.Join(r.cfg.Home, "workspaces", r.pfx+name)
	if err := os.MkdirAll(ws, 0o755); err != nil {
		t.Fatal(err)
	}
	return ws
}

func (r *rig) spawn(t *testing.T, name string, lim Limits, args ...string) (Info, string) {
	t.Helper()
	ws := r.workspace(t, name)
	info, err := r.m.Spawn(context.Background(), SpawnRequest{Name: r.pfx + name, Workspace: ws, Harness: Shell, Args: args, Limits: lim})
	if err != nil {
		t.Fatalf("spawn %s: %v", name, err)
	}
	return info, ws
}

func waitFor(t *testing.T, what string, f func() bool) {
	t.Helper()
	deadline := time.Now().Add(15 * time.Second)
	for !f() {
		if time.Now().After(deadline) {
			t.Fatalf("timed out waiting for %s", what)
		}
		time.Sleep(25 * time.Millisecond)
	}
}

func readFile(path string) string {
	b, _ := os.ReadFile(path)
	return strings.TrimSpace(string(b))
}

func scopePids(t *testing.T, c Config, name string) []string {
	t.Helper()
	return strings.Fields(readFile(filepath.Join(c.scopeDir(name), "cgroup.procs")))
}

func alive(pid string) bool {
	n, _ := strconv.Atoi(pid)
	return n > 0 && syscallKill0(n) == nil
}

func TestSleeveLifecycle(t *testing.T) {
	r := newRig(t)
	ctx := context.Background()

	hostSleep := exec.Command("sleep", "4243")
	if err := hostSleep.Start(); err != nil {
		t.Fatal(err)
	}
	defer func() { hostSleep.Process.Kill(); hostSleep.Wait() }()

	t0 := time.Now()
	bInfo, _ := r.spawn(t, "b", Limits{}, "sleep", "4242")
	t.Logf("spawn latency (b): %s", time.Since(t0))
	if bInfo.State != Running || bInfo.Session != "sleeve-"+r.pfx+"b" {
		t.Fatalf("info = %+v", bInfo)
	}
	bName := r.pfx + "b"
	bPids := scopePids(t, r.cfg, bName)
	if len(bPids) == 0 {
		t.Fatal("b has no processes in its scope")
	}

	script := fmt.Sprintf(`echo hello-from-sleeve; ps -eo pid,args > ps.out; pkill -9 -f '[s]leep 4242'; echo $? > pkill.rc; `+
		`kill -9 %s %d 2> kill.err; echo $? > kill.rc; sleep 600`, strings.Join(bPids, " "), hostSleep.Process.Pid)
	aInfo, aWS := r.spawn(t, "a", Limits{CPUPercent: 100}, "sh", "-c", script)
	aName := aInfo.Name

	waitFor(t, "a's isolation probe", func() bool {
		if readFile(filepath.Join(aWS, "kill.rc")) != "" {
			return true
		}
		if time.Since(t0) > 10*time.Second {
			cap, err := r.sess.Capture(ctx, aInfo.Session, 50)
			t.Logf("pane: %q %v", cap, err)
		}
		return false
	})
	ps := readFile(filepath.Join(aWS, "ps.out"))
	for _, line := range strings.Split(ps, "\n") {
		if strings.HasSuffix(line, " sleep 4242") || strings.HasSuffix(line, " sleep 4243") {
			t.Errorf("a can see another process: %q\n%s", line, ps)
		}
	}
	if !strings.Contains(ps, "ps -eo") {
		t.Errorf("ps output looks empty:\n%s", ps)
	}
	if got := readFile(filepath.Join(aWS, "pkill.rc")); got != "1" {
		t.Errorf("pkill rc = %q, want 1 (nothing matched)", got)
	}
	if got := readFile(filepath.Join(aWS, "kill.rc")); got == "0" {
		t.Errorf("kill of foreign pids succeeded: %s", readFile(filepath.Join(aWS, "kill.err")))
	}
	for _, p := range bPids {
		if !alive(p) {
			t.Errorf("b's process %s was killed", p)
		}
	}
	if err := hostSleep.Process.Signal(syscallSignal0); err != nil {
		t.Errorf("host process was killed: %v", err)
	}

	text, err := r.sess.Capture(ctx, aInfo.Session, 50)
	if err != nil || !strings.Contains(text, "hello-from-sleeve") {
		t.Errorf("capture = %q, %v", text, err)
	}

	waitFor(t, "usage sample", func() bool {
		i, _ := r.m.Get(aName)
		return i.Usage.Pids > 0 && i.Usage.MemCurrent > 0 && i.Usage.MemPeak > 0 && i.Usage.DiskSampled.After(time.Time{})
	})
	if !r.rec.has("sleeve.spawned " + aName) {
		t.Errorf("no spawned event: %v", r.rec.events)
	}

	if _, err := r.m.Spawn(ctx, SpawnRequest{Name: aName, Workspace: aWS, Harness: Shell}); !errors.Is(err, ErrExists) {
		t.Errorf("duplicate spawn err = %v", err)
	}

	raw, err := os.ReadFile(filepath.Join(r.cfg.Home, "sleeves.yaml"))
	if err != nil || !strings.Contains(string(raw), aName) {
		t.Errorf("sleeves.yaml = %q, %v", raw, err)
	}

	// hub restart: a fresh manager recovers both from yaml, tmux and scopes
	r.open(t)
	if i, err := r.m.Get(aName); err != nil || i.State != Running || i.Usage.MemCurrent == 0 {
		t.Fatalf("recovered a = %+v, %v", i, err)
	}

	// session killed behind the manager's back: reconcile marks it exited
	if err := r.sess.Kill(ctx, bInfo.Session); err != nil {
		t.Fatal(err)
	}
	waitFor(t, "b exited", func() bool { i, _ := r.m.Get(bName); return i.State == Exited })
	if i, _ := r.m.Get(bName); i.ExitCode != nil {
		t.Errorf("exit code %d invented for a session killed from outside", *i.ExitCode)
	}
	if r.cfg.scopeAlive(bName) {
		t.Error("b's scope survived its session")
	}
	if !r.rec.has("sleeve.exited " + bName) {
		t.Errorf("no exited event: %v", r.rec.events)
	}
	for _, p := range bPids {
		waitFor(t, "b process gone", func() bool { return !alive(p) })
	}
	if _, err := r.m.Spawn(ctx, SpawnRequest{Name: bName, Workspace: r.workspace(t, "b"), Harness: Shell, Args: []string{"sleep", "4242"}}); err != nil {
		t.Errorf("respawn of exited sleeve: %v", err)
	}

	for _, n := range []string{aName, bName} {
		if err := r.m.Kill(ctx, n); err != nil {
			t.Fatalf("kill %s: %v", n, err)
		}
		if r.cfg.scopeAlive(n) {
			t.Errorf("scope %s alive after kill", n)
		}
		if _, err := r.sess.Get(ctx, SessionName(n)); !errors.Is(err, session.ErrNotFound) {
			t.Errorf("session %s after kill: %v", n, err)
		}
		if _, err := r.m.Get(n); !errors.Is(err, ErrNotFound) {
			t.Errorf("get after kill: %v", err)
		}
	}
	if _, err := os.Stat(aWS); err != nil {
		t.Errorf("workspace gone after kill: %v", err)
	}
	if err := r.m.Kill(ctx, aName); !errors.Is(err, ErrNotFound) {
		t.Errorf("second kill = %v", err)
	}
}

func TestMemoryLimitEnforced(t *testing.T) {
	r := newRig(t)
	info, ws := r.spawn(t, "hog", Limits{MemoryBytes: 128 << 20}, "sh", "-c", `tail /dev/zero; echo $? > hog.rc; sleep 600`)
	waitFor(t, "hog to be killed", func() bool { return readFile(filepath.Join(ws, "hog.rc")) != "" })
	if rc := readFile(filepath.Join(ws, "hog.rc")); rc != "137" {
		t.Errorf("hog exit = %s, want 137 (SIGKILL by the OOM killer)", rc)
	}
	if !r.cfg.scopeAlive(info.Name) {
		t.Error("OOM took down the whole scope; OOMPolicy=continue not effective")
	}
	waitFor(t, "oom accounting", func() bool { i, _ := r.m.Get(info.Name); return i.Usage.OOMKills > 0 })
	waitFor(t, "oom event", func() bool { return r.rec.has("sleeve.oom " + info.Name) })
	i, _ := r.m.Get(info.Name)
	if i.Usage.MemPeak > 160<<20 {
		t.Errorf("peak %d exceeds limit by too much", i.Usage.MemPeak)
	}
	if err := r.m.Kill(context.Background(), info.Name); err != nil {
		t.Fatal(err)
	}
}

func TestSleeveExitKeepsSession(t *testing.T) {
	r := newRig(t)
	ctx := context.Background()
	info, ws := r.spawn(t, "crash", Limits{}, "sh", "-c", "echo crash-output; sleep 1; exit 3")
	waitFor(t, "exit", func() bool { i, _ := r.m.Get(info.Name); return i.State == Exited })

	i, _ := r.m.Get(info.Name)
	if i.ExitCode == nil || *i.ExitCode != 3 || i.ExitSignal != 0 || i.OOMKilled || i.Exited.IsZero() {
		t.Fatalf("info = %+v", i)
	}
	waitFor(t, "exited event", func() bool { return r.rec.has("sleeve.exited " + info.Name) })
	if r.cfg.scopeAlive(info.Name) {
		t.Error("scope alive after exit")
	}
	s, err := r.sess.Get(ctx, info.Session)
	if err != nil || s.State != session.Exited || s.ExitCode != 3 {
		t.Fatalf("session = %+v, %v", s, err)
	}
	text, err := r.sess.Capture(ctx, info.Session, 50)
	if err != nil || !strings.Contains(text, "crash-output") {
		t.Fatalf("capture = %q, %v", text, err)
	}

	r.open(t)
	if got, _ := r.m.Get(info.Name); got.State != Exited || got.ExitCode == nil || *got.ExitCode != 3 {
		t.Fatalf("exit info lost across restart: %+v", got)
	}

	again, err := r.m.Spawn(ctx, SpawnRequest{Name: info.Name, Workspace: ws, Harness: Shell, Args: []string{"sleep", "600"}})
	if err != nil {
		t.Fatalf("respawn over dead session: %v", err)
	}
	if again.State != Running || again.ExitCode != nil || again.Workspace != info.Workspace {
		t.Fatalf("respawned = %+v", again)
	}
	if s, err := r.sess.Get(ctx, info.Session); err != nil || s.State != session.Running {
		t.Fatalf("session after respawn = %+v, %v", s, err)
	}

	if err := r.m.Kill(ctx, info.Name); err != nil {
		t.Fatal(err)
	}
	dead, _ := r.spawn(t, "crash2", Limits{}, "sh", "-c", "sleep 1; exit 1")
	waitFor(t, "second exit", func() bool { i, _ := r.m.Get(dead.Name); return i.State == Exited })
	if err := r.m.Kill(ctx, dead.Name); err != nil {
		t.Fatal(err)
	}
	if _, err := r.sess.Get(ctx, dead.Session); !errors.Is(err, session.ErrNotFound) {
		t.Errorf("kill of an exited sleeve left its dead session: %v", err)
	}
}

func TestSleeveExitReasons(t *testing.T) {
	r := newRig(t)
	tests := []struct {
		name       string
		limits     Limits
		script     string
		wantCode   int
		wantSignal int
		wantOOM    bool
	}{
		{"oom killed", Limits{MemoryBytes: 128 << 20}, "tail /dev/zero; rc=$?; sleep 1; exit $rc", 137, 0, true},
		{"sigkill without oom", Limits{MemoryBytes: 128 << 20}, "sleep 1; kill -9 $$", 137, 0, false},
		{"plain failure", Limits{}, "sleep 1; exit 2", 2, 0, false},
	}
	for i, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			info, _ := r.spawn(t, fmt.Sprintf("r%d", i), tt.limits, "sh", "-c", tt.script)
			waitFor(t, "exit", func() bool { i, _ := r.m.Get(info.Name); return i.State == Exited })
			got, _ := r.m.Get(info.Name)
			if got.ExitCode == nil || *got.ExitCode != tt.wantCode || got.ExitSignal != tt.wantSignal || got.OOMKilled != tt.wantOOM {
				t.Fatalf("info = %+v (exit code %v)", got, got.ExitCode)
			}
			if err := r.m.Kill(context.Background(), info.Name); err != nil {
				t.Fatal(err)
			}
		})
	}
}

func TestSpawnClonesGitURL(t *testing.T) {
	r := newRig(t)
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git not installed")
	}
	src := filepath.Join(r.cfg.Home, "src")
	for _, args := range [][]string{{"init", "-q", src}, {"-C", src, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "x"}} {
		if out, err := exec.Command("git", args...).CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
	}
	name := r.pfx + "g"
	info, err := r.m.Spawn(context.Background(), SpawnRequest{Name: name, Workspace: "file://" + src, Harness: Shell, Args: []string{"sleep", "600"}})
	if err != nil {
		t.Fatal(err)
	}
	if want := filepath.Join(r.cfg.Home, "workspaces", name); info.Workspace != want {
		t.Errorf("workspace = %s, want %s", info.Workspace, want)
	}
	if _, err := os.Stat(filepath.Join(info.Workspace, ".git")); err != nil {
		t.Error(err)
	}
	if err := r.m.Kill(context.Background(), name); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(info.Workspace); err != nil {
		t.Errorf("clone removed by kill: %v", err)
	}
}

func TestSpawnRejectsBadRequests(t *testing.T) {
	r := newRig(t)
	ws := r.workspace(t, "x")
	tests := []struct {
		name string
		req  SpawnRequest
	}{
		{"no harness", SpawnRequest{Name: "ok", Workspace: ws}},
		{"relative ws", SpawnRequest{Name: "ok", Workspace: "rel", Harness: Shell}},
		{"missing ws", SpawnRequest{Name: "ok", Workspace: ws + "/nope", Harness: Shell}},
		{"home ws", SpawnRequest{Name: "ok", Workspace: r.cfg.UserHome, Harness: Shell}},
		{"bad name", SpawnRequest{Name: "Bad", Workspace: ws, Harness: Shell}},
		{"dash url", SpawnRequest{Name: "ok", Workspace: "-x://y", Harness: Shell}},
	}
	for _, tt := range tests {
		if _, err := r.m.Spawn(context.Background(), tt.req); !errors.Is(err, ErrInvalid) {
			t.Errorf("%s: err = %v, want ErrInvalid", tt.name, err)
		}
	}
	if got := r.m.List(); len(got) != 0 {
		t.Errorf("failed spawns left records: %v", got)
	}
}

func TestKillRestoresRecordWhenSaveFails(t *testing.T) {
	r := newRig(t)
	info, _ := r.spawn(t, "ks", Limits{}, "sleep", "600")
	blocker := filepath.Join(r.cfg.Home, "blocker")
	if err := os.WriteFile(blocker, nil, 0o600); err != nil {
		t.Fatal(err)
	}
	good := r.m.path
	r.m.path = filepath.Join(blocker, "sleeves.yaml")
	if err := r.m.Kill(context.Background(), info.Name); err == nil {
		t.Fatal("Kill succeeded with an unwritable store")
	}
	if _, err := r.m.Get(info.Name); err != nil {
		t.Fatalf("record lost after failed save: %v", err)
	}
	r.m.path = good
	if err := r.m.Kill(context.Background(), info.Name); err != nil {
		t.Fatalf("second Kill: %v", err)
	}
	if _, err := r.m.Get(info.Name); !errors.Is(err, ErrNotFound) {
		t.Fatalf("Get after Kill = %v", err)
	}
}

func TestFailedStartLeavesNothingRunning(t *testing.T) {
	r := newRig(t)
	ws := r.workspace(t, "fs")
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	name := r.pfx + "fs"
	if _, err := r.m.Spawn(ctx, SpawnRequest{Name: name, Workspace: ws, Harness: Shell, Args: []string{"sleep", "600"}}); err == nil {
		t.Fatal("Spawn succeeded with a canceled context")
	}
	if _, err := r.sess.Get(context.Background(), SessionName(name)); !errors.Is(err, session.ErrNotFound) {
		t.Errorf("session left behind: %v", err)
	}
	if r.cfg.scopeAlive(name) {
		t.Errorf("scope %s left running", name)
	}
	if _, err := r.m.Get(name); !errors.Is(err, ErrNotFound) {
		t.Errorf("record left behind: %v", err)
	}
}
