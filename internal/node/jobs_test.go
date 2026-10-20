package node

import (
	"archive/zip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"runtime"
	"strings"
	"sync"
	"testing"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/jobtest"
)

type collector struct {
	mu     sync.Mutex
	events []*pb.JobEvent
}

func (c *collector) send(ev *pb.JobEvent) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.events = append(c.events, ev)
	return nil
}

func (c *collector) count() int {
	c.mu.Lock()
	defer c.mu.Unlock()
	return len(c.events)
}

func (c *collector) output() string {
	c.mu.Lock()
	defer c.mu.Unlock()
	var b strings.Builder
	for _, ev := range c.events {
		if o := ev.GetOutput(); o != nil {
			b.Write(o.Data)
		}
	}
	return b.String()
}

func (c *collector) finished() *pb.JobFinished {
	c.mu.Lock()
	defer c.mu.Unlock()
	for _, ev := range c.events {
		if f := ev.GetFinished(); f != nil {
			return f
		}
	}
	return nil
}

func newJobs(t *testing.T, slots int) *Jobs {
	t.Helper()
	j, err := NewJobs(t.TempDir(), slots)
	if err != nil {
		t.Fatal(err)
	}
	return j
}

func TestMain(m *testing.M) {
	jobtest.Run()
	jobtest.QuietGit()
	os.Exit(m.Run())
}

