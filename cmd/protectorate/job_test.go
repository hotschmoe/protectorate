package main

import (
	"bytes"
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

func TestParseTarget(t *testing.T) {
	tests := []struct {
		on      string
		check   func(*pb.JobTarget) bool
		wantErr bool
	}{
		{"officewin", func(j *pb.JobTarget) bool { return j.GetNode() == "officewin" }, false},
		{"lease-0a1b2c3d", func(j *pb.JobTarget) bool { return j.GetLeaseId() == "lease-0a1b2c3d" }, false},
		{"lease-xyz", func(j *pb.JobTarget) bool { return j.GetNode() == "lease-xyz" }, false},
		{"os=windows,arch=amd64,mem=8G,gpu-vram=24G", func(j *pb.JobTarget) bool {
			s := j.GetSelector()
			return s.Os == "windows" && s.Arch == "amd64" && s.MinFreeMemoryBytes == 8<<30 && s.GpuVramBytes == 24<<30
		}, false},
		{"os=windows", func(j *pb.JobTarget) bool { return j.GetSelector().GetOs() == "windows" }, false},
		{"gpu:adreno", func(j *pb.JobTarget) bool { return j.GetSelector().GetGpu() == "adreno" }, false},
		{"os=windows,npu:hexagon,gpu:Arc Pro", func(j *pb.JobTarget) bool {
			s := j.GetSelector()
			return s.Os == "windows" && s.Npu == "hexagon" && s.Gpu == "Arc Pro"
		}, false},
		{"gpu:", nil, true},
		{"tpu:x", nil, true},
		{"", nil, true},
		{"os=", nil, true},
		{"color=red", nil, true},
		{"mem=lots", nil, true},
	}
	for _, tt := range tests {
		got, err := parseTarget(tt.on)
		if (err != nil) != tt.wantErr || (err == nil && !tt.check(got)) {
			t.Errorf("parseTarget(%q) = %v, %v", tt.on, got, err)
		}
	}
}

func TestParseGitSource(t *testing.T) {
	abs, _ := filepath.Abs("repo")
	absOf := func(p string) string { a, _ := filepath.Abs(p); return a }
	tests := []struct {
		in           string
		repo, commit string
	}{
		{"repo@abc123", abs, "abc123"},
		{"repo", abs, ""},
		{"/srv/x@feature/a@b", absOf("/srv/x@feature/a"), "b"},
		{"/srv/x@HEAD~2", absOf("/srv/x"), "HEAD~2"},
	}
	for _, tt := range tests {
		s, err := parseGitSource(tt.in)
		if err != nil || s.GetGit().Repo != tt.repo || s.GetGit().Commit != tt.commit {
			t.Errorf("parseGitSource(%q) = %v, %v", tt.in, s, err)
		}
	}
	if s, _ := parseGitSource("@main"); !filepath.IsAbs(s.GetGit().Repo) || s.GetGit().Commit != "main" {
		t.Errorf("repo defaults to the current directory: %v", s)
	}
}

func TestParseEnv(t *testing.T) {
	env, err := parseEnv([]string{"A=1", "B=x=y", "C="})
	if err != nil || env["A"] != "1" || env["B"] != "x=y" || env["C"] != "" || len(env) != 3 {
		t.Fatalf("env = %v, %v", env, err)
	}
	for _, bad := range []string{"NOEQUALS", "=v"} {
		if _, err := parseEnv([]string{bad}); err == nil {
			t.Errorf("%q must be rejected", bad)
		}
	}
}

func TestJobExitStatus(t *testing.T) {
	if err := jobExitStatus(&pb.Job{State: pb.JobState_JOB_STATE_SUCCEEDED}); err != nil {
		t.Fatal(err)
	}
	tests := []struct {
		job  *pb.Job
		code int
	}{
		{&pb.Job{Id: "j", State: pb.JobState_JOB_STATE_FAILED, ExitCode: 7}, 7},
		{&pb.Job{Id: "j", State: pb.JobState_JOB_STATE_FAILED, ExitCode: -1}, 1},
		{&pb.Job{Id: "j", State: pb.JobState_JOB_STATE_TIMED_OUT, ExitCode: 0}, 1},
		{&pb.Job{Id: "j", State: pb.JobState_JOB_STATE_CANCELED, ExitCode: 1000}, 1},
	}
	for _, tt := range tests {
		err := jobExitStatus(tt.job)
		ee, ok := err.(*exitError)
		if !ok || ee.code != tt.code {
			t.Errorf("%v -> %v", tt.job, err)
		}
	}
}

type fakeLeaseHub struct {
	protectoratev1connect.UnimplementedLeaseServiceHandler
	protectoratev1connect.UnimplementedJobServiceHandler
	submitErr error
	// holdSubmit blocks SubmitJob until the call is canceled or the channel closes.
	holdSubmit chan struct{}
	// holdJob makes WatchJob report success after this long instead of waiting for a cancel.
	holdJob  time.Duration
	renewErr error
	watching chan struct{}
	canceled chan struct{}
	released chan string

	mu         sync.Mutex
	ttl        time.Duration
	expires    time.Time
	renews     int
	submitKey  string
	cancelKeys []string
	cancelOnce sync.Once
}

func (f *fakeLeaseHub) AcquireLease(_ context.Context, r *connect.Request[pb.AcquireLeaseRequest]) (*connect.Response[pb.Lease], error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.ttl = r.Msg.Ttl.AsDuration()
	f.expires = time.Now().Add(f.ttl)
	return connect.NewResponse(&pb.Lease{Id: "lease-00000001", Resource: "box"}), nil
}

func (f *fakeLeaseHub) RenewLease(context.Context, *connect.Request[pb.RenewLeaseRequest]) (*connect.Response[pb.Lease], error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	if f.renewErr != nil {
		return nil, f.renewErr
	}
	if time.Now().After(f.expires) {
		return nil, connect.NewError(connect.CodeNotFound, errors.New("lease expired"))
	}
	f.renews++
	f.expires = time.Now().Add(f.ttl)
	return connect.NewResponse(&pb.Lease{Id: "lease-00000001"}), nil
}

func (f *fakeLeaseHub) ReleaseLease(_ context.Context, r *connect.Request[pb.ReleaseLeaseRequest]) (*connect.Response[pb.ReleaseLeaseResponse], error) {
	f.released <- r.Msg.Id
	return connect.NewResponse(&pb.ReleaseLeaseResponse{}), nil
}

func (f *fakeLeaseHub) SubmitJob(ctx context.Context, r *connect.Request[pb.SubmitJobRequest]) (*connect.Response[pb.Job], error) {
	f.mu.Lock()
	f.submitKey = r.Msg.RequestId
	f.mu.Unlock()
	if f.submitErr != nil {
		return nil, f.submitErr
	}
	if f.holdSubmit != nil {
		select {
		case <-f.holdSubmit:
		case <-ctx.Done():
			return nil, ctx.Err()
		}
	}
	return connect.NewResponse(&pb.Job{Id: "job-1", Node: "box"}), nil
}

func (f *fakeLeaseHub) CancelJob(_ context.Context, r *connect.Request[pb.CancelJobRequest]) (*connect.Response[pb.Job], error) {
	f.mu.Lock()
	f.cancelKeys = append(f.cancelKeys, r.Msg.RequestId)
	f.mu.Unlock()
	f.cancelOnce.Do(func() { close(f.canceled) })
	return connect.NewResponse(&pb.Job{Id: "job-1"}), nil
}

func (f *fakeLeaseHub) WatchJob(ctx context.Context, _ *connect.Request[pb.WatchJobRequest], s *connect.ServerStream[pb.WatchJobResponse]) error {
	select {
	case <-f.watching:
	default:
		close(f.watching)
	}
	state := pb.JobState_JOB_STATE_CANCELED
	var hold <-chan time.Time
	if f.holdJob > 0 {
		hold = time.After(f.holdJob)
	}
	select {
	case <-f.canceled:
	case <-hold:
		state = pb.JobState_JOB_STATE_SUCCEEDED
	case <-ctx.Done():
		return ctx.Err()
	}
	return s.Send(&pb.WatchJobResponse{Event: &pb.WatchJobResponse_Job{Job: &pb.Job{Id: "job-1", State: state}}})
}

func TestKeepLease(t *testing.T) {
	notFound := connect.NewError(connect.CodeNotFound, errors.New("gone"))
	unavailable := connect.NewError(connect.CodeUnavailable, errors.New("hub down"))
	tests := []struct {
		name    string
		results []error
		wantErr bool
		wantN   int
	}{
		{"renews on every tick", []error{nil, nil, nil, nil, nil, nil, nil, nil, nil, nil}, false, 10},
		{"lease gone is fatal at once", []error{nil, notFound}, true, 2},
		{"one transient failure is tolerated", []error{unavailable, nil, unavailable, nil}, false, 4},
		{"consecutive transient failures lose the lease", []error{nil, unavailable, unavailable}, true, 3},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			ticks := make(chan time.Time)
			var n int
			done := make(chan error, 1)
			go func() {
				done <- keepLease(ctx, ticks, func(context.Context) error {
					n++
					return tt.results[n-1]
				})
			}()
			for range tt.results {
				select {
				case ticks <- time.Time{}:
				case err := <-done:
					if !tt.wantErr {
						t.Fatalf("stopped early: %v", err)
					}
					done <- err
				}
			}
			if !tt.wantErr {
				cancel()
			}
			select {
			case err := <-done:
				if (err != nil) != tt.wantErr {
					t.Fatalf("err = %v, want error %v", err, tt.wantErr)
				}
			case <-time.After(5 * time.Second):
				t.Fatal("keepLease did not return")
			}
			if n != tt.wantN {
				t.Fatalf("renewed %d times, want %d", n, tt.wantN)
			}
		})
	}
}

