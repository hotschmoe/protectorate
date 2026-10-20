package hub

import (
	"context"
	"crypto/rand"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/desk"
	"github.com/hotschmoe/protectorate/internal/jobtest"
	"github.com/hotschmoe/protectorate/internal/node"
)

const agentToken = "agent-secret"

type jobEnv struct {
	h      *Hub
	home   string
	jc     protectoratev1connect.JobServiceClient
	lc     protectoratev1connect.LeaseServiceClient
	srv    *httptest.Server
	remote *node.Jobs
	// peerRoot and hubRoot are the file roots of "peer" and "hubhost".
	peerRoot, hubRoot string
	peerDesk, hubDesk *fakeDesk
	// peerGuard enforces desk policy on "peer" under peerClock.
	peerGuard *node.DeskGuard
	peerClock *clock
	// tn is the hub's tailnet administration; uninstalled is set once the
	// peer agent ran its uninstall.
	tn          *recordingAdmin
	uninstalled atomic.Bool

	invDir, agentAddr string
	local             *node.Jobs
	hubF              *node.Files
	stop              func()
}

// newJobEnv wires a hub (node "hubhost") to one remote node agent ("peer") on
// loopback and waits until both are fresh.
func newJobEnv(t *testing.T, peerSlots int) *jobEnv {
	t.Helper()
	home := t.TempDir()
	t.Setenv("PROTECTORATE_HOME", home)
	remote, err := node.NewJobs(filepath.Join(t.TempDir(), "peer-jobs"), peerSlots)
	if err != nil {
		t.Fatal(err)
	}
	local, err := node.NewJobs(filepath.Join(t.TempDir(), "local-jobs"), 1)
	if err != nil {
		t.Fatal(err)
	}
	peerFiles, err := node.NewFiles(filepath.Join(t.TempDir(), "peer-files"), false)
	if err != nil {
		t.Fatal(err)
	}
	hubFiles, err := node.NewFiles(filepath.Join(t.TempDir(), "hub-files"), false)
	if err != nil {
		t.Fatal(err)
	}
	peerDesk, hubDesk := &fakeDesk{}, &fakeDesk{}
	peerClock := &clock{t: time.Date(2026, 10, 20, 9, 0, 0, 0, time.UTC)}
	peerGuard := &node.DeskGuard{D: peerDesk, Now: peerClock.now, Screen: func() desk.ScreenState { return desk.ScreenUnlocked }, Lock: func() error { return nil }, Input: desk.NewTracker(peerClock.now)}
	peerClock.advance(24 * time.Hour)
	e := &jobEnv{home: home, remote: remote, peerRoot: peerFiles.Root(), hubRoot: hubFiles.Root(), peerDesk: peerDesk, hubDesk: hubDesk, peerGuard: peerGuard, peerClock: peerClock, tn: newRecordingAdmin()}
	agent := &node.Agent{
		Collector: node.NewCollector(node.Options{DockerHost: "unix:///nonexistent.sock"}),
		Authorize: node.TokenAuthorizer(agentToken),
		Jobs:      remote,
		Files:     peerFiles,
		Desk:      &node.Desk{D: peerDesk, Guard: peerGuard},
		Uninstaller: func() ([]string, []string, func(), error) {
			return []string{"unregister the startup"}, []string{"/home/peer/.local/bin/protectorate"}, func() { e.uninstalled.Store(true) }, nil
		},
	}
	agentSrv := httptest.NewServer(agent.Handler())
	t.Cleanup(agentSrv.Close)

	e.invDir, e.agentAddr, e.local = t.TempDir(), strings.TrimPrefix(agentSrv.URL, "http://"), local
	inv, err := OpenInventory(e.invDir, testSelf)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := inv.Add(&pb.NodeInfo{Name: "peer", Address: e.agentAddr}); err != nil {
		t.Fatal(err)
	}
	e.hubF = hubFiles
	e.bootHub(t)
	t.Cleanup(func() { e.stop() })
	return e
}

// bootHub starts a hub on e.home, wired to the peer agent, and waits until
// both nodes are fresh. The previous hub, if any, must have been stopped.
func (e *jobEnv) bootHub(t *testing.T) {
	t.Helper()
	inv, err := OpenInventory(e.invDir, testSelf)
	if err != nil {
		t.Fatal(err)
	}
	h := New("test", testToken)
	h.SetNodes(NewNodeMonitor(inv, testSelf, node.NewCollector(node.Options{DockerHost: "unix:///nonexistent.sock"}),
		NodeOptions{Interval: 100 * time.Millisecond, Token: agentToken}))
	h.EnableNodeOps(e.hubF, &node.Desk{D: e.hubDesk})
	if err := h.EnableJobs(e.home, e.local); err != nil {
		t.Fatal(err)
	}
	h.SetTailnetAdmin(e.tn)
	srv := httptest.NewServer(h.Handler())

	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { defer close(done); h.nodes.Run(ctx) }()
	leasesDone := make(chan struct{})
	go func() { defer close(leasesDone); h.leases.Run(ctx) }()
	var once sync.Once
	e.stop = func() {
		once.Do(func() {
			cancel()
			<-done
			<-leasesDone
			h.jobs.Shutdown()
			srv.Close()
		})
	}

	hc := &http.Client{Transport: bearer{testToken}}
	e.h, e.srv = h, srv
	e.jc = protectoratev1connect.NewJobServiceClient(hc, srv.URL)
	e.lc = protectoratev1connect.NewLeaseServiceClient(hc, srv.URL)
	waitUntil(t, 10*time.Second, "nodes fresh", func() bool {
		for _, n := range []string{"hubhost", "peer"} {
			s, err := h.nodes.Status(n)
			if err != nil || s.Freshness != pb.NodeFreshness_NODE_FRESHNESS_FRESH {
				return false
			}
		}
		return true
	})
}

