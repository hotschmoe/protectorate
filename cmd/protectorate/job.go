package main

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"regexp"
	"strings"
	"syscall"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

// followWindow is how long followJob keeps retrying without receiving anything.
var followWindow = 5 * time.Minute

func (o *opts) jobClient() (protectoratev1connect.JobServiceClient, error) {
	hc, err := o.httpClient()
	if err != nil {
		return nil, err
	}
	return protectoratev1connect.NewJobServiceClient(hc, o.hubURL), nil
}

// exitError carries the process exit status of a failed job.
type exitError struct {
	code int
	msg  string
}

func (e *exitError) Error() string { return e.msg }

func jobCmd(o *opts) *command {
	c := &command{Use: "job", Aliases: []string{"jobs"}, Short: "Run commands on nodes"}
	c.AddCommand(jobRunCmd(o), jobLsCmd(o), jobShowCmd(o), jobCancelCmd(o), jobLogsCmd(o), jobTestCrossCmd(o))
	return c
}

// parseTarget reads --on: a lease id, a selector such as os=windows,arch=amd64
// or gpu:adreno, or a node name. Node names have no '=' or ':'.
func parseTarget(on string) (*pb.JobTarget, error) {
	t := &pb.JobTarget{}
	switch {
	case on == "":
		return nil, errors.New("--on is required: a node, a lease id or a selector like os=windows,arch=amd64")
	case leaseIDRe.MatchString(on):
		t.Target = &pb.JobTarget_LeaseId{LeaseId: on}
	case strings.ContainsAny(on, "=:"):
		sel, err := parseSelector(on)
		if err != nil {
			return nil, err
		}
		t.Target = &pb.JobTarget_Selector{Selector: sel}
	default:
		t.Target = &pb.JobTarget_Node{Node: on}
	}
	return t, nil
}

func parseSelector(s string) (*pb.NodeSelector, error) {
	sel := &pb.NodeSelector{}
	for _, kv := range strings.Split(s, ",") {
		if k, v, ok := strings.Cut(kv, ":"); ok && (k == "gpu" || k == "npu") {
			if v == "" {
				return nil, fmt.Errorf("bad selector term %q, want %s:<vendor or name>", kv, k)
			}
			if k == "gpu" {
				sel.Gpu = v
			} else {
				sel.Npu = v
			}
			continue
		}
		k, v, ok := strings.Cut(kv, "=")
		if !ok || v == "" {
			return nil, fmt.Errorf("bad selector term %q, want key=value, gpu:<text> or npu:<text>", kv)
		}
		var err error
		switch k {
		case "os":
			sel.Os = v
		case "arch":
			sel.Arch = v
		case "mem", "min-free-mem":
			sel.MinFreeMemoryBytes, err = parseSize(v)
		case "gpu-vram", "vram":
			sel.GpuVramBytes, err = parseSize(v)
		default:
			return nil, fmt.Errorf("unknown selector key %q (os, arch, mem, gpu-vram, gpu:, npu:)", k)
		}
		if err != nil {
			return nil, err
		}
	}
	return sel, nil
}

// parseGitSource reads <repo>[@<commit>]; the repo becomes an absolute path
// because the hub resolves it.
func parseGitSource(s string) (*pb.JobSource, error) {
	repo, commit := s, ""
	if i := strings.LastIndex(s, "@"); i >= 0 {
		repo, commit = s[:i], s[i+1:]
	}
	if repo == "" {
		repo = "."
	}
	abs, err := filepath.Abs(repo)
	if err != nil {
		return nil, err
	}
	return &pb.JobSource{Kind: &pb.JobSource_Git{Git: &pb.GitSource{Repo: abs, Commit: commit}}}, nil
}

func parseEnv(pairs []string) (map[string]string, error) {
	env := map[string]string{}
	for _, p := range pairs {
		k, v, ok := strings.Cut(p, "=")
		if !ok || k == "" {
			return nil, fmt.Errorf("bad --env %q, want KEY=VALUE", p)
		}
		env[k] = v
	}
	return env, nil
}