func TestTestCrossReleasesLease(t *testing.T) {
	const pkg = "github.com/hotschmoe/protectorate/internal/fsutil"
	tests := []struct {
		name      string
		submitErr error
		interrupt bool
		wantCode  int
	}{
		{"submit fails", errors.New("hub exploded"), false, 0},
		{"interrupted while a job runs", nil, true, 130},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Setenv(tokenEnv, "tok")
			f := &fakeLeaseHub{submitErr: tt.submitErr, watching: make(chan struct{}), canceled: make(chan struct{}), released: make(chan string, 1)}
			mux := http.NewServeMux()
			mux.Handle(protectoratev1connect.NewLeaseServiceHandler(f))
			mux.Handle(protectoratev1connect.NewJobServiceHandler(f))
			srv := httptest.NewServer(mux)
			defer srv.Close()

			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			c := jobTestCrossCmd(&opts{hubURL: srv.URL})
			var out bytes.Buffer
			c.SetOut(&out)
			c.SetErr(&out)
			c.SetArgs([]string{"--goos", "linux", "--goarch", "amd64", pkg})
			done := make(chan error, 1)
			go func() { done <- c.ExecuteContext(ctx) }()

			if tt.interrupt {
				select {
				case <-f.watching:
				case <-time.After(2 * time.Minute):
					t.Fatalf("job never watched: %s", out.String())
				}
				cancel()
			}
			var err error
			select {
			case err = <-done:
			case <-time.After(2 * time.Minute):
				t.Fatalf("test-cross did not return: %s", out.String())
			}
			if tt.interrupt {
				var ee *exitError
				if !errors.As(err, &ee) || ee.code != tt.wantCode {
					t.Fatalf("err = %v, want exit %d", err, tt.wantCode)
				}
				select {
				case <-f.canceled:
				default:
					t.Fatalf("running job was not canceled: %s", out.String())
				}
			} else if err == nil {
				t.Fatal("want an error")
			}
			select {
			case id := <-f.released:
				if id != "lease-00000001" {
					t.Fatalf("released %q", id)
				}
			case <-time.After(5 * time.Second):
				t.Fatalf("lease not released: %s", out.String())
			}
		})
	}
}