func waitUntil(t *testing.T, d time.Duration, what string, cond func() bool) {
	t.Helper()
	deadline := time.Now().Add(d)
	for time.Now().Before(deadline) {
		if cond() {
			return
		}
		time.Sleep(20 * time.Millisecond)
	}
	t.Fatalf("timed out waiting for %s", what)
}

func (e *jobEnv) submit(t *testing.T, req *pb.SubmitJobRequest) *pb.Job {
	t.Helper()
	res, err := e.jc.SubmitJob(context.Background(), connect.NewRequest(req))
	if err != nil {
		t.Fatalf("submit: %v", err)
	}
	return res.Msg
}

func (e *jobEnv) get(t *testing.T, id string) *pb.Job {
	t.Helper()
	res, err := e.jc.GetJob(context.Background(), connect.NewRequest(&pb.GetJobRequest{Id: id}))
	if err != nil {
		t.Fatal(err)
	}
	return res.Msg
}

func (e *jobEnv) wait(t *testing.T, id string) *pb.Job {
	t.Helper()
	var j *pb.Job
	waitUntil(t, 30*time.Second, "job "+id+" to end", func() bool {
		j = e.get(t, id)
		return terminal(j.State)
	})
	return j
}

func (e *jobEnv) logOf(t *testing.T, id string) string {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join(e.home, "jobs", id, "job.log"))
	if err != nil {
		t.Fatal(err)
	}
	return string(raw)
}

func onNode(name string) *pb.JobTarget {
	return &pb.JobTarget{Target: &pb.JobTarget_Node{Node: name}}
}

func TestMain(m *testing.M) {
	jobtest.Run()
	jobtest.QuietGit()
	os.Exit(m.Run())
}

func gitIn(t *testing.T, dir string, args ...string) string {
	t.Helper()
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git is not installed")
	}
	cmd := exec.Command("git", append([]string{"-c", "user.email=t@t", "-c", "user.name=t", "-c", "core.autocrlf=false"}, args...)...)
	cmd.Dir = dir
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
	return strings.TrimSpace(string(out))
}

func writeFile(t *testing.T, path string, data []byte, mode os.FileMode) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, data, mode); err != nil {
		t.Fatal(err)
	}
}

func randomBytes(t *testing.T, n int) []byte {
	t.Helper()
	b := make([]byte, n)
	if _, err := rand.Read(b); err != nil {
		t.Fatal(err)
	}
	return b
}

func gitSource(repo, commit string) *pb.JobSource {
	return &pb.JobSource{Kind: &pb.JobSource_Git{Git: &pb.GitSource{Repo: repo, Commit: commit}}}
}

func TestGitJobWarmCacheAndIncrementalBundle(t *testing.T) {
	e := newJobEnv(t, 1)
	repo := t.TempDir()
	gitIn(t, repo, "init", "-q")
	writeFile(t, filepath.Join(repo, "hello.txt"), []byte("one\n"), 0o644)
	writeFile(t, filepath.Join(repo, "big.bin"), randomBytes(t, 600<<10), 0o644)
	gitIn(t, repo, "add", ".")
	gitIn(t, repo, "commit", "-qm", "c1")
	c1 := gitIn(t, repo, "rev-parse", "HEAD")

	run := func(commit string) *pb.Job {
		j := e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Source: gitSource(repo, commit), Command: jobtest.Script("cat:hello.txt", "append:cache.txt:built", "lines:cache.txt")})
		return e.wait(t, j.Id)
	}

	first := run("HEAD")
	if first.State != pb.JobState_JOB_STATE_SUCCEEDED || first.Node != "peer" {
		t.Fatalf("first = %v", first)
	}
	if first.GetSource().GetGit().Commit != c1 {
		t.Fatalf("commit not resolved to full id: %v", first.Source)
	}
	if first.SourceBytes < 600<<10 || first.WorkspaceReused {
		t.Fatalf("first run must ship the full bundle into a cold worktree: bytes=%d reused=%v", first.SourceBytes, first.WorkspaceReused)
	}
	if got := e.logOf(t, first.Id); got != "one\n1\n" {
		t.Fatalf("first log = %q", got)
	}

	second := run(c1)
	if second.State != pb.JobState_JOB_STATE_SUCCEEDED || second.SourceBytes != 0 || !second.WorkspaceReused {
		t.Fatalf("second run must reuse the worktree without sending anything: bytes=%d reused=%v state=%v", second.SourceBytes, second.WorkspaceReused, second.State)
	}
	if got := e.logOf(t, second.Id); got != "one\n2\n" {
		t.Fatalf("untracked cache.txt did not survive between jobs: %q", got)
	}

	writeFile(t, filepath.Join(repo, "hello.txt"), []byte("two\n"), 0o644)
	gitIn(t, repo, "commit", "-qam", "c2")
	third := run("HEAD")
	if third.State != pb.JobState_JOB_STATE_SUCCEEDED || !third.WorkspaceReused {
		t.Fatalf("third = %v", third)
	}
	if third.SourceBytes == 0 || third.SourceBytes > 20<<10 {
		t.Fatalf("incremental bundle should be tiny, got %d bytes (first was %d)", third.SourceBytes, first.SourceBytes)
	}
	if got := e.logOf(t, third.Id); got != "two\n3\n" {
		t.Fatalf("third log = %q", got)
	}

	if _, err := os.Stat(filepath.Join(e.remote.Dir(), "jobs", first.Id)); !os.IsNotExist(err) {
		t.Fatalf("job scratch dir must be removed, stat err = %v", err)
	}
	persisted, err := os.ReadFile(filepath.Join(e.home, "jobs", third.Id, "job.json"))
	if err != nil || !strings.Contains(string(persisted), `"SUCCEEDED"`) && !strings.Contains(string(persisted), "JOB_STATE_SUCCEEDED") {
		t.Fatalf("job.json = %s, %v", persisted, err)
	}
}

