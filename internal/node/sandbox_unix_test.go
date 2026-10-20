//go:build !windows

package node

import (
	"context"
	"os"
	"path/filepath"
	"runtime"
	"slices"
	"strconv"
	"strings"
	"testing"
	"time"

	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

func testSandbox() *sandbox {
	return &sandbox{
		cfg: sleeve.Config{
			Home: "/state", UserHome: "/home/u", User: "u",
			System:   []string{"--ro-bind", "/usr", "/usr"},
			ToolDirs: []string{"/home/u/.local/go"}, PathDirs: []string{"/home/u/.local/go/bin", "/usr/bin"},
			TasksMax: 4096, UnitPrefix: jobUnitPrefix,
		},
		home: "/state/node-jobs/home", hostname: "box",
	}
}

func TestSandboxPlan(t *testing.T) {
	jobDir := "/state/node-jobs/jobs/j1"
	tests := []struct {
		name      string
		workdir   string
		limits    *pb.JobLimits
		wantRW    []string
		wantProps []string
		wantErr   string
	}{
		{"git worktree", "/state/node-jobs/repos/k", &pb.JobLimits{CpuPercent: 200, MemoryBytes: 4 << 30},
			[]string{jobDir, "/state/node-jobs/repos/k"}, []string{"MemoryMax=4294967296", "MemorySwapMax=0", "CPUQuota=200%"}, ""},
		{"artifact in the job dir", jobDir + "/work/sub", nil, []string{jobDir}, nil, ""},
		{"workdir in a credential dir", "/home/u/.ssh", nil, nil, nil, "credentials"},
		{"workdir containing the state dir", "/", nil, nil, nil, "filesystem root"},
		{"memory below the floor", jobDir, &pb.JobLimits{MemoryBytes: 1 << 20}, nil, nil, "64 MiB"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			req := &pb.RunJobRequest{JobId: "j1", Command: []string{"make", "-j8"}, Limits: tt.limits}
			argv, envArgs, err := testSandbox().plan(req, tt.workdir, jobDir, [][2]string{{"PROTECTORATE_JOB_ID", "j1"}, {"TOKEN", "s3cret"}})
			if tt.wantErr != "" {
				if err == nil || !strings.Contains(err.Error(), tt.wantErr) {
					t.Fatalf("err = %v, want %q", err, tt.wantErr)
				}
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			line := strings.Join(argv, " ")
			for _, want := range []string{
				"systemd-run --user --scope --quiet --collect --unit=prot-job-j1",
				"bwrap --unshare-pid --unshare-ipc --unshare-uts --die-with-parent --hostname box --tmpfs /",
				"--bind /state/node-jobs/home /home/u", "--ro-bind /home/u/.local/go /home/u/.local/go",
				"--chdir " + tt.workdir + " --clearenv --args 3 -- make -j8",
			} {
				if !strings.Contains(line, want) {
					t.Errorf("argv lacks %q:\n%s", want, line)
				}
			}
			var rw []string
			for i, a := range argv {
				if a == "--bind" && argv[i+2] != "/home/u" {
					rw = append(rw, argv[i+1])
				}
			}
			if !slices.Equal(rw, tt.wantRW) {
				t.Errorf("writable binds %v, want %v", rw, tt.wantRW)
			}
			for _, p := range tt.wantProps {
				if !slices.Contains(argv, p) {
					t.Errorf("argv lacks -p %s", p)
				}
			}
			if strings.Contains(line, "s3cret") || strings.Contains(line, "--setenv") {
				t.Errorf("env values in argv: %s", line)
			}
			env := string(envArgs)
			for _, want := range []string{"--setenv\x00HOME\x00/home/u\x00", "--setenv\x00TOKEN\x00s3cret\x00", "--setenv\x00GOMODCACHE\x00/home/u/.cache/go-mod\x00"} {
				if !strings.Contains(env, want) {
					t.Errorf("env args lack %q", want)
				}
			}
		})
	}
}

// TestSandboxedJobLive runs a real job in bwrap and a systemd user scope
// where the host has both.
func TestSandboxedJobLive(t *testing.T) {
	if runtime.GOOS != "linux" {
		t.Skip("bwrap sandboxes are linux only")
	}
	home := t.TempDir()
	j := newJobs(t, 1)
	j.Home = home
	iso := j.Isolation()
	if iso.Sandbox != "bwrap" {
		t.Skipf("no sandbox on this host: %s", iso.Note)
	}
	t.Setenv("PROTECTORATE_LEAK_CHECK", "agent-only")
	realHome, err := os.UserHomeDir()
	if err != nil {
		t.Fatal(err)
	}
	script := `echo "home=$HOME"; echo "user=$USER"; ` +
		`[ -e "$HOME/.ssh" ] && echo "ssh visible" || echo "no ssh"; ` +
		`[ -e "` + home + `" ] && echo "state visible" || echo "no state"; ` +
		`echo "leak=${PROTECTORATE_LEAK_CHECK:-none}"; echo "secret=$JOB_SECRET"; ` +
		`echo "procs=$(ls /proc | grep -c '^[0-9]')"; echo cached > "$HOME/cache-marker"; pwd`
	c := &collector{}
	err = j.Run(context.Background(), &pb.RunJobRequest{
		JobId: "sbx1", Command: []string{"sh", "-c", script}, Env: map[string]string{"JOB_SECRET": "s3cret"},
		Timeout: durationpb.New(time.Minute), Limits: &pb.JobLimits{CpuPercent: 100, MemoryBytes: 256 << 20},
	}, c.send)
	if err != nil {
		t.Fatal(err)
	}
	f := c.finished()
	out := c.output()
	if f == nil || f.End != pb.JobEnd_JOB_END_EXITED || f.ExitCode != 0 {
		t.Fatalf("finished %v:\n%s", f, out)
	}
	for _, want := range []string{"home=" + realHome, "no ssh", "no state", "leak=none", "secret=s3cret", "\n" + filepath.Join(j.Dir(), "jobs", "sbx1") + "\n"} {
		if !strings.Contains(out, want) {
			t.Errorf("output lacks %q:\n%s", want, out)
		}
	}
	var procs int
	for _, line := range strings.Split(out, "\n") {
		if v, ok := strings.CutPrefix(line, "procs="); ok {
			procs, _ = strconv.Atoi(v)
		}
	}
	if procs == 0 || procs > 10 {
		t.Errorf("the job sees %d processes, not its own PID namespace:\n%s", procs, out)
	}
	var started *pb.JobStarted
	for _, ev := range c.events {
		if s := ev.GetStarted(); s != nil {
			started = s
		}
	}
	if started == nil || started.Isolation != "bwrap, cpu 100%, memory 256 MiB" {
		t.Errorf("started %v", started)
	}
	if b, err := os.ReadFile(filepath.Join(j.Dir(), "home", "cache-marker")); err != nil || string(b) != "cached\n" {
		t.Errorf("job home not kept: %q %v", b, err)
	}
}

func TestSandboxedJobLimitsAndCancelLive(t *testing.T) {
	if runtime.GOOS != "linux" {
		t.Skip("bwrap sandboxes are linux only")
	}
	j := newJobs(t, 2)
	j.Home = t.TempDir()
	if iso := j.Isolation(); iso.Sandbox != "bwrap" {
		t.Skipf("no sandbox on this host: %s", iso.Note)
	}
	t.Run("memory cap kills the job", func(t *testing.T) {
		c := &collector{}
		err := j.Run(context.Background(), &pb.RunJobRequest{
			JobId: "sbx-oom", Command: []string{"sh", "-c", "head -c 400M /dev/zero | tail >/dev/null && echo survived"},
			Timeout: durationpb.New(time.Minute), Limits: &pb.JobLimits{MemoryBytes: 128 << 20},
		}, c.send)
		if err != nil {
			t.Fatal(err)
		}
		if strings.Contains(c.output(), "survived") && c.finished().ExitCode == 0 {
			t.Skipf("memory controller not delegated to the user manager here:\n%s", c.output())
		}
	})
	t.Run("cancel kills detached children", func(t *testing.T) {
		marker := "301.25"
		c := &collector{}
		done := make(chan error, 1)
		go func() {
			done <- j.Run(context.Background(), &pb.RunJobRequest{
				JobId: "sbx-cancel", Command: []string{"sh", "-c", "setsid sleep " + marker + " & sleep " + marker},
				Timeout: durationpb.New(time.Minute),
			}, c.send)
		}()
		deadline := time.Now().Add(10 * time.Second)
		for !hasProcess(marker) {
			if time.Now().After(deadline) {
				t.Fatal("job never started")
			}
			time.Sleep(20 * time.Millisecond)
		}
		j.Cancel("sbx-cancel")
		if err := <-done; err != nil {
			t.Fatal(err)
		}
		if f := c.finished(); f == nil || f.End != pb.JobEnd_JOB_END_CANCELED {
			t.Fatalf("finished %v", f)
		}
		for deadline = time.Now().Add(5 * time.Second); hasProcess(marker); time.Sleep(20 * time.Millisecond) {
			if time.Now().After(deadline) {
				t.Fatal("a sandboxed process outlived the cancel")
			}
		}
	})
}

// hasProcess reports whether a process with arg in its command line runs.
func hasProcess(arg string) bool {
	ents, _ := os.ReadDir("/proc")
	for _, e := range ents {
		b, err := os.ReadFile(filepath.Join("/proc", e.Name(), "cmdline"))
		if err == nil && strings.Contains(string(b), "sleep\x00"+arg) {
			return true
		}
	}
	return false
}

// TestSandboxJobDirUnderHomeLeavesNoMountPoints covers the usual layout,
// the job dir inside ~/.protectorate: bwrap makes a mount point for the job
// directory inside the job home, which must go when the job ends.
func TestSandboxJobDirUnderHomeLeavesNoMountPoints(t *testing.T) {
	if runtime.GOOS != "linux" {
		t.Skip("bwrap sandboxes are linux only")
	}
	home := t.TempDir()
	t.Setenv("HOME", home)
	state := filepath.Join(home, ".protectorate")
	j, err := NewJobs(filepath.Join(state, "node-jobs"), 1)
	if err != nil {
		t.Fatal(err)
	}
	j.Home = state
	if iso := j.Isolation(); iso.Sandbox != "bwrap" {
		t.Skipf("no sandbox on this host: %s", iso.Note)
	}
	c := &collector{}
	if err := j.Run(context.Background(), &pb.RunJobRequest{JobId: "sbx-home", Command: []string{"sh", "-c", "pwd; ls -A \"$HOME\""}, Timeout: durationpb.New(time.Minute)}, c.send); err != nil {
		t.Fatal(err)
	}
	if f := c.finished(); f == nil || f.ExitCode != 0 {
		t.Fatalf("finished %v:\n%s", f, c.output())
	}
	leftover := filepath.Join(j.Dir(), "home", ".protectorate", "node-jobs", "jobs", "sbx-home")
	if _, err := os.Stat(leftover); !os.IsNotExist(err) {
		t.Fatalf("mount point %s left behind (%v)", leftover, err)
	}
	if strings.Contains(c.output(), "tsnet") || strings.Contains(c.output(), "token") {
		t.Fatalf("state visible in the job home:\n%s", c.output())
	}
}