func TestTestCrossLeaseLifecycle(t *testing.T) {
	const pkg = "github.com/hotschmoe/protectorate/internal/fsutil"
	const ttl = 300 * time.Millisecond
	tests := []struct {
		name       string
		hub        func() *fakeLeaseHub
		interrupt  bool
		wantErr    string
		wantRenews int
		wantCancel bool
		wantKeyCut bool
	}{
		{
			name:       "renewal keeps the lease across several ttls",
			hub:        func() *fakeLeaseHub { return &fakeLeaseHub{holdJob: 4 * ttl} },
			wantRenews: 6,
		},
		{
			name: "renewal failure stops the job",
			hub: func() *fakeLeaseHub {
				return &fakeLeaseHub{renewErr: connect.NewError(connect.CodeNotFound, errors.New("lease expired"))}
			},
			wantErr:    "lease lease-00000001 lost",
			wantCancel: true,
		},
		{
			name:       "interrupt while the submit is in flight cancels by request id",
			hub:        func() *fakeLeaseHub { return &fakeLeaseHub{holdSubmit: make(chan struct{})} },
			interrupt:  true,
			wantErr:    "interrupted",
			wantCancel: true,
			wantKeyCut: true,
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			old := testCrossLeaseTTL
			testCrossLeaseTTL = ttl
			defer func() { testCrossLeaseTTL = old }()
			t.Setenv(tokenEnv, "tok")
			f := tt.hub()
			f.watching, f.canceled, f.released = make(chan struct{}), make(chan struct{}), make(chan string, 1)
			mux := http.NewServeMux()
			mux.Handle(protectoratev1connect.NewLeaseServiceHandler(f))
			mux.Handle(protectoratev1connect.NewJobServiceHandler(f))
			srv := httptest.NewServer(mux)
			defer srv.Close()

			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			c := jobTestCrossCmd(&opts{hubURL: srv.URL})
			var out bytes.Buffer
			c.SetOut(&out)
			c.SetErr(&out)
			c.SetArgs([]string{"--goos", "linux", "--goarch", "amd64", pkg})
			done := make(chan error, 1)
			go func() { done <- c.ExecuteContext(ctx) }()

			if tt.interrupt {
				waitFor(t, "submit", func() bool { f.mu.Lock(); defer f.mu.Unlock(); return f.submitKey != "" })
				cancel()
			}
			var err error
			select {
			case err = <-done:
			case <-time.After(time.Minute):
				t.Fatalf("test-cross did not return: %s", out.String())
			}
			if tt.wantErr == "" && err != nil || tt.wantErr != "" && (err == nil || !strings.Contains(err.Error(), tt.wantErr)) {
				t.Fatalf("err = %v, want %q: %s", err, tt.wantErr, out.String())
			}
			select {
			case <-f.canceled:
				if !tt.wantCancel {
					t.Fatal("job was canceled")
				}
			default:
				if tt.wantCancel {
					t.Fatalf("job was not canceled: %s", out.String())
				}
			}
			select {
			case <-f.released:
			case <-time.After(5 * time.Second):
				t.Fatalf("lease not released: %s", out.String())
			}
			f.mu.Lock()
			defer f.mu.Unlock()
			if f.renews < tt.wantRenews {
				t.Fatalf("renewed %d times, want at least %d", f.renews, tt.wantRenews)
			}
			if tt.wantKeyCut && (len(f.cancelKeys) == 0 || f.cancelKeys[0] != f.submitKey || f.submitKey == "") {
				t.Fatalf("cancel keys %v, submit key %q", f.cancelKeys, f.submitKey)
			}
		})
	}
}