func jobRunCmd(o *opts) *command {
	var on, gitSrc, artifact, workdir string
	var globs, envPairs []string
	var timeout time.Duration
	var admin bool
	var cpu uint32
	var mem string
	c := &command{
		Use:   "run --on <node|lease|selector> [--git <repo>[@<commit>] | --artifact <file|dir>] [--artifact-glob 'dist/*'] -- <cmd...>",
		Short: "Run a command on a node and stream its output",
		Long: `Run a command on a node chosen by --on: a node name, a lease id, or a selector
such as os=windows,arch=amd64,mem=8G or gpu:adreno,npu:hexagon (least-loaded
fresh match, never a leased node). The command runs in:

  --git repo@commit   a per-repo worktree on the node, reused by later jobs so
                      build caches (target/, node_modules) stay warm
  --artifact file     a fresh job directory holding the file, marked executable
  --artifact dir      a fresh job directory holding the directory's contents
                      (zipped, permission bits kept); --workdir picks the
                      subdirectory the command runs in
  neither             an empty job directory

Output streams live; the exit status of the command is this command's exit
status. Files matching --artifact-glob (relative to the working directory) are
copied back to $PROTECTORATE_HOME/jobs/<id>/artifacts/ on the hub.`,
		Args: minimumArgs(1),
		RunE: func(cmd *command, args []string) error {
			if cmd.ArgsLenAtDash() != 0 {
				return errors.New("usage: job run [flags] -- <cmd...>")
			}
			target, err := parseTarget(on)
			if err != nil {
				return err
			}
			req := &pb.SubmitJobRequest{Target: target, Command: args, ArtifactGlobs: globs, Timeout: durationpb.New(timeout), Admin: admin}
			memBytes, err := parseOptSize(mem)
			if err != nil {
				return err
			}
			if cpu > 0 || memBytes > 0 {
				req.Limits = &pb.JobLimits{CpuPercent: cpu, MemoryBytes: memBytes}
			}
			if req.Env, err = parseEnv(envPairs); err != nil {
				return err
			}
			switch {
			case gitSrc != "" && artifact != "":
				return errors.New("--git and --artifact are exclusive")
			case gitSrc != "":
				if req.Source, err = parseGitSource(gitSrc); err != nil {
					return err
				}
			case artifact != "":
				abs, err := filepath.Abs(artifact)
				if err != nil {
					return err
				}
				req.Source = &pb.JobSource{Kind: &pb.JobSource_Artifact{Artifact: &pb.ArtifactSource{Path: abs, Workdir: workdir}}}
			case workdir != "":
				return errors.New("--workdir needs --artifact <dir>")
			}
			jc, err := o.jobClient()
			if err != nil {
				return err
			}
			j, err := runJob(cmd.Context(), o, cmd, jc, req)
			if err != nil {
				return err
			}
			return jobExitStatus(j)
		},
	}
	f := c.Flags()
	f.StringVar(&on, "on", "", "node name, lease id or selector (os=,arch=,mem=,gpu-vram=,gpu:<text>,npu:<text>)")
	f.StringVar(&gitSrc, "git", "", "run a commit of a git repository on the hub: <repo>[@<commit>]")
	f.StringVar(&artifact, "artifact", "", "run in a job directory holding this file or directory from the hub")
	f.StringVar(&workdir, "workdir", "", "with --artifact <dir>: run in this subdirectory of it")
	f.StringArrayVar(&globs, "artifact-glob", nil, "fetch files matching this glob back to the hub (repeatable)")
	f.StringArrayVarP(&envPairs, "env", "e", nil, "environment variable KEY=VALUE (repeatable)")
	f.DurationVar(&timeout, "timeout", time.Hour, "kill the command after this long (at most 24h)")
	f.Uint32Var(&cpu, "cpu", 0, "cap a sandboxed job's CPU, in percent of one core (200 = two cores)")
	f.StringVar(&mem, "mem", "", "cap a sandboxed job's memory, e.g. 4G (at least 64M)")
	f.BoolVar(&admin, "admin", false, "run elevated (sudo -n on Linux, the agent's elevated token on Windows), unsandboxed; needs a named node and an admin grant for it")
	return c
}

