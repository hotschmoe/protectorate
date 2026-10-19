package main

import (
	"path/filepath"
	"testing"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
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