func waitFor(t *testing.T, what string, cond func() bool) {
	t.Helper()
	deadline := time.Now().Add(time.Minute)
	for !cond() {
		if time.Now().After(deadline) {
			t.Fatalf("timed out waiting for %s", what)
		}
		time.Sleep(10 * time.Millisecond)
	}
}

type watchHub struct {
	protectoratev1connect.UnimplementedJobServiceHandler
	mu      sync.Mutex
	script  []func(*pb.WatchJobRequest, *connect.ServerStream[pb.WatchJobResponse]) error
	offsets []uint64
}

func (w *watchHub) WatchJob(_ context.Context, req *connect.Request[pb.WatchJobRequest], stream *connect.ServerStream[pb.WatchJobResponse]) error {
	w.mu.Lock()
	w.offsets = append(w.offsets, req.Msg.FromOffset)
	step := w.script[min(len(w.offsets), len(w.script))-1]
	w.mu.Unlock()
	return step(req.Msg, stream)
}

func logFrame(off uint64, text string) *pb.WatchJobResponse {
	return &pb.WatchJobResponse{Event: &pb.WatchJobResponse_Log{Log: &pb.JobLog{Offset: off, Data: []byte(text)}}}
}

func jobFrame(s pb.JobState) *pb.WatchJobResponse {
	return &pb.WatchJobResponse{Event: &pb.WatchJobResponse_Job{Job: &pb.Job{Id: "j1", State: s}}}
}