func TestNewJobsClearsStaleScratchButKeepsRepos(t *testing.T) {
	dir := t.TempDir()
	for _, p := range []string{"jobs/old/upload", "repos/keep/marker"} {
		if err := os.MkdirAll(filepath.Join(dir, filepath.Dir(p)), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(dir, p), []byte("x"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if _, err := NewJobs(dir, 1); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(dir, "jobs", "old")); !os.IsNotExist(err) {
		t.Fatalf("stale job dir survived: %v", err)
	}
	if _, err := os.Stat(filepath.Join(dir, "repos", "keep", "marker")); err != nil {
		t.Fatalf("repo cache lost: %v", err)
	}
	if _, err := NewJobs(dir, 0); err == nil {
		t.Fatal("zero concurrency must be rejected")
	}
}

func TestUpload(t *testing.T) {
	j := newJobs(t, 1)
	sum := func(b ...string) string {
		h := sha256.New()
		for _, s := range b {
			h.Write([]byte(s))
		}
		return hex.EncodeToString(h.Sum(nil))
	}
	up := func(id string, off uint64, data string, last bool, digest string) (*pb.UploadJobDataResponse, error) {
		return j.Upload(&pb.UploadJobDataRequest{JobId: id, Offset: off, Data: []byte(data), Last: last, Sha256: digest})
	}

	if _, err := up("bad/id", 0, "x", false, ""); connect.CodeOf(err) != connect.CodeInvalidArgument {
		t.Fatalf("bad id: %v", err)
	}
	if _, err := up("a1", 5, "x", false, ""); connect.CodeOf(err) != connect.CodeFailedPrecondition {
		t.Fatalf("resume without start: %v", err)
	}
	if res, err := up("a1", 0, "hello ", false, ""); err != nil || res.Received != 6 {
		t.Fatalf("first chunk: %v, %v", res, err)
	}
	if _, err := up("a1", 0+3, "x", false, ""); connect.CodeOf(err) != connect.CodeFailedPrecondition {
		t.Fatalf("gap: %v", err)
	}
	if res, err := up("a1", 6, "world", true, sum("hello ", "world")); err != nil || res.Received != 11 {
		t.Fatalf("last chunk: %v, %v", res, err)
	}
	got, err := os.ReadFile(filepath.Join(j.jobDir("a1"), "upload"))
	if err != nil || string(got) != "hello world" {
		t.Fatalf("stored = %q, %v", got, err)
	}
	if _, err := up("a1", 11, "more", false, ""); connect.CodeOf(err) != connect.CodeFailedPrecondition {
		t.Fatalf("append after completion: %v", err)
	}

	if _, err := up("b1", 0, "data", true, sum("other")); connect.CodeOf(err) != connect.CodeDataLoss {
		t.Fatalf("digest mismatch: %v", err)
	}
	if _, err := os.Stat(j.jobDir("b1")); !os.IsNotExist(err) {
		t.Fatalf("failed upload left files: %v", err)
	}

	if _, err := up("c1", 0, "abc", false, ""); err != nil {
		t.Fatal(err)
	}
	if res, err := up("c1", 0, "xyz", true, sum("xyz")); err != nil || res.Received != 3 {
		t.Fatalf("restart from zero: %v, %v", res, err)
	}
	if !j.Cancel("c1") {
		t.Fatal("cancel must report that it dropped the upload")
	}
	if _, err := os.Stat(j.jobDir("c1")); !os.IsNotExist(err) {
		t.Fatalf("cancel must remove the upload: %v", err)
	}
	if j.Cancel("never-existed") {
		t.Fatal("cancel of an unknown job must report not found")
	}
}

func TestValidateRun(t *testing.T) {
	ok := &pb.RunJobRequest{JobId: "j1", Command: []string{"true"}}
	tests := []struct {
		name string
		mod  func(r *pb.RunJobRequest)
		bad  bool
	}{
		{"ok", func(*pb.RunJobRequest) {}, false},
		{"bad id", func(r *pb.RunJobRequest) { r.JobId = "../x" }, true},
		{"no command", func(r *pb.RunJobRequest) { r.Command = nil }, true},
		{"empty argv0", func(r *pb.RunJobRequest) { r.Command = []string{""} }, true},
		{"two sources", func(r *pb.RunJobRequest) { r.RepoKey, r.Commit, r.ArtifactName = "k", strings.Repeat("a", 40), "x" }, true},
		{"git ok", func(r *pb.RunJobRequest) { r.RepoKey, r.Commit = "k", strings.Repeat("a", 40) }, false},
		{"git short commit", func(r *pb.RunJobRequest) { r.RepoKey, r.Commit = "k", "abc" }, true},
		{"git option commit", func(r *pb.RunJobRequest) { r.RepoKey, r.Commit = "k", "--upload-pack=x" }, true},
		{"git path key", func(r *pb.RunJobRequest) { r.RepoKey, r.Commit = "../k", strings.Repeat("a", 40) }, true},
		{"artifact path", func(r *pb.RunJobRequest) { r.ArtifactName = "../x" }, true},
		{"artifact drive", func(r *pb.RunJobRequest) { r.ArtifactName = "C:x" }, true},
		{"artifact ok", func(r *pb.RunJobRequest) { r.ArtifactName = "pkg.test.exe" }, false},
		{"env bad name", func(r *pb.RunJobRequest) { r.Env = map[string]string{"A=B": "x"} }, true},
		{"glob abs", func(r *pb.RunJobRequest) { r.ArtifactGlobs = []string{"/etc/*"} }, true},
		{"glob dotdot", func(r *pb.RunJobRequest) { r.ArtifactGlobs = []string{"a/../../b"} }, true},
		{"glob ok", func(r *pb.RunJobRequest) { r.ArtifactGlobs = []string{"dist/*", "target/release/app"} }, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			r := &pb.RunJobRequest{JobId: ok.JobId, Command: ok.Command}
			tt.mod(r)
			err := validateRun(r)
			if (err != nil) != tt.bad || (err != nil && connect.CodeOf(err) != connect.CodeInvalidArgument) {
				t.Fatalf("err = %v, want bad=%v", err, tt.bad)
			}
		})
	}
}

func TestMatchArtifacts(t *testing.T) {
	dir := t.TempDir()
	for _, p := range []string{"dist/a.bin", "dist/sub/b.bin", "top.txt", "target/x.o"} {
		if err := os.MkdirAll(filepath.Join(dir, filepath.Dir(p)), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(dir, p), []byte(p), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.Symlink("/etc/passwd", filepath.Join(dir, "dist", "link")); err != nil {
		t.Fatal(err)
	}
	tests := []struct {
		globs []string
		want  []string
	}{
		{[]string{"dist/*"}, []string{"dist/a.bin", "dist/sub/b.bin"}},
		{[]string{"top.txt", "dist/a.bin", "top.*"}, []string{"dist/a.bin", "top.txt"}},
		{[]string{"nothing*"}, []string{}},
		{[]string{"dist/link"}, []string{}},
		{nil, []string{}},
	}
	for _, tt := range tests {
		got, err := matchArtifacts(dir, tt.globs)
		if err != nil || !reflect.DeepEqual(got, tt.want) {
			t.Errorf("%v = %v, %v; want %v", tt.globs, got, err, tt.want)
		}
	}
}

func TestRunOutputExitAndEnv(t *testing.T) {
	j := newJobs(t, 1)
	c := &collector{}
	req := &pb.RunJobRequest{JobId: "r1", Command: jobtest.Script("say:out", "err:err", "env:A,PROTECTORATE_JOB_ID", "exit:3"), Env: map[string]string{"A": "val"}}
	if err := j.Run(context.Background(), req, c.send); err != nil {
		t.Fatal(err)
	}
	f := c.finished()
	if f == nil || f.End != pb.JobEnd_JOB_END_EXITED || f.ExitCode != 3 {
		t.Fatalf("finished = %v", f)
	}
	if got := c.output(); got != "out\nerr\nval r1\n" {
		t.Fatalf("output = %q", got)
	}
	if c.count() == 0 || c.events[0].GetStarted() == nil {
		t.Fatalf("first event = %v", c.events)
	}
	if _, err := os.Stat(j.jobDir("r1")); !os.IsNotExist(err) {
		t.Fatalf("scratch left behind: %v", err)
	}
}

func TestRunTimeoutKillsTree(t *testing.T) {
	j := newJobs(t, 1)
	c := &collector{}
	start := time.Now()
	req := &pb.RunJobRequest{JobId: "t1", Command: jobtest.Script("spawn:300", "wait"), Timeout: durationpb.New(300 * time.Millisecond)}
	if err := j.Run(context.Background(), req, c.send); err != nil {
		t.Fatal(err)
	}
	if f := c.finished(); f == nil || f.End != pb.JobEnd_JOB_END_TIMED_OUT || f.ExitCode == 0 {
		t.Fatalf("finished = %v", f)
	}
	if time.Since(start) > 8*time.Second {
		t.Fatalf("took %s", time.Since(start))
	}
}

func TestRunStubbornProcessIsKilledAfterGrace(t *testing.T) {
	j := newJobs(t, 1)
	c := &collector{}
	start := time.Now()
	req := &pb.RunJobRequest{JobId: "s1", Command: jobtest.Script("ignoreterm", "say:ready", "sleep:300"), Timeout: durationpb.New(500 * time.Millisecond)}
	if err := j.Run(context.Background(), req, c.send); err != nil {
		t.Fatal(err)
	}
	if f := c.finished(); f == nil || f.End != pb.JobEnd_JOB_END_TIMED_OUT {
		t.Fatalf("finished = %v", f)
	}
	want := killGrace
	if runtime.GOOS == "windows" {
		want = 0
	}
	if d := time.Since(start); d < want || d > want+5*time.Second {
		t.Fatalf("SIGTERM-immune process died after %s, want about %s", d, want)
	}
}

func waitOutput(t *testing.T, c *collector, want string) {
	t.Helper()
	deadline := time.Now().Add(10 * time.Second)
	for !strings.Contains(c.output(), want) && time.Now().Before(deadline) {
		time.Sleep(10 * time.Millisecond)
	}
	if !strings.Contains(c.output(), want) {
		t.Fatalf("output %q never contained %q", c.output(), want)
	}
}

func TestRunSurvivesDisconnectAndAttachResumes(t *testing.T) {
	j := newJobs(t, 1)
	ctx, cancel := context.WithCancel(context.Background())
	c := &collector{}
	done := make(chan error, 1)
	go func() {
		done <- j.Run(ctx, &pb.RunJobRequest{JobId: "d1", Command: jobtest.Script("say:up", "sleep:1", "say:down", "exit:7")}, c.send)
	}()
	waitOutput(t, c, "up")
	cancel()
	if err := <-done; err == nil {
		t.Fatal("a disconnected run must report the context error")
	}
	listed := j.List()
	if len(listed) != 1 || listed[0].JobId != "d1" || !listed[0].Started || listed[0].Finished != nil || listed[0].LogBytes < 3 {
		t.Fatalf("List after disconnect = %v", listed)
	}

	seen := uint64(len(c.output()))
	c2 := &collector{}
	if err := j.Attach(context.Background(), "d1", seen, c2.send); err != nil {
		t.Fatal(err)
	}
	if got, want := c.output()+c2.output(), "up\ndown\n"; got != want {
		t.Fatalf("output across attach = %q, want %q", got, want)
	}
	if f := c2.finished(); f == nil || f.ExitCode != 7 || f.End != pb.JobEnd_JOB_END_EXITED {
		t.Fatalf("finished = %v", f)
	}
	again := &collector{}
	if err := j.Attach(context.Background(), "d1", 0, again.send); err != nil || again.output() != "up\ndown\n" || again.finished() == nil {
		t.Fatalf("finished job must stay attachable: %v %q", err, again.output())
	}
	if j.Cancel("d1") {
		t.Fatal("a finished job is not cancelable")
	}
}

func TestAttachErrors(t *testing.T) {
	j := newJobs(t, 1)
	c := &collector{}
	if err := j.Run(context.Background(), &pb.RunJobRequest{JobId: "a1", Command: jobtest.Script("say:x")}, c.send); err != nil {
		t.Fatal(err)
	}
	tests := []struct {
		name   string
		id     string
		offset uint64
		want   connect.Code
	}{
		{"unknown job", "nope", 0, connect.CodeNotFound},
		{"offset past the output", "a1", 99, connect.CodeFailedPrecondition},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := j.Attach(context.Background(), tt.id, tt.offset, (&collector{}).send)
			if connect.CodeOf(err) != tt.want {
				t.Fatalf("code = %v (%v), want %v", connect.CodeOf(err), err, tt.want)
			}
		})
	}
}

func TestFinishedJobsAreSweptAfterRetention(t *testing.T) {
	j := newJobs(t, 1)
	now := time.Now()
	j.now = func() time.Time { return now }
	if err := j.Run(context.Background(), &pb.RunJobRequest{JobId: "r1", Command: jobtest.Script("say:x")}, (&collector{}).send); err != nil {
		t.Fatal(err)
	}
	now = now.Add(retention - time.Minute)
	if len(j.List()) != 1 {
		t.Fatal("job dropped before its retention ended")
	}
	now = now.Add(2 * time.Minute)
	if got := j.List(); len(got) != 0 {
		t.Fatalf("job kept past retention: %v", got)
	}
	if _, err := os.Stat(j.runDir("r1")); !os.IsNotExist(err) {
		t.Fatalf("run directory kept: %v", err)
	}
}

func TestRunWaitsForSlotAndCancelWhileQueued(t *testing.T) {
	j := newJobs(t, 1)
	c1 := &collector{}
	first := make(chan error, 1)
	go func() {
		first <- j.Run(context.Background(), &pb.RunJobRequest{JobId: "q1", Command: jobtest.Script("sleep:1")}, c1.send)
	}()
	deadline := time.Now().Add(10 * time.Second)
	for c1.count() == 0 && time.Now().Before(deadline) {
		time.Sleep(10 * time.Millisecond)
	}
	c2 := &collector{}
	second := make(chan error, 1)
	go func() {
		second <- j.Run(context.Background(), &pb.RunJobRequest{JobId: "q2", Command: jobtest.Script("say:two")}, c2.send)
	}()
	time.Sleep(300 * time.Millisecond)
	if c2.count() != 0 {
		t.Fatalf("queued job ran early: %v", c2.events)
	}
	if !j.Cancel("q2") {
		t.Fatal("queued job is not cancelable")
	}
	if err := <-second; err != nil {
		t.Fatal(err)
	}
	if f := c2.finished(); f == nil || f.End != pb.JobEnd_JOB_END_CANCELED {
		t.Fatalf("queued cancel = %v", f)
	}
	if err := <-first; err != nil || c1.finished().ExitCode != 0 {
		t.Fatalf("first = %v, %v", c1.finished(), err)
	}
}

func TestRunDuplicateJobID(t *testing.T) {
	j := newJobs(t, 2)
	c := &collector{}
	done := make(chan struct{})
	go func() {
		defer close(done)
		_ = j.Run(context.Background(), &pb.RunJobRequest{JobId: "dup", Command: jobtest.Script("sleep:30")}, c.send)
	}()
	deadline := time.Now().Add(10 * time.Second)
	for c.count() == 0 && time.Now().Before(deadline) {
		time.Sleep(10 * time.Millisecond)
	}
	err := j.Run(context.Background(), &pb.RunJobRequest{JobId: "dup", Command: jobtest.Script()}, (&collector{}).send)
	if connect.CodeOf(err) != connect.CodeAlreadyExists {
		t.Fatalf("duplicate run: %v", err)
	}
	j.Cancel("dup")
	<-done
}

func TestRunGitWithoutBundleOrCommitFails(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git is not installed")
	}
	j := newJobs(t, 1)
	c := &collector{}
	req := &pb.RunJobRequest{JobId: "g1", Command: []string{"true"}, RepoKey: "repo", Commit: strings.Repeat("a", 40)}
	if err := j.Run(context.Background(), req, c.send); err != nil {
		t.Fatal(err)
	}
	if f := c.finished(); f == nil || f.End != pb.JobEnd_JOB_END_ERROR || !strings.Contains(f.Error, "not in the node's cache") {
		t.Fatalf("finished = %v", f)
	}
	res, err := j.Prepare(context.Background(), &pb.PrepareJobRequest{RepoKey: "repo", Commit: strings.Repeat("a", 40)})
	if err != nil || res.HasCommit {
		t.Fatalf("prepare = %v, %v", res, err)
	}
	if _, err := j.Prepare(context.Background(), &pb.PrepareJobRequest{RepoKey: "../x", Commit: strings.Repeat("a", 40)}); connect.CodeOf(err) != connect.CodeInvalidArgument {
		t.Fatalf("path traversal in repo key: %v", err)
	}
}

