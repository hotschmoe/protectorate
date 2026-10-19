package main

import (
	"testing"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

func TestParseSize(t *testing.T) {
	tests := []struct {
		in      string
		want    uint64
		wantErr bool
	}{
		{"512", 512, false}, {"512M", 512 << 20, false}, {"8G", 8 << 30, false}, {"8GiB", 8 << 30, false},
		{"1.5g", 3 << 29, false}, {"2K", 2048, false}, {"", 0, true}, {"G", 0, true}, {"-1G", 0, true}, {"abc", 0, true},
	}
	for _, tt := range tests {
		got, err := parseSize(tt.in)
		if (err != nil) != tt.wantErr || got != tt.want {
			t.Errorf("parseSize(%q) = %d, %v", tt.in, got, err)
		}
	}
}

func TestExitLabels(t *testing.T) {
	code := func(n int32) *int32 { return &n }
	sleeves := []struct {
		name    string
		s       *pb.Sleeve
		label   string
		rowName string
	}{
		{"plain exit", &pb.Sleeve{State: pb.SleeveState_SLEEVE_STATE_EXITED, ExitCode: code(3)}, "exit 3", "exited(3)"},
		{"signal", &pb.Sleeve{State: pb.SleeveState_SLEEVE_STATE_EXITED, ExitCode: code(139), ExitSignal: 11}, "killed by signal 11 (exit 139)", "exited(139)"},
		{"oom", &pb.Sleeve{State: pb.SleeveState_SLEEVE_STATE_EXITED, ExitCode: code(137), OomKilled: true}, "exit 137 (OOM killed)", "exited(137,oom)"},
		{"unknown", &pb.Sleeve{State: pb.SleeveState_SLEEVE_STATE_EXITED}, "unknown", "exited"},
		{"running", &pb.Sleeve{State: pb.SleeveState_SLEEVE_STATE_RUNNING}, "unknown", "running"},
	}
	for _, tt := range sleeves {
		t.Run(tt.name, func(t *testing.T) {
			if tt.s.State == pb.SleeveState_SLEEVE_STATE_EXITED {
				if got := exitLabel(tt.s); got != tt.label {
					t.Errorf("exitLabel = %q, want %q", got, tt.label)
				}
			}
			if got := sleeveRowState(tt.s); got != tt.rowName {
				t.Errorf("sleeveRowState = %q, want %q", got, tt.rowName)
			}
		})
	}
	sessions := []struct {
		s    *pb.Session
		want string
	}{
		{&pb.Session{State: pb.SessionState_SESSION_STATE_RUNNING}, "running"},
		{&pb.Session{State: pb.SessionState_SESSION_STATE_EXITED, ExitCode: 3}, "exited(3)"},
		{&pb.Session{State: pb.SessionState_SESSION_STATE_EXITED, ExitCode: 137, ExitSignal: 9}, "exited(signal 9)"},
	}
	for _, tt := range sessions {
		if got := sessionStateLabel(tt.s); got != tt.want {
			t.Errorf("sessionStateLabel = %q, want %q", got, tt.want)
		}
	}
}
