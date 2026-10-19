//go:build !windows

package node

import (
	"reflect"
	"testing"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

func TestCommandFor(t *testing.T) {
	cmd := []string{"pacman", "-Syu", "--noconfirm"}
	tests := []struct {
		name      string
		iso       *pb.JobIsolation
		mode      pb.JobMode
		env       map[string]string
		wantArgv  []string
		wantLabel string
		wantErr   bool
	}{
		{"sudo keeps the job's variables by name", &pb.JobIsolation{Sandbox: "none", Admin: "sudo"}, pb.JobMode_JOB_MODE_ADMIN, map[string]string{"B": "2", "A": "secret"},
			[]string{"sudo", "-n", "--preserve-env=PROTECTORATE_JOB_ID,PROTECTORATE_JOB_DIR,A,B", "--", "pacman", "-Syu", "--noconfirm"}, "sudo", false},
		{"agent is root", &pb.JobIsolation{Sandbox: "none", Admin: "root"}, pb.JobMode_JOB_MODE_ADMIN, nil, cmd, "root", false},
		{"cannot elevate", &pb.JobIsolation{Sandbox: "none", Note: "no admin jobs: sudo -n fails"}, pb.JobMode_JOB_MODE_ADMIN, nil, nil, "", true},
		{"unknown elevation", &pb.JobIsolation{Admin: "doas"}, pb.JobMode_JOB_MODE_ADMIN, nil, nil, "", true},
		{"plain", &pb.JobIsolation{Sandbox: "none", Admin: "sudo"}, pb.JobMode_JOB_MODE_PLAIN, nil, cmd, "plain", false},
		{"sandboxed without a sandbox says so", &pb.JobIsolation{Sandbox: "none", Note: "bwrap is not installed"}, pb.JobMode_JOB_MODE_UNSPECIFIED, nil, cmd, "none: bwrap is not installed", false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			j := &Jobs{Probe: func() *pb.JobIsolation { return tt.iso }}
			argv, label, err := j.commandFor(&pb.RunJobRequest{Command: cmd, Env: tt.env, Mode: tt.mode})
			if (err != nil) != tt.wantErr {
				t.Fatalf("err = %v", err)
			}
			if !tt.wantErr && (!reflect.DeepEqual(argv, tt.wantArgv) || label != tt.wantLabel) {
				t.Fatalf("argv %q label %q", argv, label)
			}
		})
	}
}