func TestRunLogIsCapped(t *testing.T) {
	j := newJobs(t, 1)
	c := &collector{}
	req := &pb.RunJobRequest{JobId: "l1", Command: jobtest.Script("flood:70000000", "say:done")}
	if err := j.Run(context.Background(), req, c.send); err != nil {
		t.Fatal(err)
	}
	out := c.output()
	if len(out) > maxLogBytes+64 || !strings.HasSuffix(out, "[log truncated]\n") {
		t.Fatalf("log = %d bytes, tail %q", len(out), out[max(0, len(out)-30):])
	}
	if f := c.finished(); f == nil || f.ExitCode != 0 {
		t.Fatalf("a chatty job must still finish: %v", f)
	}
}

func TestAgentJobRPCsNeedAuthAndEnablement(t *testing.T) {
	jobs := newJobs(t, 1)
	srv := httptest.NewServer((&Agent{Collector: NewCollector(Options{DockerHost: "unix:///nonexistent.sock"}), Authorize: TokenAuthorizer("tok"), Jobs: jobs}).Handler())
	defer srv.Close()
	off := httptest.NewServer((&Agent{Collector: NewCollector(Options{DockerHost: "unix:///nonexistent.sock"}), Authorize: TokenAuthorizer("tok")}).Handler())
	defer off.Close()

	run := func(url, tok string) error {
		c := protectoratev1connect.NewNodeAgentServiceClient(&http.Client{Transport: tokenRT(tok)}, url)
		stream, err := c.RunJob(context.Background(), connect.NewRequest(&pb.RunJobRequest{JobId: "x", Command: []string{"true"}}))
		if err != nil {
			return err
		}
		defer stream.Close()
		for stream.Receive() {
		}
		return stream.Err()
	}
	if err := run(srv.URL, ""); connect.CodeOf(err) != connect.CodeUnauthenticated {
		t.Fatalf("no token: %v", err)
	}
	if err := run(srv.URL, "wrong"); connect.CodeOf(err) != connect.CodeUnauthenticated {
		t.Fatalf("wrong token: %v", err)
	}
	if err := run(srv.URL, "tok"); err != nil {
		t.Fatalf("authorized run: %v", err)
	}
	if err := run(off.URL, "tok"); connect.CodeOf(err) != connect.CodeUnimplemented {
		t.Fatalf("jobs disabled: %v", err)
	}
	c := protectoratev1connect.NewNodeAgentServiceClient(&http.Client{Transport: tokenRT("")}, srv.URL)
	if _, err := c.CancelJob(context.Background(), connect.NewRequest(&pb.NodeAgentServiceCancelJobRequest{JobId: "x"})); connect.CodeOf(err) != connect.CodeUnauthenticated {
		t.Fatalf("unauthenticated cancel: %v", err)
	}
	if _, err := c.UploadJobData(context.Background(), connect.NewRequest(&pb.UploadJobDataRequest{JobId: "x"})); connect.CodeOf(err) != connect.CodeUnauthenticated {
		t.Fatalf("unauthenticated upload: %v", err)
	}
}

type tokenRT string

func (t tokenRT) RoundTrip(r *http.Request) (*http.Response, error) {
	r = r.Clone(r.Context())
	if t != "" {
		r.Header.Set("Authorization", "Bearer "+string(t))
	}
	return http.DefaultTransport.RoundTrip(r)
}

func TestExtractZipRefusesUnsafeEntries(t *testing.T) {
	for _, name := range []string{"../evil", "/abs", "a/../../evil"} {
		t.Run(name, func(t *testing.T) {
			src := filepath.Join(t.TempDir(), "a.zip")
			f, err := os.Create(src)
			if err != nil {
				t.Fatal(err)
			}
			zw := zip.NewWriter(f)
			w, _ := zw.Create(name)
			_, _ = w.Write([]byte("x"))
			zw.Close()
			f.Close()
			dst := filepath.Join(t.TempDir(), "out")
			if err := extractZip(src, dst); err == nil {
				t.Fatalf("accepted %q", name)
			}
		})
	}
}