func jobExitStatus(j *pb.Job) error {
	if j.State == pb.JobState_JOB_STATE_SUCCEEDED {
		return nil
	}
	code := int(j.ExitCode)
	if code < 1 || code > 255 {
		code = 1
	}
	msg := fmt.Sprintf("job %s %s", j.Id, jobStateName(j.State))
	if j.Error != "" {
		msg += ": " + j.Error
	}
	return &exitError{code: code, msg: msg}
}

func jobStateName(s pb.JobState) string {
	return strings.ToLower(strings.ReplaceAll(strings.TrimPrefix(s.String(), "JOB_STATE_"), "_", " "))
}

// newRequestID returns a fresh idempotency key for SubmitJob.
func newRequestID() (string, error) {
	var b [16]byte
	if _, err := rand.Read(b[:]); err != nil {
		return "", fmt.Errorf("request id: %w", err)
	}
	return hex.EncodeToString(b[:]), nil
}

// runJob submits req under a fresh idempotency key and streams its log to
// stdout (stderr with --json) until the job ends. An interrupt, SIGTERM or a
// canceled ctx cancels the job by that key, so it works even when the submit
// is still in flight and the job id is not known; a second interrupt quits.
func runJob(ctx context.Context, o *opts, cmd *command, jc protectoratev1connect.JobServiceClient, req *pb.SubmitJobRequest) (*pb.Job, error) {
	if req.RequestId == "" {
		key, err := newRequestID()
		if err != nil {
			return nil, err
		}
		req.RequestId = key
	}
	stderr := cmd.ErrOrStderr()
	sigc := make(chan os.Signal, 2)
	signal.Notify(sigc, os.Interrupt, syscall.SIGTERM)
	defer signal.Stop(sigc)
	finished, canceled := make(chan struct{}), make(chan struct{})
	go func() {
		defer close(canceled)
		select {
		case <-sigc:
		case <-ctx.Done():
		case <-finished:
			if ctx.Err() == nil {
				return
			}
		}
		fmt.Fprintln(stderr, "interrupted: canceling job (interrupt again to quit)")
		go func() { <-sigc; os.Exit(130) }()
		cctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
		defer cancel()
		_, _ = jc.CancelJob(cctx, connect.NewRequest(&pb.CancelJobRequest{RequestId: req.RequestId}))
	}()
	defer func() {
		close(finished)
		<-canceled
	}()

	res, err := jc.SubmitJob(ctx, connect.NewRequest(req))
	if err != nil {
		return nil, err
	}
	job := res.Msg
	fmt.Fprintf(stderr, "job %s on %s\n", job.Id, job.Node)

	logw := cmd.OutOrStdout()
	if o.json {
		logw = stderr
	}
	final, err := followJob(ctx, jc, job.Id, 0, logw)
	if err != nil {
		return nil, err
	}
	fmt.Fprintln(stderr, jobSummary(final))
	if o.json {
		if err := o.out(cmd).print(final, nil); err != nil {
			return nil, err
		}
	}
	return final, nil
}