func TestFollowJobResumesAcrossHubRestart(t *testing.T) {
	down := func(*pb.WatchJobRequest, *connect.ServerStream[pb.WatchJobResponse]) error {
		return connect.NewError(connect.CodeUnavailable, errors.New("hub restarting"))
	}
	partial := func(_ *pb.WatchJobRequest, s *connect.ServerStream[pb.WatchJobResponse]) error {
		for _, f := range []*pb.WatchJobResponse{jobFrame(pb.JobState_JOB_STATE_RUNNING), logFrame(0, "one\n")} {
			if err := s.Send(f); err != nil {
				return err
			}
		}
		return nil
	}
	rest := func(r *pb.WatchJobRequest, s *connect.ServerStream[pb.WatchJobResponse]) error {
		for _, f := range []*pb.WatchJobResponse{jobFrame(pb.JobState_JOB_STATE_RUNNING), logFrame(r.FromOffset, "two\n"), jobFrame(pb.JobState_JOB_STATE_SUCCEEDED)} {
			if err := s.Send(f); err != nil {
				return err
			}
		}
		return nil
	}
	notFound := func(*pb.WatchJobRequest, *connect.ServerStream[pb.WatchJobResponse]) error {
		return connect.NewError(connect.CodeNotFound, errors.New("no such job"))
	}
	tests := []struct {
		name        string
		script      []func(*pb.WatchJobRequest, *connect.ServerStream[pb.WatchJobResponse]) error
		wantOut     string
		wantOffsets []uint64
		wantErr     bool
	}{
		{"stream ends early, then the hub is down, then it is back", []func(*pb.WatchJobRequest, *connect.ServerStream[pb.WatchJobResponse]) error{partial, down, rest}, "one\ntwo\n", []uint64{0, 4, 4}, false},
		{"a missing job is final", []func(*pb.WatchJobRequest, *connect.ServerStream[pb.WatchJobResponse]) error{notFound}, "", []uint64{0}, true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			h := &watchHub{script: tt.script}
			mux := http.NewServeMux()
			mux.Handle(protectoratev1connect.NewJobServiceHandler(h))
			srv := httptest.NewServer(mux)
			defer srv.Close()
			var out bytes.Buffer
			job, err := followJob(context.Background(), protectoratev1connect.NewJobServiceClient(http.DefaultClient, srv.URL), "j1", 0, &out)
			if (err != nil) != tt.wantErr {
				t.Fatalf("err = %v", err)
			}
			if err == nil && job.State != pb.JobState_JOB_STATE_SUCCEEDED {
				t.Fatalf("job = %v", job)
			}
			if out.String() != tt.wantOut {
				t.Fatalf("out = %q, want %q", out.String(), tt.wantOut)
			}
			if !slices.Equal(h.offsets, tt.wantOffsets) {
				t.Fatalf("offsets = %v, want %v", h.offsets, tt.wantOffsets)
			}
		})
	}
}