func toolExe(t *testing.T, dir string) string {
	t.Helper()
	exe, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	name := "tool"
	if runtime.GOOS == "windows" {
		name += ".exe"
	}
	dst := filepath.Join(dir, name)
	data, err := os.ReadFile(exe)
	if err != nil {
		t.Fatal(err)
	}
	writeFile(t, dst, data, 0o755)
	return dst
}

func TestArtifactJobAndArtifactUpload(t *testing.T) {
	e := newJobEnv(t, 1)
	bin := toolExe(t, t.TempDir())
	if fi, _ := os.Stat(bin); fi.Size() < 2<<20 {
		t.Fatalf("test binary too small to span several upload frames: %d", fi.Size())
	}

	j := e.submit(t, &pb.SubmitJobRequest{
		Target: onNode("peer"), Source: &pb.JobSource{Kind: &pb.JobSource_Artifact{Artifact: &pb.ArtifactSource{Path: bin}}},
		Command:       append([]string{"./" + filepath.Base(bin)}, jobtest.Args("say:artifact ran with arg1", "floodfile:out/big.bin:3000000", "write:out/sub/small.txt:hi", "write:other.txt:skip")...),
		ArtifactGlobs: []string{"out"},
	})
	j = e.wait(t, j.Id)
	if j.State != pb.JobState_JOB_STATE_SUCCEEDED {
		t.Fatalf("job = %v\n%s", j, e.logOf(t, j.Id))
	}
	if got := e.logOf(t, j.Id); got != "artifact ran with arg1\n" {
		t.Fatalf("log = %q", got)
	}
	if j.SourceBytes < 2<<20 {
		t.Fatalf("source bytes = %d", j.SourceBytes)
	}
	if len(j.Artifacts) != 2 || j.ArtifactBytes != 3000000+3 {
		t.Fatalf("artifacts = %v (%d bytes)", j.Artifacts, j.ArtifactBytes)
	}
	big, err := os.Stat(filepath.Join(e.home, "jobs", j.Id, "artifacts", "out", "big.bin"))
	if err != nil || big.Size() != 3000000 {
		t.Fatalf("big.bin = %v, %v", big, err)
	}
	small, _ := os.ReadFile(filepath.Join(e.home, "jobs", j.Id, "artifacts", "out", "sub", "small.txt"))
	if string(small) != "hi\n" {
		t.Fatalf("small.txt = %q", small)
	}
	if _, err := os.Stat(filepath.Join(e.home, "jobs", j.Id, "artifacts", "other.txt")); err == nil {
		t.Fatal("unmatched file was sent back")
	}
}

func TestJobOnHubsOwnNode(t *testing.T) {
	e := newJobEnv(t, 1)
	j := e.wait(t, e.submit(t, &pb.SubmitJobRequest{Target: onNode("hubhost"), Command: jobtest.Script("say:local", "env:PROTECTORATE_JOB_ID")}).Id)
	if j.State != pb.JobState_JOB_STATE_SUCCEEDED || j.Node != "hubhost" {
		t.Fatalf("job = %v", j)
	}
	if got := e.logOf(t, j.Id); got != "local\n"+j.Id+"\n" {
		t.Fatalf("log = %q", got)
	}
}

func TestJobFailureAndEnv(t *testing.T) {
	e := newJobEnv(t, 1)
	j := e.wait(t, e.submit(t, &pb.SubmitJobRequest{
		Target: onNode("peer"), Command: jobtest.Script("env:SECRET_VALUE", "exit:7"), Env: map[string]string{"SECRET_VALUE": "s3cret"},
	}).Id)
	if j.State != pb.JobState_JOB_STATE_FAILED || j.ExitCode != 7 || j.Error != "exit status 7" {
		t.Fatalf("job = %v", j)
	}
	if len(j.EnvKeys) != 1 || j.EnvKeys[0] != "SECRET_VALUE" {
		t.Fatalf("env keys = %v", j.EnvKeys)
	}
	raw, _ := os.ReadFile(filepath.Join(e.home, "jobs", j.Id, "job.json"))
	if strings.Contains(string(raw), "s3cret") {
		t.Fatal("env values must not be persisted in job.json")
	}

	missing := e.wait(t, e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: []string{"definitely-not-a-command"}}).Id)
	if missing.State != pb.JobState_JOB_STATE_FAILED || !strings.Contains(missing.Error, "definitely-not-a-command") {
		t.Fatalf("missing command = %v", missing)
	}
}