// followJob copies the log from offset to w and returns the final record. A
// dropped stream, or a hub that is restarting, is resumed from the last
// offset received until followWindow passes with no progress.
func followJob(ctx context.Context, jc protectoratev1connect.JobServiceClient, id string, offset uint64, w io.Writer) (*pb.Job, error) {
	var last *pb.Job
	var streamErr error
	progress := time.Now()
	for delay := time.Second; time.Since(progress) < followWindow; delay = min(2*delay, 5*time.Second) {
		stream, err := jc.WatchJob(ctx, connect.NewRequest(&pb.WatchJobRequest{Id: id, FromOffset: offset, Follow: true}))
		if err == nil {
			for stream.Receive() {
				progress = time.Now()
				switch ev := stream.Msg().Event.(type) {
				case *pb.WatchJobResponse_Job:
					last = ev.Job
				case *pb.WatchJobResponse_Log:
					if _, err := w.Write(ev.Log.Data); err != nil {
						stream.Close()
						return nil, err
					}
					offset = ev.Log.Offset + uint64(len(ev.Log.Data))
				}
			}
			err = stream.Err()
			stream.Close()
			if err == nil && last != nil && jobEnded(last.State) {
				return last, nil
			}
		}
		streamErr = err
		if ctx.Err() != nil {
			return nil, ctx.Err()
		}
		if code := connect.CodeOf(err); err != nil && code != connect.CodeUnavailable && code != connect.CodeUnknown && code != connect.CodeInternal && code != connect.CodeDeadlineExceeded {
			break
		}
		select {
		case <-time.After(delay):
		case <-ctx.Done():
			return nil, ctx.Err()
		}
	}
	if streamErr == nil {
		streamErr = errors.New("log stream ended before the job did")
	}
	return nil, fmt.Errorf("watch job %s: %w", id, streamErr)
}

func jobEnded(s pb.JobState) bool {
	return s != pb.JobState_JOB_STATE_PREPARING && s != pb.JobState_JOB_STATE_RUNNING
}

func jobSummary(j *pb.Job) string {
	var b strings.Builder
	fmt.Fprintf(&b, "job %s %s", j.Id, jobStateName(j.State))
	if j.State == pb.JobState_JOB_STATE_SUCCEEDED || j.State == pb.JobState_JOB_STATE_FAILED {
		fmt.Fprintf(&b, " (exit %d)", j.ExitCode)
	}
	fmt.Fprintf(&b, " on %s: run %s", j.Node, j.RunTime.AsDuration().Round(time.Millisecond))
	if j.Source != nil {
		fmt.Fprintf(&b, ", source %s in %s", fmtBytes(j.SourceBytes), j.TransferTime.AsDuration().Round(time.Millisecond))
		if j.WorkspaceReused {
			b.WriteString(", warm worktree")
		}
	}
	if len(j.Artifacts) > 0 {
		fmt.Fprintf(&b, ", %d artifact(s) %s", len(j.Artifacts), fmtBytes(j.ArtifactBytes))
	}
	if j.Error != "" {
		b.WriteString(": " + j.Error)
	}
	return b.String()
}

func jobLsCmd(o *opts) *command {
	var state string
	var limit int32
	c := &command{
		Use:   "ls",
		Short: "List jobs, newest first",
		Args:  noArgs,
		RunE: func(cmd *command, _ []string) error {
			req := &pb.ListJobsRequest{Limit: limit}
			if state != "" {
				v, ok := pb.JobState_value["JOB_STATE_"+strings.ToUpper(strings.ReplaceAll(state, "-", "_"))]
				if !ok {
					return fmt.Errorf("unknown state %q", state)
				}
				req.State = pb.JobState(v)
			}
			jc, err := o.jobClient()
			if err != nil {
				return err
			}
			res, err := jc.ListJobs(cmd.Context(), connect.NewRequest(req))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintln(w, "ID\tSTATE\tNODE\tAGE\tRUN\tCOMMAND")
				for _, j := range res.Msg.Jobs {
					fmt.Fprintf(w, "%s\t%s\t%s\t%s\t%s\t%s\n", j.Id, jobStateName(j.State), j.Node,
						fmtDur(time.Since(j.SubmittedAt.AsTime())), j.RunTime.AsDuration().Round(time.Second), truncate(strings.Join(j.Command, " "), 48))
				}
			})
		},
	}
	c.Flags().StringVar(&state, "state", "", "only jobs in this state (running, failed, ...)")
	c.Flags().Int32Var(&limit, "limit", 0, "most jobs to list (default 50)")
	return c
}

