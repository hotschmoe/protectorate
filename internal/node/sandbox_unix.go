//go:build !windows

package node

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"time"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

// jobUnitPrefix starts the systemd scope of every sandboxed job.
const jobUnitPrefix = "prot-job-"

// sandbox runs jobs in the sleeve sandbox (internal/sleeve Box): bwrap in a
// systemd user scope, the job's directories the only writable binds, the
// user's real home replaced by the node's job home.
type sandbox struct {
	cfg      sleeve.Config
	home     string
	hostname string
}

// probeIsolation reports how this node runs jobs and readies the sandbox
// when one works here.
func (j *Jobs) probeIsolation() *pb.JobIsolation {
	iso := &pb.JobIsolation{Sandbox: "none"}
	if sb, err := j.newSandbox(); err != nil {
		iso.Note = "jobs run as the agent's user, unsandboxed: " + err.Error()
	} else {
		j.box, iso.Sandbox = sb, "bwrap"
	}
	iso.Admin, iso.Note = probeAdmin(iso.Note)
	return iso
}

func (j *Jobs) newSandbox() (*sandbox, error) {
	if runtime.GOOS != "linux" {
		return nil, fmt.Errorf("no bwrap sandbox on %s", runtime.GOOS)
	}
	if j.Home == "" {
		return nil, errors.New("the agent has no state directory to keep out of jobs")
	}
	cfg, err := sleeve.DefaultConfig(j.Home, nil)
	if err != nil {
		return nil, err
	}
	cfg.UnitPrefix = jobUnitPrefix
	if err := cfg.Preflight(); err != nil {
		return nil, err
	}
	if err := cfg.CheckBind(j.dir); err != nil {
		return nil, fmt.Errorf("job directory: %w", err)
	}
	sb := &sandbox{cfg: cfg, home: filepath.Join(j.dir, "home"), hostname: Hostname()}
	if err := os.MkdirAll(sb.home, 0o700); err != nil {
		return nil, fmt.Errorf("create job home: %w", err)
	}
	if err := sb.trial(); err != nil {
		return nil, err
	}
	return sb, nil
}

// trial runs `true` in a sandbox once, so a node whose bwrap or user
// systemd cannot work here (no user namespaces, no user manager) says so
// instead of failing every job.
func (s *sandbox) trial() error {
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	argv := s.cfg.BoxArgv(sleeve.Box{
		Unit: jobUnitPrefix + "probe-" + strconv.Itoa(os.Getpid()), Hostname: s.hostname, Home: s.home,
		Chdir: "/", Command: []string{"true"},
	})
	cmd := exec.CommandContext(ctx, argv[0], argv[1:]...)
	var out bytes.Buffer
	cmd.Stdout, cmd.Stderr = &out, &out
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("sandbox trial failed: %w: %s", err, strings.TrimSpace(out.String()))
	}
	return nil
}

// forget removes the empty mount point bwrap made in the job home for a job
// directory under the user's home, so finished jobs leave nothing behind.
func (s *sandbox) forget(jobDir string) {
	rel, err := filepath.Rel(s.cfg.UserHome, jobDir)
	if err != nil || !filepath.IsLocal(rel) {
		return
	}
	_ = os.Remove(filepath.Join(s.home, rel))
}

// env starts with a clean job environment; the job's own variables win.
func (s *sandbox) env(jobEnv [][2]string) [][2]string {
	uh := s.cfg.UserHome
	env := [][2]string{
		{"HOME", uh},
		{"PATH", strings.Join(s.cfg.PathDirs, ":")},
		{"USER", s.cfg.User},
		{"LOGNAME", s.cfg.User},
		{"SHELL", "/bin/sh"},
		{"LANG", "C.UTF-8"},
		{"TERM", "dumb"},
		// ~/go is a read-only tool dir; modules go to the job home.
		{"GOMODCACHE", filepath.Join(uh, ".cache", "go-mod")},
	}
	return append(env, jobEnv...)
}

// plan is the argv of a sandboxed job and the --setenv arguments bwrap reads
// from fd 3. The working directory (a warm worktree or inside the job
// directory) and the job directory are the only writable binds besides the
// job home.
func (s *sandbox) plan(req *pb.RunJobRequest, workdir, jobDir string, jobEnv [][2]string) ([]string, []byte, error) {
	lim := sleeve.Limits{CPUPercent: req.GetLimits().GetCpuPercent(), MemoryBytes: req.GetLimits().GetMemoryBytes()}
	if err := lim.Validate(); err != nil {
		return nil, nil, err
	}
	rw := []string{jobDir}
	if !within(jobDir, workdir) {
		rw = append(rw, workdir)
	}
	for _, d := range rw {
		if err := s.cfg.CheckBind(d); err != nil {
			return nil, nil, err
		}
	}
	argv := s.cfg.BoxArgv(sleeve.Box{
		Unit: jobUnitPrefix + req.JobId, Hostname: s.hostname, Limits: lim, Home: s.home,
		RW: rw, Chdir: workdir, EnvFD: true, Command: req.Command,
	})
	return argv, sleeve.SetenvArgs(s.env(jobEnv)), nil
}