func TestJobCancelKillsProcessTree(t *testing.T) {
	e := newJobEnv(t, 1)
	j := e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: jobtest.Script("spawn:300", "wait")})
	var childPID int
	waitUntil(t, 10*time.Second, "child pid in log", func() bool {
		line := strings.TrimSpace(e.logOf(t, j.Id))
		pid, err := strconv.Atoi(strings.TrimPrefix(line, "child="))
		childPID = pid
		return strings.HasPrefix(line, "child=") && err == nil
	})
	if e.get(t, j.Id).State != pb.JobState_JOB_STATE_RUNNING {
		t.Fatalf("state = %v", e.get(t, j.Id).State)
	}
	if !jobtest.Alive(childPID) {
		t.Fatal("grandchild is not running before cancel")
	}

	start := time.Now()
	res, err := e.jc.CancelJob(context.Background(), connect.NewRequest(&pb.CancelJobRequest{Id: j.Id}))
	if err != nil {
		t.Fatal(err)
	}
	if res.Msg.State != pb.JobState_JOB_STATE_CANCELED {
		t.Fatalf("after cancel = %v", res.Msg)
	}
	if time.Since(start) > 8*time.Second {
		t.Fatalf("cancel took %s", time.Since(start))
	}
	waitUntil(t, 5*time.Second, "grandchild to die", func() bool { return !jobtest.Alive(childPID) })

	again, err := e.jc.CancelJob(context.Background(), connect.NewRequest(&pb.CancelJobRequest{Id: j.Id}))
	if err != nil || again.Msg.State != pb.JobState_JOB_STATE_CANCELED {
		t.Fatalf("second cancel = %v, %v", again, err)
	}
}

func TestJobTimeout(t *testing.T) {
	e := newJobEnv(t, 1)
	start := time.Now()
	j := e.wait(t, e.submit(t, &pb.SubmitJobRequest{
		Target: onNode("peer"), Command: jobtest.Script("say:started", "sleep:300"), Timeout: durationpb.New(700 * time.Millisecond),
	}).Id)
	if j.State != pb.JobState_JOB_STATE_TIMED_OUT || !strings.Contains(j.Error, "timed out") {
		t.Fatalf("job = %v", j)
	}
	if time.Since(start) > 10*time.Second {
		t.Fatalf("timeout took %s", time.Since(start))
	}
	if got := e.logOf(t, j.Id); got != "started\n" {
		t.Fatalf("log = %q", got)
	}
}

func TestNodeConcurrencyLimit(t *testing.T) {
	e := newJobEnv(t, 1)
	first := e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: jobtest.Script("say:a", "sleep:1.5")})
	waitUntil(t, 10*time.Second, "first running", func() bool { return e.get(t, first.Id).State == pb.JobState_JOB_STATE_RUNNING })
	second := e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: jobtest.Script("say:b")})
	time.Sleep(500 * time.Millisecond)
	if st := e.get(t, second.Id).State; st != pb.JobState_JOB_STATE_PREPARING {
		t.Fatalf("second must wait for the slot, state = %v", st)
	}
	f, s := e.wait(t, first.Id), e.wait(t, second.Id)
	if s.State != pb.JobState_JOB_STATE_SUCCEEDED || s.StartedAt.AsTime().Before(f.FinishedAt.AsTime().Add(-100*time.Millisecond)) {
		t.Fatalf("second started at %v before first finished at %v", s.StartedAt.AsTime(), f.FinishedAt.AsTime())
	}
}

func TestConcurrencyTwoSlots(t *testing.T) {
	e := newJobEnv(t, 2)
	a := e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: jobtest.Script("sleep:1.5")})
	b := e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: jobtest.Script("sleep:1.5")})
	waitUntil(t, 10*time.Second, "both running", func() bool {
		return e.get(t, a.Id).State == pb.JobState_JOB_STATE_RUNNING && e.get(t, b.Id).State == pb.JobState_JOB_STATE_RUNNING
	})
	e.wait(t, a.Id)
	e.wait(t, b.Id)
}

func TestSamRepoJobsSerialize(t *testing.T) {
	e := newJobEnv(t, 2)
	repo := t.TempDir()
	gitIn(t, repo, "init", "-q")
	writeFile(t, filepath.Join(repo, "f"), []byte("x"), 0o644)
	gitIn(t, repo, "add", ".")
	gitIn(t, repo, "commit", "-qm", "c1")
	req := func() *pb.SubmitJobRequest {
		return &pb.SubmitJobRequest{Target: onNode("peer"), Source: gitSource(repo, ""), Command: jobtest.Script("appendpid:pids", "sleep:0.5", "appendpid:pids", "lines:pids")}
	}
	a, b := e.submit(t, req()), e.submit(t, req())
	ja, jb := e.wait(t, a.Id), e.wait(t, b.Id)
	la, lb := strings.TrimSpace(e.logOf(t, ja.Id)), strings.TrimSpace(e.logOf(t, jb.Id))
	if !(la == "2" && lb == "4") && !(la == "4" && lb == "2") {
		t.Fatalf("jobs on one worktree overlapped: logs %q and %q", la, lb)
	}
}