func truncate(s string, n int) string {
	if len(s) <= n {
		return s
	}
	return s[:n-1] + "~"
}

func jobShowCmd(o *opts) *command {
	return &command{
		Use:   "show <id>",
		Short: "Show one job record",
		Args:  exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			jc, err := o.jobClient()
			if err != nil {
				return err
			}
			res, err := jc.GetJob(cmd.Context(), connect.NewRequest(&pb.GetJobRequest{Id: args[0]}))
			if err != nil {
				return err
			}
			j := res.Msg
			return o.out(cmd).print(j, func(w io.Writer) {
				fmt.Fprintf(w, "id\t%s\nstate\t%s\nnode\t%s\ncommand\t%s\n", j.Id, jobStateName(j.State), j.Node, strings.Join(j.Command, " "))
				fmt.Fprintf(w, "mode\t%s\nisolation\t%s\n", jobModeName(j.Mode), orDash(j.Isolation))
				if j.Task != "" {
					fmt.Fprintf(w, "task\t%s\n", j.Task)
				}
				switch k := j.Source.GetKind().(type) {
				case *pb.JobSource_Git:
					fmt.Fprintf(w, "source\tgit %s@%s\n", k.Git.Repo, k.Git.Commit)
				case *pb.JobSource_Artifact:
					fmt.Fprintf(w, "source\tartifact %s\n", k.Artifact.Path)
				}
				fmt.Fprintf(w, "submitted\t%s\nstarted\t%s\nfinished\t%s\n", fmtStamp(j.SubmittedAt, "-"), fmtStamp(j.StartedAt, "-"), fmtStamp(j.FinishedAt, "-"))
				fmt.Fprintf(w, "exit code\t%d\nerror\t%s\n", j.ExitCode, orDash(j.Error))
				fmt.Fprintf(w, "source sent\t%s in %s (warm worktree: %t)\n", fmtBytes(j.SourceBytes), j.TransferTime.AsDuration().Round(time.Millisecond), j.WorkspaceReused)
				fmt.Fprintf(w, "run time\t%s\nlog\t%s\n", j.RunTime.AsDuration().Round(time.Millisecond), fmtBytes(j.LogBytes))
				for _, a := range j.Artifacts {
					fmt.Fprintf(w, "artifact\t%s (%s)\n", a.Path, fmtBytes(a.SizeBytes))
				}
			})
		},
	}
}

func jobModeName(m pb.JobMode) string {
	if m == pb.JobMode_JOB_MODE_UNSPECIFIED {
		m = pb.JobMode_JOB_MODE_SANDBOXED
	}
	return strings.ToLower(strings.TrimPrefix(m.String(), "JOB_MODE_"))
}

func jobCancelCmd(o *opts) *command {
	return &command{
		Use:   "cancel <id>",
		Short: "Stop a job and its process tree",
		Args:  exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			jc, err := o.jobClient()
			if err != nil {
				return err
			}
			res, err := jc.CancelJob(cmd.Context(), connect.NewRequest(&pb.CancelJobRequest{Id: args[0]}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprintf(w, "job %s %s\n", res.Msg.Id, jobStateName(res.Msg.State)) })
		},
	}
}

func jobLogsCmd(o *opts) *command {
	var follow bool
	c := &command{
		Use:   "logs <id>",
		Short: "Print a job's output",
		Args:  exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			jc, err := o.jobClient()
			if err != nil {
				return err
			}
			if follow {
				_, err := followJob(cmd.Context(), jc, args[0], 0, cmd.OutOrStdout())
				return err
			}
			stream, err := jc.WatchJob(cmd.Context(), connect.NewRequest(&pb.WatchJobRequest{Id: args[0]}))
			if err != nil {
				return err
			}
			defer stream.Close()
			for stream.Receive() {
				if l := stream.Msg().GetLog(); l != nil {
					if _, err := cmd.OutOrStdout().Write(l.Data); err != nil {
						return err
					}
				}
			}
			return stream.Err()
		},
	}
	c.Flags().BoolVarP(&follow, "follow", "f", false, "keep streaming until the job ends")
	return c
}

