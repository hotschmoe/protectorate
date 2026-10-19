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
	"testing"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/auth"
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
}

// newJobEnv wires a hub (node "hubhost") to one remote node agent ("peer") on
// loopback and waits until both are fresh.
func newJobEnv(t *testing.T, peerSlots int) *jobEnv {
	t.Helper()
	if runtime.GOOS == "windows" {
		t.Skip("job tests use sh")
	}
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
	agent := &node.Agent{
		Collector: node.NewCollector(node.Options{DockerHost: "unix:///nonexistent.sock"}),
		Authorize: node.TokenAuthorizer(agentToken),
		Jobs:      remote,
	}
	agentSrv := httptest.NewServer(agent.Handler())
	t.Cleanup(agentSrv.Close)

	inv, err := OpenInventory(t.TempDir(), testSelf)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := inv.Add(&pb.NodeInfo{Name: "peer", Address: strings.TrimPrefix(agentSrv.URL, "http://")}); err != nil {
		t.Fatal(err)
	}
	h := New("test", testToken)
	h.SetNodes(NewNodeMonitor(inv, testSelf, node.NewCollector(node.Options{DockerHost: "unix:///nonexistent.sock"}),
		NodeOptions{Interval: 100 * time.Millisecond, Token: agentToken}))
	if err := h.EnableJobs(home, local); err != nil {
		t.Fatal(err)
	}
	srv := httptest.NewServer(h.Handler())
	t.Cleanup(srv.Close)

	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { defer close(done); h.nodes.Run(ctx) }()
	leasesDone := make(chan struct{})
	go func() { defer close(leasesDone); h.leases.Run(ctx) }()
	t.Cleanup(func() {
		cancel()
		<-done
		<-leasesDone
		h.jobs.Shutdown()
	})

	hc := &http.Client{Transport: bearer{testToken}}
	e := &jobEnv{h: h, home: home, srv: srv, remote: remote,
		jc: protectoratev1connect.NewJobServiceClient(hc, srv.URL),
		lc: protectoratev1connect.NewLeaseServiceClient(hc, srv.URL)}
	waitUntil(t, 10*time.Second, "nodes fresh", func() bool {
		for _, n := range []string{"hubhost", "peer"} {
			s, err := h.nodes.Status(n)
			if err != nil || s.Freshness != pb.NodeFreshness_NODE_FRESHNESS_FRESH {
				return false
			}
		}
		return true
	})
	return e
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

func sh(script string) []string { return []string{"sh", "-c", script} }

func gitIn(t *testing.T, dir string, args ...string) string {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-c", "user.email=t@t", "-c", "user.name=t"}, args...)...)
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

	script := "cat hello.txt; echo built >> cache.txt; wc -l < cache.txt"
	run := func(commit string) *pb.Job {
		j := e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Source: gitSource(repo, commit), Command: sh(script)})
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

func TestArtifactJobAndArtifactUpload(t *testing.T) {
	e := newJobEnv(t, 1)
	bin := filepath.Join(t.TempDir(), "tool.sh")
	pad := strings.Repeat("# padding to span several upload frames\n", 60000)
	writeFile(t, bin, []byte("#!/bin/sh\necho artifact ran with $1\nmkdir -p out/sub\nhead -c 3000000 /dev/zero > out/big.bin\necho hi > out/sub/small.txt\necho skip > other.txt\n"+pad), 0o644)
	if fi, _ := os.Stat(bin); fi.Size() < 2<<20 {
		t.Fatalf("test file too small: %d", fi.Size())
	}

	j := e.submit(t, &pb.SubmitJobRequest{
		Target: onNode("peer"), Source: &pb.JobSource{Kind: &pb.JobSource_Artifact{Artifact: &pb.ArtifactSource{Path: bin}}},
		Command: []string{"./tool.sh", "arg1"}, ArtifactGlobs: []string{"out"},
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
	j := e.wait(t, e.submit(t, &pb.SubmitJobRequest{Target: onNode("hubhost"), Command: sh("echo local; echo $PROTECTORATE_JOB_ID")}).Id)
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
		Target: onNode("peer"), Command: sh("echo $SECRET_VALUE; exit 7"), Env: map[string]string{"SECRET_VALUE": "s3cret"},
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
	j := e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: sh("sleep 300 & echo child=$!; wait")})
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
	if !processAlive(childPID) {
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
	waitUntil(t, 5*time.Second, "grandchild to die", func() bool { return !processAlive(childPID) })

	again, err := e.jc.CancelJob(context.Background(), connect.NewRequest(&pb.CancelJobRequest{Id: j.Id}))
	if err != nil || again.Msg.State != pb.JobState_JOB_STATE_CANCELED {
		t.Fatalf("second cancel = %v, %v", again, err)
	}
}

func processAlive(pid int) bool {
	raw, err := os.ReadFile("/proc/" + strconv.Itoa(pid) + "/stat")
	if err != nil {
		p, perr := os.FindProcess(pid)
		return perr == nil && p.Signal(nil) == nil
	}
	s := string(raw)
	return s[strings.LastIndex(s, ")")+2] != 'Z'
}

func TestJobTimeout(t *testing.T) {
	e := newJobEnv(t, 1)
	start := time.Now()
	j := e.wait(t, e.submit(t, &pb.SubmitJobRequest{
		Target: onNode("peer"), Command: sh("echo started; sleep 300"), Timeout: durationpb.New(700 * time.Millisecond),
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
	first := e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: sh("echo a; sleep 1.5")})
	waitUntil(t, 10*time.Second, "first running", func() bool { return e.get(t, first.Id).State == pb.JobState_JOB_STATE_RUNNING })
	second := e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: sh("echo b")})
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
	a := e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: sh("sleep 1.5")})
	b := e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: sh("sleep 1.5")})
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
		return &pb.SubmitJobRequest{Target: onNode("peer"), Source: gitSource(repo, ""), Command: sh("echo $$ >> pids; sleep 0.5; echo $$ >> pids; cat pids | wc -l")}
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
	j := e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: sh("echo one; sleep 0.5; echo two")})
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