func TestWatchJobReplayAndFollow(t *testing.T) {
	e := newJobEnv(t, 1)
	j := e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: jobtest.Script("say:one", "sleep:0.5", "say:two")})
	stream, err := e.jc.WatchJob(context.Background(), connect.NewRequest(&pb.WatchJobRequest{Id: j.Id, Follow: true}))
	if err != nil {
		t.Fatal(err)
	}
	var log strings.Builder
	var states []pb.JobState
	for stream.Receive() {
		switch ev := stream.Msg().Event.(type) {
		case *pb.WatchJobResponse_Job:
			states = append(states, ev.Job.State)
		case *pb.WatchJobResponse_Log:
			log.Write(ev.Log.Data)
		}
	}
	if err := stream.Err(); err != nil {
		t.Fatal(err)
	}
	if log.String() != "one\ntwo\n" || states[len(states)-1] != pb.JobState_JOB_STATE_SUCCEEDED {
		t.Fatalf("log = %q, states = %v", log.String(), states)
	}

	stream, err = e.jc.WatchJob(context.Background(), connect.NewRequest(&pb.WatchJobRequest{Id: j.Id, FromOffset: 4}))
	if err != nil {
		t.Fatal(err)
	}
	log.Reset()
	for stream.Receive() {
		if l := stream.Msg().GetLog(); l != nil {
			log.Write(l.Data)
		}
	}
	if log.String() != "two\n" {
		t.Fatalf("replay from offset = %q", log.String())
	}

	_, err = e.jc.GetJob(context.Background(), connect.NewRequest(&pb.GetJobRequest{Id: "nope"}))
	if connect.CodeOf(err) != connect.CodeNotFound {
		t.Fatalf("unknown job code = %v", connect.CodeOf(err))
	}
}

func TestSubmitValidation(t *testing.T) {
	e := newJobEnv(t, 1)
	tests := []struct {
		name string
		req  *pb.SubmitJobRequest
		want connect.Code
	}{
		{"no command", &pb.SubmitJobRequest{Target: onNode("peer")}, connect.CodeInvalidArgument},
		{"no target", &pb.SubmitJobRequest{Command: []string{"true"}}, connect.CodeInvalidArgument},
		{"unknown node", &pb.SubmitJobRequest{Target: onNode("ghost"), Command: []string{"true"}}, connect.CodeNotFound},
		{"unknown lease", &pb.SubmitJobRequest{Target: &pb.JobTarget{Target: &pb.JobTarget_LeaseId{LeaseId: "lease-0"}}, Command: []string{"true"}}, connect.CodeNotFound},
		{"huge timeout", &pb.SubmitJobRequest{Target: onNode("peer"), Command: []string{"true"}, Timeout: durationpb.New(48 * time.Hour)}, connect.CodeInvalidArgument},
		{"bad glob", &pb.SubmitJobRequest{Target: onNode("peer"), Command: []string{"true"}, ArtifactGlobs: []string{"../x"}}, connect.CodeInvalidArgument},
		{"missing artifact", &pb.SubmitJobRequest{Target: onNode("peer"), Command: []string{"true"}, Source: &pb.JobSource{Kind: &pb.JobSource_Artifact{Artifact: &pb.ArtifactSource{Path: "/nonexistent/x"}}}}, connect.CodeInvalidArgument},
		{"workdir escape", &pb.SubmitJobRequest{Target: onNode("peer"), Command: []string{"true"}, Source: &pb.JobSource{Kind: &pb.JobSource_Artifact{Artifact: &pb.ArtifactSource{Path: ".", Workdir: "../x"}}}}, connect.CodeInvalidArgument},
		{"workdir on file", &pb.SubmitJobRequest{Target: onNode("peer"), Command: []string{"true"}, Source: &pb.JobSource{Kind: &pb.JobSource_Artifact{Artifact: &pb.ArtifactSource{Path: "../../go.mod", Workdir: "x"}}}}, connect.CodeInvalidArgument},
		{"not a repo", &pb.SubmitJobRequest{Target: onNode("peer"), Command: []string{"true"}, Source: gitSource(t.TempDir(), "")}, connect.CodeInvalidArgument},
		{"option injection as rev", &pb.SubmitJobRequest{Target: onNode("peer"), Command: []string{"true"}, Source: gitSource(".", "--output=/tmp/x")}, connect.CodeInvalidArgument},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := e.jc.SubmitJob(context.Background(), connect.NewRequest(tt.req))
			if connect.CodeOf(err) != tt.want {
				t.Fatalf("code = %v (%v), want %v", connect.CodeOf(err), err, tt.want)
			}
		})
	}
}

func (e *jobEnv) restart(t *testing.T) {
	t.Helper()
	e.stop()
	e.bootHub(t)
}

func TestJobSurvivesHubRestart(t *testing.T) {
	e := newJobEnv(t, 1)
	j := e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: jobtest.Script("say:up", "sleep:2", "say:down", "exit:3")})
	waitUntil(t, 10*time.Second, "first output", func() bool { return strings.Contains(e.logOf(t, j.Id), "up") })

	e.restart(t)
	if got := e.get(t, j.Id); terminal(got.State) {
		t.Fatalf("job ended with the hub: %v", got)
	}
	var offsets []uint64
	var watched strings.Builder
	stream, err := e.jc.WatchJob(context.Background(), connect.NewRequest(&pb.WatchJobRequest{Id: j.Id, FromOffset: 3, Follow: true}))
	if err != nil {
		t.Fatal(err)
	}
	var last *pb.Job
	for stream.Receive() {
		if l := stream.Msg().GetLog(); l != nil {
			offsets = append(offsets, l.Offset)
			watched.Write(l.Data)
		}
		if jj := stream.Msg().GetJob(); jj != nil {
			last = jj
		}
	}
	if err := stream.Err(); err != nil {
		t.Fatal(err)
	}
	if watched.String() != "up\ndown\n"[3:] || offsets[0] != 3 {
		t.Fatalf("watch from offset 3 got %q at %v", watched.String(), offsets)
	}
	if last == nil || last.State != pb.JobState_JOB_STATE_FAILED || last.ExitCode != 3 || last.Error != "exit status 3" {
		t.Fatalf("final record = %v", last)
	}
	if got := e.logOf(t, j.Id); got != "up\ndown\n" {
		t.Fatalf("log across restart = %q", got)
	}
	if last.StartedAt == nil || last.StartedAt.AsTime().After(last.FinishedAt.AsTime()) {
		t.Fatalf("timestamps = %v %v", last.StartedAt, last.FinishedAt)
	}
}