var unsafeName = regexp.MustCompile(`[^A-Za-z0-9._-]+`)

func jobTestCrossCmd(o *opts) *command {
	var goos, goarch, on string
	var timeout time.Duration
	c := &command{
		Use:   "test-cross --goos <os> --goarch <arch> <packages...> [-- <test flags>]",
		Short: "Compile Go tests here and run the test binaries on a matching node",
		Long: `Runs "go test -c" for every package with GOOS/GOARCH set, uploads each test
binary as an artifact job and runs it on one node that matches the platform (a
lease pins the node for the whole run; it lives 60s and is renewed while this
command runs, so a crash frees the node within a minute). The node needs no Go toolchain. Each
binary ships in a zip with its package directory (testdata and the other files
tests open) and the module's go.mod/go.sum, laid out as in the module, and runs
with the package directory as working directory, as "go test" does. Files over
8 MiB and .git, node_modules and target directories are left out. Arguments
after -- go to the test binaries, e.g. -test.run TestX -test.v.`,
		Args: minimumArgs(1),
		RunE: func(cmd *command, args []string) error {
			pkgArgs, testArgs := args, []string(nil)
			if dash := cmd.ArgsLenAtDash(); dash >= 0 {
				pkgArgs, testArgs = args[:dash], args[dash:]
			}
			if len(pkgArgs) == 0 {
				return errors.New("name at least one package")
			}
			return testCross(cmd, o, goos, goarch, on, timeout, pkgArgs, testArgs)
		},
	}
	f := c.Flags()
	f.StringVar(&goos, "goos", "", "target GOOS (required)")
	f.StringVar(&goarch, "goarch", "", "target GOARCH (required)")
	f.StringVar(&on, "on", "", "run on this node or lease instead of picking a matching node")
	f.DurationVar(&timeout, "timeout", 10*time.Minute, "per-package run timeout")
	_ = c.MarkFlagRequired("goos")
	_ = c.MarkFlagRequired("goarch")
	return c
}

