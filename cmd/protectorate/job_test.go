package main

import (
	"bytes"
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"path/filepath"
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
		{"os=windows", func(j *pb.JobTarget) bool { return j.AllowFamily }, false},
		{"", nil, true},
		{"os=", nil, true},
		{"color=red", nil, true},
		{"mem=lots", nil, true},
	}
	for _, tt := range tests {
		got, err := parseTarget(tt.on, tt.on == "os=windows")
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
	watching  chan struct{}
	canceled  chan struct{}
	released  chan string
}

func (f *fakeLeaseHub) AcquireLease(context.Context, *connect.Request[pb.AcquireLeaseRequest]) (*connect.Response[pb.Lease], error) {
	return connect.NewResponse(&pb.Lease{Id: "lease-00000001", Resource: "box"}), nil
}

func (f *fakeLeaseHub) ReleaseLease(_ context.Context, r *connect.Request[pb.ReleaseLeaseRequest]) (*connect.Response[pb.ReleaseLeaseResponse], error) {
	f.released <- r.Msg.Id
	return connect.NewResponse(&pb.ReleaseLeaseResponse{}), nil
}

func (f *fakeLeaseHub) SubmitJob(context.Context, *connect.Request[pb.SubmitJobRequest]) (*connect.Response[pb.Job], error) {
	if f.submitErr != nil {
		return nil, f.submitErr
	}
	return connect.NewResponse(&pb.Job{Id: "job-1", Node: "box"}), nil
}

func (f *fakeLeaseHub) CancelJob(context.Context, *connect.Request[pb.CancelJobRequest]) (*connect.Response[pb.Job], error) {
	close(f.canceled)
	return connect.NewResponse(&pb.Job{Id: "job-1"}), nil
}

func (f *fakeLeaseHub) WatchJob(ctx context.Context, _ *connect.Request[pb.WatchJobRequest], s *connect.ServerStream[pb.WatchJobResponse]) error {
	close(f.watching)
	select {
	case <-f.canceled:
	case <-ctx.Done():
		return ctx.Err()
	}
	return s.Send(&pb.WatchJobResponse{Event: &pb.WatchJobResponse_Job{Job: &pb.Job{Id: "job-1", State: pb.JobState_JOB_STATE_CANCELED}}})
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