func TestRecoveredJobCanBeCanceled(t *testing.T) {
	e := newJobEnv(t, 1)
	j := e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: jobtest.Script("say:up", "sleep:300")})
	waitUntil(t, 10*time.Second, "first output", func() bool { return strings.Contains(e.logOf(t, j.Id), "up") })
	e.restart(t)
	waitUntil(t, 10*time.Second, "reattached", func() bool {
		e.h.jobs.mu.Lock()
		defer e.h.jobs.mu.Unlock()
		je := e.h.jobs.jobs[j.Id]
		je.mu.Lock()
		defer je.mu.Unlock()
		return je.streaming
	})
	res, err := e.jc.CancelJob(context.Background(), connect.NewRequest(&pb.CancelJobRequest{Id: j.Id}))
	if err != nil {
		t.Fatal(err)
	}
	if res.Msg.State != pb.JobState_JOB_STATE_CANCELED {
		t.Fatalf("after cancel = %v", res.Msg)
	}
	waitUntil(t, 10*time.Second, "process gone", func() bool { return !e.remote.Cancel(j.Id) })
}

func TestRestartReconcilesRecordsWithNodes(t *testing.T) {
	now := time.Now()
	tests := []struct {
		name     string
		node     string
		state    pb.JobState
		age      time.Duration
		wantLost string
	}{
		{"node reachable and never had the job", "peer", pb.JobState_JOB_STATE_RUNNING, 0, "no record of the job"},
		{"job still preparing, node never got it", "peer", pb.JobState_JOB_STATE_PREPARING, 0, "no record of the job"},
		{"node no longer in the inventory", "gone", pb.JobState_JOB_STATE_RUNNING, 0, "no longer known"},
		{"node unreachable past the job's deadline", "ghost", pb.JobState_JOB_STATE_RUNNING, 3 * time.Hour, "stayed unreachable"},
		{"node unreachable within the deadline", "ghost", pb.JobState_JOB_STATE_RUNNING, time.Minute, ""},
	}
	e := newJobEnv(t, 1)
	e.stop()
	inv, err := OpenInventory(e.invDir, testSelf)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := inv.Add(&pb.NodeInfo{Name: "ghost", Address: "127.0.0.1:1"}); err != nil {
		t.Fatal(err)
	}
	ids := make([]string, len(tests))
	for i, tt := range tests {
		ids[i] = "2026010" + strconv.Itoa(i+1) + "-000000-deadbeef"
		jd := filepath.Join(e.home, "jobs", ids[i])
		if err := os.MkdirAll(jd, 0o700); err != nil {
			t.Fatal(err)
		}
		j := &pb.Job{Id: ids[i], State: tt.state, Node: tt.node, Timeout: durationpb.New(time.Hour), SubmittedAt: timestamppb.New(now.Add(-tt.age))}
		if err := writeJob(jd, j); err != nil {
			t.Fatal(err)
		}
		writeFile(t, filepath.Join(jd, "job.log"), []byte("partial"), 0o600)
	}
	e.bootHub(t)
	for i, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if tt.wantLost == "" {
				time.Sleep(300 * time.Millisecond)
				if got := e.get(t, ids[i]); terminal(got.State) {
					t.Fatalf("job given up on too early: %v", got)
				}
				return
			}
			got := e.wait(t, ids[i])
			if got.State != pb.JobState_JOB_STATE_FAILED || !strings.Contains(got.Error, tt.wantLost) || got.LogBytes != 7 {
				t.Fatalf("record = %v, want failed containing %q", got, tt.wantLost)
			}
		})
	}
}

func TestShutdownLeavesRemoteJobsRunningAndStopsLocalOnes(t *testing.T) {
	e := newJobEnv(t, 1)
	remote := e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: jobtest.Script("say:up", "sleep:300")})
	localJob := e.submit(t, &pb.SubmitJobRequest{Target: onNode("hubhost"), Command: jobtest.Script("say:up", "sleep:300")})
	for _, id := range []string{remote.Id, localJob.Id} {
		waitUntil(t, 10*time.Second, "running", func() bool { return e.get(t, id).State == pb.JobState_JOB_STATE_RUNNING })
	}
	e.h.jobs.Shutdown()
	if got := e.get(t, remote.Id); got.State != pb.JobState_JOB_STATE_RUNNING {
		t.Fatalf("remote job after shutdown = %v", got)
	}
	if got := e.get(t, localJob.Id); got.State != pb.JobState_JOB_STATE_FAILED || got.Error != "hub stopped" {
		t.Fatalf("local job after shutdown = %v", got)
	}
	if listed := e.remote.List(); len(listed) != 1 || listed[0].Finished != nil {
		t.Fatalf("node dropped the job with the hub: %v", listed)
	}
	e.remote.Cancel(remote.Id)
}