func testCross(cmd *command, o *opts, goos, goarch, on string, timeout time.Duration, patterns, testArgs []string) error {
	ctx, stop := signal.NotifyContext(cmd.Context(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	jobCtx, lose := context.WithCancelCause(ctx)
	defer lose(nil)
	stderr := cmd.ErrOrStderr()
	pkgs, err := goList(ctx, patterns)
	if err != nil {
		return err
	}
	tmp, err := os.MkdirTemp("", "protectorate-test-cross-")
	if err != nil {
		return err
	}
	defer os.RemoveAll(tmp)

	ext := ""
	if goos == "windows" {
		ext = ".exe"
	}
	type bin struct{ pkg, stage, workdir, name string }
	var bins []bin
	for i, pkg := range pkgs {
		stage := filepath.Join(tmp, fmt.Sprintf("stage%d", i))
		rel, err := stagePackage(pkg.dir, pkg.modDir, stage)
		if err != nil {
			return fmt.Errorf("stage %s: %w", pkg.path, err)
		}
		name := unsafeName.ReplaceAllString(pkg.path, "_") + ".test" + ext
		path := filepath.Join(stage, filepath.FromSlash(rel), name)
		build := exec.CommandContext(ctx, "go", "test", "-c", "-o", path, pkg.path)
		build.Env = append(os.Environ(), "GOOS="+goos, "GOARCH="+goarch, "CGO_ENABLED=0")
		build.Stderr = stderr
		if err := build.Run(); err != nil {
			return fmt.Errorf("compile tests of %s for %s/%s: %w", pkg.path, goos, goarch, err)
		}
		if _, err := os.Stat(path); err != nil {
			fmt.Fprintf(stderr, "%s: no test files\n", pkg.path)
			continue
		}
		bins = append(bins, bin{pkg.path, stage, rel, name})
	}
	if len(bins) == 0 {
		return errors.New("no package has tests")
	}

	jc, err := o.jobClient()
	if err != nil {
		return err
	}
	var target *pb.JobTarget
	if on != "" {
		if target, err = parseTarget(on); err != nil {
			return err
		}
	} else {
		lc, err := o.leaseClient()
		if err != nil {
			return err
		}
		res, err := lc.AcquireLease(ctx, connect.NewRequest(&pb.AcquireLeaseRequest{
			Target: &pb.AcquireLeaseRequest_Selector{Selector: &pb.NodeSelector{Os: goos, Arch: goarch}},
			Holder: defaultHolder() + " test-cross", Ttl: durationpb.New(testCrossLeaseTTL),
		}))
		if err != nil {
			return fmt.Errorf("find a %s/%s node: %w", goos, goarch, err)
		}
		lease := res.Msg
		fmt.Fprintf(stderr, "lease %s on %s\n", lease.Id, lease.Resource)
		renewed := make(chan struct{})
		renewCtx, stopRenew := context.WithCancel(ctx)
		ticker := time.NewTicker(testCrossLeaseTTL / 3)
		go func() {
			defer close(renewed)
			defer ticker.Stop()
			err := keepLease(renewCtx, ticker.C, func(rc context.Context) error {
				rc, cancel := context.WithTimeout(rc, testCrossLeaseTTL/3)
				defer cancel()
				_, err := lc.RenewLease(rc, connect.NewRequest(&pb.RenewLeaseRequest{Id: lease.Id}))
				return err
			})
			if err != nil {
				fmt.Fprintf(stderr, "lease %s lost: %v\n", lease.Id, err)
				lose(fmt.Errorf("lease %s lost: %w", lease.Id, err))
			}
		}()
		defer func() {
			stopRenew()
			<-renewed
			rctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
			defer cancel()
			if _, err := lc.ReleaseLease(rctx, connect.NewRequest(&pb.ReleaseLeaseRequest{Id: lease.Id})); err != nil {
				fmt.Fprintf(stderr, "release lease %s: %v (it expires after its ttl)\n", lease.Id, err)
			}
		}()
		target = &pb.JobTarget{Target: &pb.JobTarget_LeaseId{LeaseId: lease.Id}}
	}

	var failed []string
	for _, b := range bins {
		fmt.Fprintf(cmd.OutOrStdout(), "=== %s\n", b.pkg)
		exe := "./" + b.name
		if goos == "windows" {
			exe = `.\` + b.name
		}
		j, err := runJob(jobCtx, o, cmd, jc, &pb.SubmitJobRequest{
			Target:  target,
			Source:  &pb.JobSource{Kind: &pb.JobSource_Artifact{Artifact: &pb.ArtifactSource{Path: b.stage, Workdir: b.workdir}}},
			Command: append([]string{exe}, testArgs...),
			Timeout: durationpb.New(timeout),
		})
		if ctx.Err() != nil {
			return &exitError{code: 130, msg: "interrupted"}
		}
		if cause := context.Cause(jobCtx); cause != nil {
			return cause
		}
		if err != nil {
			return err
		}
		if j.State != pb.JobState_JOB_STATE_SUCCEEDED {
			failed = append(failed, b.pkg)
		}
	}
	if len(failed) > 0 {
		return &exitError{code: 1, msg: "tests failed: " + strings.Join(failed, ", ")}
	}
	return nil
}

// testCrossLeaseTTL is how long the test-cross lease outlives its last
// renewal, and so how long a crashed run keeps the node. A variable only so
// tests can shorten it.
var testCrossLeaseTTL = 60 * time.Second

// maxRenewFailures is how many renewals in a row may fail before the lease is
// given up; with a TTL/3 tick the lease is still live after the first miss.
const maxRenewFailures = 2

// keepLease calls renew on every tick until ctx ends (nil) or the lease is
// lost: renew reports it gone or invalid, or fails maxRenewFailures times in a
// row.
func keepLease(ctx context.Context, ticks <-chan time.Time, renew func(context.Context) error) error {
	failures := 0
	for {
		select {
		case <-ctx.Done():
			return nil
		case <-ticks:
		}
		err := renew(ctx)
		if err == nil {
			failures = 0
			continue
		}
		if ctx.Err() != nil {
			return nil
		}
		switch connect.CodeOf(err) {
		case connect.CodeNotFound, connect.CodeInvalidArgument, connect.CodePermissionDenied, connect.CodeUnauthenticated:
			return fmt.Errorf("renew: %w", err)
		}
		if failures++; failures >= maxRenewFailures {
			return fmt.Errorf("renew failed %d times: %w", failures, err)
		}
	}
}

type goPackage struct{ path, dir, modDir string }

func goList(ctx context.Context, patterns []string) ([]goPackage, error) {
	var out, errb bytes.Buffer
	cmd := exec.CommandContext(ctx, "go", append([]string{"list", "-f", "{{.ImportPath}}\t{{.Dir}}\t{{with .Module}}{{.Dir}}{{end}}"}, patterns...)...)
	cmd.Stdout, cmd.Stderr = &out, &errb
	if err := cmd.Run(); err != nil {
		return nil, fmt.Errorf("go list: %w: %s", err, strings.TrimSpace(errb.String()))
	}
	var pkgs []goPackage
	for _, line := range strings.Split(strings.TrimSpace(out.String()), "\n") {
		f := strings.Split(strings.TrimSpace(line), "\t")
		if len(f) != 3 || f[2] == "" {
			return nil, fmt.Errorf("go list: unexpected line %q", line)
		}
		pkgs = append(pkgs, goPackage{f[0], f[1], f[2]})
	}
	return pkgs, nil
}

const stageMaxFile = 8 << 20

var stageSkipDirs = map[string]bool{".git": true, "node_modules": true, "target": true}

// stagePackage copies a package directory and its module's go.mod/go.sum into
// stage, keeping the module-relative layout, and returns the package's
// slash-separated path below stage.
func stagePackage(pkgDir, modDir, stage string) (string, error) {
	rel, err := filepath.Rel(modDir, pkgDir)
	if err != nil || !filepath.IsLocal(rel) {
		return "", fmt.Errorf("package directory %s is outside its module %s", pkgDir, modDir)
	}
	for _, f := range []string{"go.mod", "go.sum"} {
		if err := copyFileMode(filepath.Join(modDir, f), filepath.Join(stage, f)); err != nil && !errors.Is(err, os.ErrNotExist) {
			return "", err
		}
	}
	dst := filepath.Join(stage, rel)
	err = filepath.WalkDir(pkgDir, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		r, err := filepath.Rel(pkgDir, path)
		if err != nil {
			return err
		}
		if d.IsDir() {
			if r != "." && stageSkipDirs[d.Name()] {
				return filepath.SkipDir
			}
			return os.MkdirAll(filepath.Join(dst, r), 0o755)
		}
		fi, err := d.Info()
		if err != nil || !fi.Mode().IsRegular() || fi.Size() > stageMaxFile {
			return err
		}
		return copyFileMode(path, filepath.Join(dst, r))
	})
	return filepath.ToSlash(rel), err
}

func copyFileMode(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	fi, err := in.Stat()
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return err
	}
	out, err := os.OpenFile(dst, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, fi.Mode().Perm())
	if err != nil {
		return err
	}
	if _, err := io.Copy(out, in); err != nil {
		out.Close()
		return err
	}
	return out.Close()
}
