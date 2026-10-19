package main

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"regexp"
	"strings"
	"time"

	"connectrpc.com/connect"
	"github.com/spf13/cobra"
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

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

func jobCmd(o *opts) *cobra.Command {
	c := &cobra.Command{Use: "job", Aliases: []string{"jobs"}, Short: "Run commands on nodes"}
	c.AddCommand(jobRunCmd(o), jobLsCmd(o), jobShowCmd(o), jobCancelCmd(o), jobLogsCmd(o), jobTestCrossCmd(o))
	return c
}

// parseTarget reads --on: a lease id, a selector such as os=windows,arch=amd64,
// or a node name.
func parseTarget(on string, allowFamily bool) (*pb.JobTarget, error) {
	t := &pb.JobTarget{AllowFamily: allowFamily}
	switch {
	case on == "":
		return nil, errors.New("--on is required: a node, a lease id or a selector like os=windows,arch=amd64")
	case leaseIDRe.MatchString(on):
		t.Target = &pb.JobTarget_LeaseId{LeaseId: on}
	case strings.Contains(on, "="):
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
		k, v, ok := strings.Cut(kv, "=")
		if !ok || v == "" {
			return nil, fmt.Errorf("bad selector term %q, want key=value", kv)
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
			return nil, fmt.Errorf("unknown selector key %q (os, arch, mem, gpu-vram)", k)
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

func jobRunCmd(o *opts) *cobra.Command {
	var on, gitSrc, artifact string
	var globs, envPairs []string
	var timeout time.Duration
	var allowFamily bool
	c := &cobra.Command{
		Use:   "run --on <node|lease|selector> [--git <repo>[@<commit>] | --artifact <file>] [--artifact-glob 'dist/*'] -- <cmd...>",
		Short: "Run a command on a node and stream its output",
		Long: `Run a command on a node chosen by --on: a node name, a lease id, or a selector
such as os=windows,arch=amd64,mem=8G (least-loaded fresh match, never a leased
node). The command runs in:

  --git repo@commit   a per-repo worktree on the node, reused by later jobs so
                      build caches (target/, node_modules) stay warm
  --artifact file     a fresh job directory holding the file, marked executable
  neither             an empty job directory

Output streams live; the exit status of the command is this command's exit
status. Files matching --artifact-glob (relative to the working directory) are
copied back to $PROTECTORATE_HOME/jobs/<id>/artifacts/ on the hub.`,
		Args: cobra.MinimumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if cmd.ArgsLenAtDash() != 0 {
				return errors.New("usage: job run [flags] -- <cmd...>")
			}
			target, err := parseTarget(on, allowFamily)
			if err != nil {
				return err
			}
			req := &pb.SubmitJobRequest{Target: target, Command: args, ArtifactGlobs: globs, Timeout: durationpb.New(timeout)}
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
				req.Source = &pb.JobSource{Kind: &pb.JobSource_Artifact{Artifact: &pb.ArtifactSource{Path: abs}}}
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
	f.StringVar(&on, "on", "", "node name, lease id or selector (os=,arch=,mem=,gpu-vram=)")
	f.StringVar(&gitSrc, "git", "", "run a commit of a git repository on the hub: <repo>[@<commit>]")
	f.StringVar(&artifact, "artifact", "", "run in a job directory holding this file from the hub")
	f.StringArrayVar(&globs, "artifact-glob", nil, "fetch files matching this glob back to the hub (repeatable)")
	f.StringArrayVarP(&envPairs, "env", "e", nil, "environment variable KEY=VALUE (repeatable)")
	f.DurationVar(&timeout, "timeout", time.Hour, "kill the command after this long (at most 24h)")
	f.BoolVar(&allowFamily, "allow-family", false, "allow nodes of tier family")
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

// runJob submits req and streams its log to stdout (stderr with --json) until
// the job ends. Interrupt cancels the job; a second interrupt quits.
func runJob(ctx context.Context, o *opts, cmd *cobra.Command, jc protectoratev1connect.JobServiceClient, req *pb.SubmitJobRequest) (*pb.Job, error) {
	res, err := jc.SubmitJob(ctx, connect.NewRequest(req))
	if err != nil {
		return nil, err
	}
	job := res.Msg
	stderr := cmd.ErrOrStderr()
	fmt.Fprintf(stderr, "job %s on %s\n", job.Id, job.Node)

	sigc := make(chan os.Signal, 2)
	signal.Notify(sigc, os.Interrupt)
	defer signal.Stop(sigc)
	go func() {
		select {
		case <-sigc:
		case <-ctx.Done():
			return
		}
		fmt.Fprintln(stderr, "interrupted: canceling job (interrupt again to quit)")
		go func() { <-sigc; os.Exit(130) }()
		cctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
		defer cancel()
		_, _ = jc.CancelJob(cctx, connect.NewRequest(&pb.CancelJobRequest{Id: job.Id}))
	}()

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
// dropped stream is resumed from the last offset a few times.
func followJob(ctx context.Context, jc protectoratev1connect.JobServiceClient, id string, offset uint64, w io.Writer) (*pb.Job, error) {
	var last *pb.Job
	var streamErr error
	for attempt := 0; attempt < 5; attempt++ {
		if attempt > 0 {
			select {
			case <-time.After(time.Second):
			case <-ctx.Done():
				return nil, ctx.Err()
			}
		}
		stream, err := jc.WatchJob(ctx, connect.NewRequest(&pb.WatchJobRequest{Id: id, FromOffset: offset, Follow: true}))
		if err != nil {
			streamErr = err
			continue
		}
		for stream.Receive() {
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
		streamErr = stream.Err()
		stream.Close()
		if streamErr == nil && last != nil {
			return last, nil
		}
		if connect.CodeOf(streamErr) == connect.CodeNotFound {
			break
		}
	}
	if streamErr == nil {
		streamErr = errors.New("log stream ended without a job record")
	}
	return nil, fmt.Errorf("watch job %s: %w", id, streamErr)
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

func jobLsCmd(o *opts) *cobra.Command {
	var state string
	var limit int32
	c := &cobra.Command{
		Use:   "ls",
		Short: "List jobs, newest first",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
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

func jobShowCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "show <id>",
		Short: "Show one job record",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
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

func jobCancelCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "cancel <id>",
		Short: "Stop a job and its process tree",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
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

func jobLogsCmd(o *opts) *cobra.Command {
	var follow bool
	c := &cobra.Command{
		Use:   "logs <id>",
		Short: "Print a job's output",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			jc, err := o.jobClient()
			if err != nil {
				return err
			}
			stream, err := jc.WatchJob(cmd.Context(), connect.NewRequest(&pb.WatchJobRequest{Id: args[0], Follow: follow}))
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

func jobTestCrossCmd(o *opts) *cobra.Command {
	var goos, goarch, on string
	var ttl, timeout time.Duration
	var allowFamily bool
	c := &cobra.Command{
		Use:   "test-cross --goos <os> --goarch <arch> <packages...> [-- <test flags>]",
		Short: "Compile Go tests here and run the test binaries on a matching node",
		Long: `Runs "go test -c" for every package with GOOS/GOARCH set, uploads each test
binary as an artifact job and runs it on one node that matches the platform (a
lease pins the node for the whole run). The node needs no Go toolchain. Tests
run in an empty directory, so tests that read files next to their sources
(testdata) will not find them. Arguments after -- go to the test binaries,
e.g. -test.run TestX -test.v.`,
		Args: cobra.MinimumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			pkgArgs, testArgs := args, []string(nil)
			if dash := cmd.ArgsLenAtDash(); dash >= 0 {
				pkgArgs, testArgs = args[:dash], args[dash:]
			}
			if len(pkgArgs) == 0 {
				return errors.New("name at least one package")
			}
			return testCross(cmd, o, goos, goarch, on, ttl, timeout, allowFamily, pkgArgs, testArgs)
		},
	}
	f := c.Flags()
	f.StringVar(&goos, "goos", "", "target GOOS (required)")
	f.StringVar(&goarch, "goarch", "", "target GOARCH (required)")
	f.StringVar(&on, "on", "", "run on this node or lease instead of picking a matching node")
	f.DurationVar(&ttl, "ttl", 30*time.Minute, "lease lifetime for the picked node")
	f.DurationVar(&timeout, "timeout", 10*time.Minute, "per-package run timeout")
	f.BoolVar(&allowFamily, "allow-family", false, "allow nodes of tier family")
	_ = c.MarkFlagRequired("goos")
	_ = c.MarkFlagRequired("goarch")
	return c
}

func testCross(cmd *cobra.Command, o *opts, goos, goarch, on string, ttl, timeout time.Duration, allowFamily bool, patterns, testArgs []string) error {
	ctx := cmd.Context()
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
	type bin struct{ pkg, path, name string }
	var bins []bin
	for _, pkg := range pkgs {
		name := unsafeName.ReplaceAllString(pkg, "_") + ".test" + ext
		path := filepath.Join(tmp, name)
		build := exec.CommandContext(ctx, "go", "test", "-c", "-o", path, pkg)
		build.Env = append(os.Environ(), "GOOS="+goos, "GOARCH="+goarch, "CGO_ENABLED=0")
		build.Stderr = stderr
		if err := build.Run(); err != nil {
			return fmt.Errorf("compile tests of %s for %s/%s: %w", pkg, goos, goarch, err)
		}
		if _, err := os.Stat(path); err != nil {
			fmt.Fprintf(stderr, "%s: no test files\n", pkg)
			continue
		}
		bins = append(bins, bin{pkg, path, name})
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
		if target, err = parseTarget(on, allowFamily); err != nil {
			return err
		}
	} else {
		lc, err := o.leaseClient()
		if err != nil {
			return err
		}
		res, err := lc.AcquireLease(ctx, connect.NewRequest(&pb.AcquireLeaseRequest{
			Target: &pb.AcquireLeaseRequest_Selector{Selector: &pb.NodeSelector{Os: goos, Arch: goarch}},
			Holder: defaultHolder() + " test-cross", Ttl: durationpb.New(ttl), AllowFamily: allowFamily,
		}))
		if err != nil {
			return fmt.Errorf("find a %s/%s node: %w", goos, goarch, err)
		}
		lease := res.Msg
		fmt.Fprintf(stderr, "lease %s on %s\n", lease.Id, lease.Resource)
		defer func() {
			_, _ = lc.ReleaseLease(context.Background(), connect.NewRequest(&pb.ReleaseLeaseRequest{Id: lease.Id}))
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
		j, err := runJob(ctx, o, cmd, jc, &pb.SubmitJobRequest{
			Target:  target,
			Source:  &pb.JobSource{Kind: &pb.JobSource_Artifact{Artifact: &pb.ArtifactSource{Path: b.path}}},
			Command: append([]string{exe}, testArgs...),
			Timeout: durationpb.New(timeout),
		})
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

func goList(ctx context.Context, patterns []string) ([]string, error) {
	var out, errb bytes.Buffer
	cmd := exec.CommandContext(ctx, "go", append([]string{"list"}, patterns...)...)
	cmd.Stdout, cmd.Stderr = &out, &errb
	if err := cmd.Run(); err != nil {
		return nil, fmt.Errorf("go list: %w: %s", err, strings.TrimSpace(errb.String()))
	}
	return strings.Fields(out.String()), nil
}