func TestJobEventsAndScopes(t *testing.T) {
	e := newJobEnv(t, 1)
	sub, err := protectoratev1connect.NewEventServiceClient(&http.Client{Transport: bearer{testToken}}, e.srv.URL).
		Subscribe(context.Background(), connect.NewRequest(&pb.SubscribeRequest{}))
	if err != nil {
		t.Fatal(err)
	}
	defer sub.Close()
	j := e.wait(t, e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: jobtest.Script()}).Id)
	seen := map[string]bool{}
	for sub.Receive() {
		ev := sub.Msg()
		if ev.Kind == "job.updated" && ev.Subject == j.Id {
			seen[ev.Payload.Fields["state"].GetStringValue()] = true
			if seen["JOB_STATE_SUCCEEDED"] {
				break
			}
		}
	}
	if !seen["JOB_STATE_PREPARING"] || !seen["JOB_STATE_RUNNING"] || !seen["JOB_STATE_SUCCEEDED"] {
		t.Fatalf("job.updated states = %v", seen)
	}
}

func TestJobAndLeaseScopesAndAudit(t *testing.T) {
	e := newJobEnv(t, 1)
	store, err := auth.OpenStore(filepath.Join(e.home, "tokens.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	auditDir := filepath.Join(e.home, "audit")
	au := auth.NewAudit(auditDir)
	t.Cleanup(func() { au.Close() })
	e.h.SetAuth(auth.New(testToken, store, au))
	srv := httptest.NewServer(e.h.Handler())
	t.Cleanup(srv.Close)

	mint := func(name string, scopes ...string) *http.Client {
		_, secret, err := store.Create(name, scopes, time.Hour, time.Now())
		if err != nil {
			t.Fatal(err)
		}
		return &http.Client{Transport: bearer{secret}}
	}
	ctx := context.Background()
	ttl := durationpb.New(time.Minute)
	clients := map[string]*http.Client{
		"owner": {Transport: bearer{testToken}}, "read": mint("read", "read"),
		"jobs": mint("jobs", "job_*"), "leases": mint("leases", "lease_*"), "submit": mint("submit", "job_submit_job"),
	}
	type calls struct {
		submit, get, list, cancel, watch, acquire, renew, release, listLeases error
	}
	run := func(c *http.Client, leaseID, jobID string) calls {
		jc := protectoratev1connect.NewJobServiceClient(c, srv.URL)
		lc := protectoratev1connect.NewLeaseServiceClient(c, srv.URL)
		var r calls
		_, r.submit = jc.SubmitJob(ctx, connect.NewRequest(&pb.SubmitJobRequest{Target: onNode("peer"), Command: []string{"true"}}))
		_, r.get = jc.GetJob(ctx, connect.NewRequest(&pb.GetJobRequest{Id: jobID}))
		_, r.list = jc.ListJobs(ctx, connect.NewRequest(&pb.ListJobsRequest{}))
		_, r.cancel = jc.CancelJob(ctx, connect.NewRequest(&pb.CancelJobRequest{Id: jobID}))
		st, err := jc.WatchJob(ctx, connect.NewRequest(&pb.WatchJobRequest{Id: jobID}))
		if err == nil {
			for st.Receive() {
			}
			err = st.Err()
		}
		r.watch = err
		_, r.acquire = lc.AcquireLease(ctx, connect.NewRequest(&pb.AcquireLeaseRequest{Target: &pb.AcquireLeaseRequest_Resource{Resource: "peer"}, Holder: "h", Ttl: ttl}))
		_, r.renew = lc.RenewLease(ctx, connect.NewRequest(&pb.RenewLeaseRequest{Id: leaseID}))
		_, r.release = lc.ReleaseLease(ctx, connect.NewRequest(&pb.ReleaseLeaseRequest{Id: "lease-ffffffff"}))
		_, r.listLeases = lc.ListLeases(ctx, connect.NewRequest(&pb.ListLeasesRequest{}))
		return r
	}
	seed := e.wait(t, e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: jobtest.Script()}).Id)
	l, err := e.lc.AcquireLease(ctx, connect.NewRequest(&pb.AcquireLeaseRequest{Target: &pb.AcquireLeaseRequest_Resource{Resource: "hubhost"}, Holder: "seed", Ttl: durationpb.New(time.Hour)}))
	if err != nil {
		t.Fatal(err)
	}

	denied := connect.CodePermissionDenied
	tests := []struct {
		who  string
		want map[string]connect.Code
	}{
		{"read", map[string]connect.Code{"submit": denied, "cancel": denied, "acquire": denied, "renew": denied, "release": denied}},
		{"jobs", map[string]connect.Code{"acquire": denied, "renew": denied, "release": denied, "listLeases": denied}},
		{"leases", map[string]connect.Code{"submit": denied, "get": denied, "list": denied, "cancel": denied, "watch": denied}},
		{"submit", map[string]connect.Code{"get": denied, "list": denied, "cancel": denied, "watch": denied, "acquire": denied, "renew": denied, "release": denied, "listLeases": denied}},
	}
	for _, tt := range tests {
		t.Run(tt.who, func(t *testing.T) {
			got := run(clients[tt.who], l.Msg.Id, seed.Id)
			for name, err := range map[string]error{
				"submit": got.submit, "get": got.get, "list": got.list, "cancel": got.cancel, "watch": got.watch,
				"acquire": got.acquire, "renew": got.renew, "release": got.release, "listLeases": got.listLeases,
			} {
				if want, isDenied := tt.want[name]; isDenied {
					if code(err) != want {
						t.Errorf("%s: err = %v, want denied", name, err)
					}
				} else if code(err) == denied || code(err) == connect.CodeUnauthenticated {
					t.Errorf("%s: unexpectedly refused: %v", name, err)
				}
			}
		})
	}

	entries, err := auth.Tail(auditDir, 1000)
	if err != nil {
		t.Fatal(err)
	}
	seen := map[string]bool{}
	for _, en := range entries {
		seen[en.Procedure[strings.LastIndex(en.Procedure, "/")+1:]] = true
	}
	for _, m := range []string{"SubmitJob", "CancelJob", "AcquireLease", "RenewLease", "ReleaseLease"} {
		if !seen[m] {
			t.Errorf("%s is not audited", m)
		}
	}
	for _, m := range []string{"GetJob", "ListJobs", "WatchJob", "ListLeases"} {
		if seen[m] {
			t.Errorf("read RPC %s is audited", m)
		}
	}

	res, err := http.Post(srv.URL+"/protectorate.v1.NodeAgentService/RunJob", "application/json", strings.NewReader("{}"))
	if err != nil {
		t.Fatal(err)
	}
	res.Body.Close()
	if res.StatusCode != http.StatusNotFound {
		t.Fatalf("the hub must not serve the node agent service, got %d", res.StatusCode)
	}
}