func TestRestartMarksActiveJobsFailed(t *testing.T) {
	dir := t.TempDir()
	id := "20260101-000000-deadbeef"
	jd := filepath.Join(dir, id)
	if err := os.MkdirAll(jd, 0o700); err != nil {
		t.Fatal(err)
	}
	j := &pb.Job{Id: id, State: pb.JobState_JOB_STATE_RUNNING, Node: "peer"}
	if err := writeJob(jd, j); err != nil {
		t.Fatal(err)
	}
	writeFile(t, filepath.Join(jd, "job.log"), []byte("partial"), 0o600)
	m, err := OpenJobs(dir, nil, nil, nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	got := m.List(pb.JobState_JOB_STATE_UNSPECIFIED, 0)
	if len(got) != 1 || got[0].State != pb.JobState_JOB_STATE_FAILED || got[0].LogBytes != 7 || !strings.Contains(got[0].Error, "restarted") {
		t.Fatalf("after restart = %v", got)
	}
	reopened, err := OpenJobs(dir, nil, nil, nil, nil)
	if err != nil || reopened.List(pb.JobState_JOB_STATE_FAILED, 0)[0].Id != id {
		t.Fatalf("persisted failure lost: %v", err)
	}
}

func TestShutdownCancelsRunningJobs(t *testing.T) {
	e := newJobEnv(t, 1)
	j := e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: sh("echo up; sleep 300")})
	waitUntil(t, 10*time.Second, "running", func() bool { return e.get(t, j.Id).State == pb.JobState_JOB_STATE_RUNNING })
	e.h.jobs.Shutdown()
	got := e.get(t, j.Id)
	if got.State != pb.JobState_JOB_STATE_FAILED || got.Error != "hub stopped" {
		t.Fatalf("after shutdown = %v", got)
	}
	waitUntil(t, 5*time.Second, "node to drop the job", func() bool {
		e.remote.Cancel(j.Id)
		_, err := os.Stat(filepath.Join(e.remote.Dir(), "jobs", j.Id))
		return os.IsNotExist(err)
	})
}

func TestJobEventsAndScopes(t *testing.T) {
	e := newJobEnv(t, 1)
	sub, err := protectoratev1connect.NewEventServiceClient(&http.Client{Transport: bearer{testToken}}, e.srv.URL).
		Subscribe(context.Background(), connect.NewRequest(&pb.SubscribeRequest{}))
	if err != nil {
		t.Fatal(err)
	}
	defer sub.Close()
	j := e.wait(t, e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: sh("true")}).Id)
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
	seed := e.wait(t, e.submit(t, &pb.SubmitJobRequest{Target: onNode("peer"), Command: sh("true")}).Id)
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