func TestWarmNodes(t *testing.T) {
	m := &JobManager{jobs: map[string]*jobEntry{}}
	add := func(id, node, repo string, started bool) {
		j := &pb.Job{Id: id, Node: node, Source: gitSource(repo, "abc")}
		if started {
			j.StartedAt = timestamppb.Now()
		}
		m.jobs[id] = &jobEntry{node: node, job: j}
	}
	add("1", "a", "/r1", true)
	add("2", "b", "/r1", false)
	add("3", "c", "/r2", true)
	m.jobs["4"] = &jobEntry{node: "d", job: &pb.Job{Id: "4"}}
	got := m.warmNodes("/r1")
	if len(got) != 1 || !got["a"] {
		t.Fatalf("warm nodes = %v: only a started job leaves a worktree behind", got)
	}
	if m.warmNodes("") != nil {
		t.Fatal("jobs without a repo have no warm nodes")
	}
}

func TestArtifactDirectoryRunsInWorkdirAndKeepsModes(t *testing.T) {
	e := newJobEnv(t, 1)
	root := t.TempDir()
	writeFile(t, filepath.Join(root, "go.mod"), []byte("module x\n"), 0o644)
	writeFile(t, filepath.Join(root, "pkg", "testdata", "in.txt"), []byte("data\n"), 0o644)
	tool := toolExe(t, filepath.Join(root, "pkg"))

	j := e.submit(t, &pb.SubmitJobRequest{
		Target: onNode("peer"), Source: &pb.JobSource{Kind: &pb.JobSource_Artifact{Artifact: &pb.ArtifactSource{Path: root, Workdir: "pkg"}}},
		Command:       append([]string{"./" + filepath.Base(tool)}, jobtest.Args("cat:testdata/in.txt", "pwd", "write:out/tool:x", "chmod:out/tool:755", "write:out/plain:x")...),
		ArtifactGlobs: []string{"out"},
	})
	j = e.wait(t, j.Id)
	if j.State != pb.JobState_JOB_STATE_SUCCEEDED {
		t.Fatalf("job = %v\n%s", j, e.logOf(t, j.Id))
	}
	log := e.logOf(t, j.Id)
	if !strings.HasPrefix(log, "data\n") || !strings.HasSuffix(filepath.ToSlash(log), "/work/pkg\n") {
		t.Fatalf("log = %q", log)
	}
	for name, want := range map[string]os.FileMode{"tool": 0o755, "plain": 0o644} {
		fi, err := os.Stat(filepath.Join(e.home, "jobs", j.Id, "artifacts", "out", name))
		if err != nil || (runtime.GOOS != "windows" && fi.Mode().Perm()&0o111 != want&0o111) {
			t.Fatalf("%s = %v, %v; want mode %v", name, fi, err, want)
		}
	}
}

func TestSubmitRequestIDsSurviveHubRestart(t *testing.T) {
	e := newJobEnv(t, 1)
	const kept, canceledFirst = "request-persist-0001", "request-persist-0002"
	first := e.submit(t, &pb.SubmitJobRequest{RequestId: kept, Target: onNode("peer"), Command: jobtest.Script("say:once")})
	e.wait(t, first.Id)
	if _, err := e.jc.CancelJob(context.Background(), connect.NewRequest(&pb.CancelJobRequest{RequestId: canceledFirst})); connect.CodeOf(err) != connect.CodeNotFound {
		t.Fatalf("cancel before submit: %v", err)
	}

	e.restart(t)
	again := e.submit(t, &pb.SubmitJobRequest{RequestId: kept, Target: onNode("peer"), Command: jobtest.Script("say:once")})
	if again.Id != first.Id {
		t.Fatalf("request id forgotten across the restart: %s then %s", first.Id, again.Id)
	}
	if n := len(e.h.jobs.List(pb.JobState_JOB_STATE_UNSPECIFIED, 0)); n != 1 {
		t.Fatalf("%d jobs after a deduped resubmit", n)
	}
	_, err := e.jc.SubmitJob(context.Background(), connect.NewRequest(&pb.SubmitJobRequest{RequestId: canceledFirst, Target: onNode("peer"), Command: jobtest.Script("say:late")}))
	if connect.CodeOf(err) != connect.CodeAborted {
		t.Fatalf("a submit canceled by its key before it arrived must stay refused after a restart: %v", err)
	}
}
