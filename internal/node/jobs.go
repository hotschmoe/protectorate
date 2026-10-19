package node

import (
	"archive/zip"
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"hash"
	"io"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

const (
	// ChunkSize is the frame size of uploads and artifact streams.
	ChunkSize = 1 << 20

	maxLogBytes       = 64 << 20
	killGrace         = 3 * time.Second
	waitDelay         = 5 * time.Second
	uploadIdle        = time.Hour
	defaultJobTimeout = time.Hour
	knownCommits      = 8
	// retention is how long a finished job's output and artifacts are held
	// for the hub to collect.
	retention = time.Hour
)

var (
	idRe     = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$`)
	commitRe = regexp.MustCompile(`^([0-9a-f]{40}|[0-9a-f]{64})$`)
)

// Jobs executes jobs for the hub. Layout under Dir:
//
//	repos/<key>/       persistent git worktree of one hub repository
//	jobs/<id>/         scratch space of one job, removed when it ends
//	runs/<id>/         output (out.log) and collected artifacts of one job,
//	                   kept for an hour after it ends
//
// A job runs independently of whoever started it: the hub can disconnect,
// restart and attach again.
type Jobs struct {
	dir   string
	slots chan struct{}
	now   func() time.Time

	mu      sync.Mutex
	uploads map[string]*upload
	runs    map[string]*jobRun
	repos   map[string]chan struct{}
}

type upload struct {
	mu   sync.Mutex
	f    *os.File
	h    hash.Hash
	size uint64
	// last and done are atomic because the idle sweep reads them without mu.
	last atomic.Int64
	done atomic.Bool
}

type jobRun struct {
	cancel   context.CancelFunc
	canceled atomic.Bool
	dir      string

	mu        sync.Mutex
	wake      chan struct{}
	logf      *os.File
	logSize   uint64
	started   *pb.JobStarted
	fin       *pb.JobFinished
	finAt     time.Time
	artifacts []string
}

func (r *jobRun) signal() {
	close(r.wake)
	r.wake = make(chan struct{})
}

func (r *jobRun) setStarted(s *pb.JobStarted) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.started = s
	r.signal()
}

func (r *jobRun) write(p []byte) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	n, err := r.logf.Write(p)
	r.logSize += uint64(n)
	r.signal()
	return err
}

func (r *jobRun) finish(f *pb.JobFinished, now time.Time) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.logf.Close()
	r.fin, r.finAt = f, now
	r.signal()
}

func (r *jobRun) done() bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	return r.fin != nil
}

// NewJobs prepares dir and clears the scratch space left by a previous
// process. concurrency bounds simultaneously running commands.
func NewJobs(dir string, concurrency int) (*Jobs, error) {
	if concurrency < 1 {
		return nil, errors.New("job concurrency must be at least 1")
	}
	dir, err := filepath.Abs(dir)
	if err != nil {
		return nil, err
	}
	if err := os.MkdirAll(filepath.Join(dir, "repos"), 0o700); err != nil {
		return nil, fmt.Errorf("create job dir: %w", err)
	}
	for _, sub := range []string{"jobs", "runs"} {
		if err := os.RemoveAll(filepath.Join(dir, sub)); err != nil {
			return nil, fmt.Errorf("clear stale %s: %w", sub, err)
		}
		if err := os.MkdirAll(filepath.Join(dir, sub), 0o700); err != nil {
			return nil, fmt.Errorf("create job dir: %w", err)
		}
	}
	return &Jobs{
		dir:     dir,
		now:     time.Now,
		slots:   make(chan struct{}, concurrency),
		uploads: map[string]*upload{},
		runs:    map[string]*jobRun{},
		repos:   map[string]chan struct{}{},
	}, nil
}

// Dir is the job root.
func (j *Jobs) Dir() string { return j.dir }

func (j *Jobs) jobDir(id string) string { return filepath.Join(j.dir, "jobs", id) }

func (j *Jobs) runDir(id string) string { return filepath.Join(j.dir, "runs", id) }

func badArg(format string, args ...any) error {
	return connect.NewError(connect.CodeInvalidArgument, fmt.Errorf(format, args...))
}

// Prepare reports what the node's cache holds for a repository.
func (j *Jobs) Prepare(ctx context.Context, req *pb.PrepareJobRequest) (*pb.PrepareJobResponse, error) {
	if !idRe.MatchString(req.RepoKey) {
		return nil, badArg("invalid repo key")
	}
	if !commitRe.MatchString(req.Commit) {
		return nil, badArg("commit must be a full object id")
	}
	repo := filepath.Join(j.dir, "repos", req.RepoKey)
	res := &pb.PrepareJobResponse{}
	if _, err := os.Stat(filepath.Join(repo, ".git")); err != nil {
		return res, nil
	}
	_, err := gitOut(ctx, repo, "cat-file", "-e", req.Commit+"^{commit}")
	res.HasCommit = err == nil
	out, err := gitOut(ctx, repo, "for-each-ref", "--sort=-committerdate", fmt.Sprintf("--count=%d", knownCommits), "--format=%(objectname)", "refs/pc/")
	if err == nil {
		res.KnownCommits = strings.Fields(out)
	}
	return res, nil
}

// Upload appends one chunk of a job's source to its scratch directory.
func (j *Jobs) Upload(req *pb.UploadJobDataRequest) (*pb.UploadJobDataResponse, error) {
	if !idRe.MatchString(req.JobId) {
		return nil, badArg("invalid job id")
	}
	u, err := j.upload(req.JobId, req.Offset)
	if err != nil {
		return nil, err
	}
	u.mu.Lock()
	defer u.mu.Unlock()
	if u.done.Load() {
		return nil, connect.NewError(connect.CodeFailedPrecondition, errors.New("upload already complete"))
	}
	if req.Offset != u.size {
		return nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("offset %d, node has %d bytes", req.Offset, u.size))
	}
	u.last.Store(time.Now().UnixNano())
	if _, err := u.f.Write(req.Data); err != nil {
		j.dropUpload(req.JobId, u)
		return nil, connect.NewError(connect.CodeInternal, fmt.Errorf("write upload: %w", err))
	}
	u.h.Write(req.Data)
	u.size += uint64(len(req.Data))
	if req.Last {
		if got := hex.EncodeToString(u.h.Sum(nil)); got != strings.ToLower(req.Sha256) {
			j.dropUpload(req.JobId, u)
			return nil, connect.NewError(connect.CodeDataLoss, fmt.Errorf("sha256 mismatch: node has %s", got))
		}
		if err := u.f.Close(); err != nil {
			j.dropUpload(req.JobId, u)
			return nil, connect.NewError(connect.CodeInternal, err)
		}
		u.done.Store(true)
		if err := os.Rename(u.f.Name(), filepath.Join(j.jobDir(req.JobId), "upload")); err != nil {
			j.dropUpload(req.JobId, u)
			return nil, connect.NewError(connect.CodeInternal, err)
		}
	}
	return &pb.UploadJobDataResponse{Received: u.size}, nil
}

// upload finds or starts the upload of a job. An upload restarts only from
// offset zero.
func (j *Jobs) upload(id string, offset uint64) (*upload, error) {
	j.mu.Lock()
	defer j.mu.Unlock()
	for k, u := range j.uploads {
		if j.runs[k] == nil && time.Since(time.Unix(0, u.last.Load())) > uploadIdle {
			u.f.Close()
			delete(j.uploads, k)
			os.RemoveAll(j.jobDir(k))
		}
	}
	if u := j.uploads[id]; u != nil {
		if offset != 0 || u.done.Load() {
			return u, nil
		}
		u.f.Close()
		delete(j.uploads, id)
	} else if offset != 0 {
		return nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("offset %d but no upload in progress", offset))
	}
	if err := os.MkdirAll(j.jobDir(id), 0o700); err != nil {
		return nil, connect.NewError(connect.CodeInternal, err)
	}
	f, err := os.OpenFile(filepath.Join(j.jobDir(id), "upload.part"), os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o600)
	if err != nil {
		return nil, connect.NewError(connect.CodeInternal, err)
	}
	u := &upload{f: f, h: sha256.New()}
	u.last.Store(time.Now().UnixNano())
	j.uploads[id] = u
	return u, nil
}

func (j *Jobs) dropUpload(id string, u *upload) {
	u.f.Close()
	j.mu.Lock()
	delete(j.uploads, id)
	j.mu.Unlock()
	os.RemoveAll(j.jobDir(id))
}

// Cancel kills a running job's process tree. A job that already ended is
// not found.
func (j *Jobs) Cancel(id string) bool {
	j.mu.Lock()
	r := j.runs[id]
	u := j.uploads[id]
	j.mu.Unlock()
	if r == nil {
		if u == nil {
			return false
		}
		j.dropUpload(id, u)
		return true
	}
	if r.done() {
		return false
	}
	r.canceled.Store(true)
	r.cancel()
	return true
}

// sweepRuns drops the output of jobs that ended more than retention ago. The
// caller holds j.mu.
func (j *Jobs) sweepRuns() {
	now := j.now()
	for id, r := range j.runs {
		r.mu.Lock()
		old := r.fin != nil && now.Sub(r.finAt) > retention
		r.mu.Unlock()
		if old {
			delete(j.runs, id)
			os.RemoveAll(r.dir)
		}
	}
}

// List reports the jobs the node is running or holds the result of.
func (j *Jobs) List() []*pb.NodeJob {
	j.mu.Lock()
	j.sweepRuns()
	ids := make([]string, 0, len(j.runs))
	runs := make([]*jobRun, 0, len(j.runs))
	for id, r := range j.runs {
		ids = append(ids, id)
		runs = append(runs, r)
	}
	j.mu.Unlock()
	out := make([]*pb.NodeJob, len(runs))
	for i, r := range runs {
		r.mu.Lock()
		nj := &pb.NodeJob{JobId: ids[i], Started: r.started != nil, Finished: r.fin, LogBytes: r.logSize}
		if r.fin != nil {
			nj.FinishedAt = timestamppb.New(r.finAt)
		}
		r.mu.Unlock()
		out[i] = nj
	}
	sort.Slice(out, func(a, b int) bool { return out[a].JobId < out[b].JobId })
	return out
}

func (j *Jobs) lockRepo(ctx context.Context, key string) (func(), error) {
	j.mu.Lock()
	ch := j.repos[key]
	if ch == nil {
		ch = make(chan struct{}, 1)
		j.repos[key] = ch
	}
	j.mu.Unlock()
	select {
	case ch <- struct{}{}:
		return func() { <-ch }, nil
	case <-ctx.Done():
		return nil, ctx.Err()
	}
}

// ValidArtifactName reports whether name is a plain file name on every OS.
func ValidArtifactName(name string) bool {
	return name != "" && name != "." && name != ".." && !strings.ContainsAny(name, "/\\:\x00") && len(name) <= 200
}

func validateRun(req *pb.RunJobRequest) error {
	switch {
	case !idRe.MatchString(req.JobId):
		return badArg("invalid job id")
	case len(req.Command) == 0 || req.Command[0] == "":
		return badArg("command is empty")
	case req.RepoKey != "" && req.ArtifactName != "":
		return badArg("a job has one source")
	case req.RepoKey != "" && (!idRe.MatchString(req.RepoKey) || !commitRe.MatchString(req.Commit)):
		return badArg("invalid repo key or commit")
	case req.ArtifactName != "" && !ValidArtifactName(req.ArtifactName):
		return badArg("invalid artifact name")
	case req.ArtifactArchive && (req.RepoKey != "" || req.ArtifactName != ""):
		return badArg("a job has one source")
	case req.Workdir != "" && (!req.ArtifactArchive || !fs.ValidPath(req.Workdir)):
		return badArg("invalid workdir %q", req.Workdir)
	}
	for k := range req.Env {
		if k == "" || strings.ContainsAny(k, "=\x00") {
			return badArg("invalid environment variable name %q", k)
		}
	}
	for _, g := range req.ArtifactGlobs {
		if _, err := filepath.Match(g, ""); err != nil || !fs.ValidPath(g) {
			return badArg("invalid artifact glob %q", g)
		}
	}
	return nil
}

// Run starts one job and streams it from the beginning through send. A job
// that cannot be set up ends with a JOB_END_ERROR event, not a failed call.
// The job outlives the stream: when ctx ends Run returns and the job keeps
// going until it exits or times out; Attach picks it up again.
func (j *Jobs) Run(ctx context.Context, req *pb.RunJobRequest, send func(*pb.JobEvent) error) error {
	if err := j.start(req); err != nil {
		return err
	}
	return j.Attach(ctx, req.JobId, 0, send)
}

func (j *Jobs) start(req *pb.RunJobRequest) error {
	if err := validateRun(req); err != nil {
		return err
	}
	runCtx, cancel := context.WithCancel(context.Background())
	r := &jobRun{cancel: cancel, dir: j.runDir(req.JobId), wake: make(chan struct{})}
	j.mu.Lock()
	j.sweepRuns()
	if j.runs[req.JobId] != nil {
		j.mu.Unlock()
		cancel()
		return connect.NewError(connect.CodeAlreadyExists, errors.New("job is already running"))
	}
	if err := os.MkdirAll(r.dir, 0o700); err != nil {
		j.mu.Unlock()
		cancel()
		return connect.NewError(connect.CodeInternal, err)
	}
	f, err := os.OpenFile(filepath.Join(r.dir, "out.log"), os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o600)
	if err != nil {
		j.mu.Unlock()
		cancel()
		return connect.NewError(connect.CodeInternal, err)
	}
	r.logf = f
	j.runs[req.JobId] = r
	j.mu.Unlock()
	go j.exec(runCtx, cancel, r, req)
	return nil
}

func (j *Jobs) exec(ctx context.Context, cancel context.CancelFunc, r *jobRun, req *pb.RunJobRequest) {
	defer cancel()
	jobDir := j.jobDir(req.JobId)
	fin := j.runJob(ctx, r, req, jobDir)
	j.mu.Lock()
	delete(j.uploads, req.JobId)
	j.mu.Unlock()
	os.RemoveAll(jobDir)
	r.finish(fin, j.now())
}

func (j *Jobs) runJob(ctx context.Context, r *jobRun, req *pb.RunJobRequest, jobDir string) *pb.JobFinished {
	setupFailed := func(err error) *pb.JobFinished {
		if r.canceled.Load() {
			return &pb.JobFinished{End: pb.JobEnd_JOB_END_CANCELED}
		}
		return &pb.JobFinished{End: pb.JobEnd_JOB_END_ERROR, ExitCode: -1, Error: err.Error()}
	}

	if req.RepoKey != "" {
		unlock, err := j.lockRepo(ctx, req.RepoKey)
		if err != nil {
			return setupFailed(err)
		}
		defer unlock()
	}
	select {
	case j.slots <- struct{}{}:
		defer func() { <-j.slots }()
	case <-ctx.Done():
		return setupFailed(ctx.Err())
	}

	workdir, reused, err := j.workspace(ctx, req, jobDir)
	if err != nil {
		return setupFailed(err)
	}
	r.setStarted(&pb.JobStarted{WorkspaceReused: reused})

	timeout := req.Timeout.AsDuration()
	if timeout <= 0 {
		timeout = defaultJobTimeout
	}
	fin := j.execute(ctx, r, req, workdir, jobDir, timeout)
	if fin.End == pb.JobEnd_JOB_END_EXITED || fin.End == pb.JobEnd_JOB_END_TIMED_OUT {
		files, err := collectArtifacts(workdir, req.ArtifactGlobs, filepath.Join(r.dir, "artifacts"))
		if err != nil && fin.Error == "" {
			fin.Error = "collect artifacts: " + err.Error()
		}
		r.mu.Lock()
		r.artifacts = files
		r.mu.Unlock()
	}
	return fin
}

// Attach streams a job from an output offset: started, the output from
// offset, then the artifacts and the result once it ended. It returns when
// the result is sent or ctx ends; the job is not affected either way.
func (j *Jobs) Attach(ctx context.Context, id string, offset uint64, send func(*pb.JobEvent) error) error {
	j.mu.Lock()
	j.sweepRuns()
	r := j.runs[id]
	j.mu.Unlock()
	if r == nil {
		return connect.NewError(connect.CodeNotFound, fmt.Errorf("node has no job %q", id))
	}
	f, err := os.Open(filepath.Join(r.dir, "out.log"))
	if err != nil {
		return connect.NewError(connect.CodeInternal, err)
	}
	defer f.Close()
	buf := make([]byte, ChunkSize)
	sentStarted := false
	for {
		r.mu.Lock()
		started, fin, size, wake, files := r.started, r.fin, r.logSize, r.wake, r.artifacts
		r.mu.Unlock()
		if offset > size {
			return connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("offset %d, node has %d bytes of output", offset, size))
		}
		if started != nil && !sentStarted {
			sentStarted = true
			if err := send(&pb.JobEvent{Event: &pb.JobEvent_Started{Started: started}}); err != nil {
				return err
			}
		}
		for offset < size {
			n, err := f.ReadAt(buf[:min(uint64(len(buf)), size-offset)], int64(offset))
			if n > 0 {
				ev := &pb.JobEvent{Event: &pb.JobEvent_Output{Output: &pb.JobOutput{Data: append([]byte(nil), buf[:n]...)}}}
				if err := send(ev); err != nil {
					return err
				}
				offset += uint64(n)
			}
			if err != nil && n == 0 {
				return connect.NewError(connect.CodeInternal, err)
			}
		}
		if fin != nil {
			abuf := make([]byte, ChunkSize)
			for _, rel := range files {
				if err := sendFile(ctx, filepath.Join(r.dir, "artifacts", filepath.FromSlash(rel)), rel, abuf, send); err != nil {
					return err
				}
			}
			return send(&pb.JobEvent{Event: &pb.JobEvent_Finished{Finished: fin}})
		}
		select {
		case <-wake:
		case <-ctx.Done():
			return ctx.Err()
		}
	}
}

// workspace readies the working directory: the persistent worktree for git
// jobs, the scratch directory for the rest.
func (j *Jobs) workspace(ctx context.Context, req *pb.RunJobRequest, jobDir string) (workdir string, reused bool, err error) {
	switch {
	case req.RepoKey != "":
		return j.checkout(ctx, req, jobDir)
	case req.ArtifactArchive:
		root := filepath.Join(jobDir, "work")
		if err := extractZip(filepath.Join(jobDir, "upload"), root); err != nil {
			return "", false, fmt.Errorf("extract artifact: %w", err)
		}
		workdir := root
		if req.Workdir != "" {
			workdir = filepath.Join(root, filepath.FromSlash(req.Workdir))
		}
		if fi, err := os.Stat(workdir); err != nil || !fi.IsDir() {
			return "", false, fmt.Errorf("workdir %q is not in the artifact", req.Workdir)
		}
		return workdir, false, nil
	case req.ArtifactName != "":
		dst := filepath.Join(jobDir, req.ArtifactName)
		if err := os.Rename(filepath.Join(jobDir, "upload"), dst); err != nil {
			return "", false, fmt.Errorf("artifact was not uploaded: %w", err)
		}
		if err := os.Chmod(dst, 0o755); err != nil {
			return "", false, err
		}
		return jobDir, false, nil
	}
	if err := os.MkdirAll(jobDir, 0o700); err != nil {
		return "", false, err
	}
	return jobDir, false, nil
}

func (j *Jobs) checkout(ctx context.Context, req *pb.RunJobRequest, jobDir string) (string, bool, error) {
	repo := filepath.Join(j.dir, "repos", req.RepoKey)
	_, statErr := os.Stat(filepath.Join(repo, ".git"))
	reused := statErr == nil
	if !reused {
		if err := os.MkdirAll(repo, 0o700); err != nil {
			return "", false, err
		}
		if _, err := gitOut(ctx, repo, "init", "--quiet"); err != nil {
			return "", false, err
		}
	}
	ref := "refs/pc/" + req.Commit
	if req.BundleUploaded {
		bundle := filepath.Join(jobDir, "upload")
		if _, err := gitOut(ctx, repo, "fetch", "--quiet", bundle, "+"+ref+":"+ref); err != nil {
			return "", false, err
		}
	}
	if _, err := gitOut(ctx, repo, "cat-file", "-e", req.Commit+"^{commit}"); err != nil {
		return "", false, fmt.Errorf("commit %s is not in the node's cache and no bundle was sent", req.Commit)
	}
	if _, err := gitOut(ctx, repo, "update-ref", ref, req.Commit); err != nil {
		return "", false, err
	}
	if _, err := gitOut(ctx, repo, "checkout", "--quiet", "--force", "--detach", req.Commit); err != nil {
		return "", false, err
	}
	return repo, reused, nil
}

// gitOut runs git with settings that keep checkouts byte-identical across
// platforms and never prompt.
func gitOut(ctx context.Context, dir string, args ...string) (string, error) {
	cmd := exec.CommandContext(ctx, "git", append([]string{"-c", "core.autocrlf=false", "-c", "core.longpaths=true", "-c", "gc.auto=0"}, args...)...)
	cmd.Dir = dir
	cmd.Env = append(os.Environ(), "GIT_TERMINAL_PROMPT=0")
	var out, errb bytes.Buffer
	cmd.Stdout, cmd.Stderr = &out, &errb
	if err := cmd.Run(); err != nil {
		if errors.Is(err, exec.ErrNotFound) {
			return "", errors.New("git is not installed on this node")
		}
		return "", fmt.Errorf("git %s: %w: %s", args[0], err, strings.TrimSpace(errb.String()))
	}
	return out.String(), nil
}

func (j *Jobs) execute(ctx context.Context, r *jobRun, req *pb.RunJobRequest, workdir, jobDir string, timeout time.Duration) *pb.JobFinished {
	cmd := exec.Command(req.Command[0], req.Command[1:]...)
	cmd.Dir = workdir
	cmd.Env = append(os.Environ(), "PROTECTORATE_JOB_ID="+req.JobId, "PROTECTORATE_JOB_DIR="+jobDir)
	keys := make([]string, 0, len(req.Env))
	for k := range req.Env {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, k := range keys {
		cmd.Env = append(cmd.Env, k+"="+req.Env[k])
	}
	w := &logWriter{r: r}
	cmd.Stdout, cmd.Stderr = w, w
	cmd.WaitDelay = waitDelay

	start := time.Now()
	tree, err := startTree(cmd)
	if err != nil {
		return &pb.JobFinished{End: pb.JobEnd_JOB_END_ERROR, ExitCode: -1, Error: fmt.Sprintf("start %q: %v", req.Command[0], err)}
	}
	defer tree.close()

	tctx, stop := context.WithTimeout(ctx, timeout)
	defer stop()
	done := make(chan error, 1)
	go func() { done <- cmd.Wait() }()

	end := pb.JobEnd_JOB_END_EXITED
	var errText string
	select {
	case <-done:
	case <-tctx.Done():
		switch {
		case r.canceled.Load():
			end = pb.JobEnd_JOB_END_CANCELED
		default:
			end = pb.JobEnd_JOB_END_TIMED_OUT
			errText = fmt.Sprintf("timed out after %s", timeout)
		}
		exited := false
		if tree.interrupt() {
			select {
			case <-done:
				exited = true
			case <-time.After(killGrace):
			}
		}
		tree.kill()
		if !exited {
			<-done
		}
	}
	tree.kill()
	code := int32(-1)
	if cmd.ProcessState != nil {
		code = int32(cmd.ProcessState.ExitCode())
	}
	return &pb.JobFinished{End: end, ExitCode: code, Error: errText, RunTime: durationpb.New(time.Since(start))}
}

// logWriter appends merged stdout and stderr to the job's output file.
// Output past maxLogBytes is replaced by one notice. A failed write cancels
// the job.
type logWriter struct {
	r         *jobRun
	sent      int
	truncated bool
	failed    bool
}

const truncatedNotice = "\n[log truncated]\n"

func (w *logWriter) Write(p []byte) (int, error) {
	if w.failed || w.truncated {
		return len(p), nil
	}
	chunk := p
	if room := maxLogBytes - w.sent; len(p) > room {
		chunk = append(append([]byte(nil), p[:room]...), truncatedNotice...)
		w.truncated = true
	}
	w.sent += len(p)
	if err := w.r.write(chunk); err != nil {
		w.failed = true
		w.r.cancel()
	}
	return len(p), nil
}

// collectArtifacts copies files matching globs, relative to workdir, into dst
// and returns their slash-separated relative paths. Matched directories
// contribute the regular files below them; symlinks are skipped.
func collectArtifacts(workdir string, globs []string, dst string) ([]string, error) {
	if len(globs) == 0 {
		return nil, nil
	}
	files, err := matchArtifacts(workdir, globs)
	if err != nil {
		return nil, err
	}
	for _, rel := range files {
		if err := copyArtifact(filepath.Join(workdir, filepath.FromSlash(rel)), filepath.Join(dst, filepath.FromSlash(rel))); err != nil {
			return nil, fmt.Errorf("%s: %w", rel, err)
		}
	}
	return files, nil
}

func copyArtifact(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	fi, err := in.Stat()
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(dst), 0o700); err != nil {
		return err
	}
	out, err := os.OpenFile(dst, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o600)
	if err != nil {
		return err
	}
	if _, err := io.Copy(out, in); err != nil {
		out.Close()
		return err
	}
	if err := out.Close(); err != nil {
		return err
	}
	return os.Chmod(dst, fi.Mode().Perm())
}

func matchArtifacts(workdir string, globs []string) ([]string, error) {
	root := os.DirFS(workdir)
	seen := map[string]bool{}
	for _, g := range globs {
		matches, err := fs.Glob(root, g)
		if err != nil {
			return nil, err
		}
		for _, m := range matches {
			fi, err := os.Lstat(filepath.Join(workdir, filepath.FromSlash(m)))
			if err != nil {
				continue
			}
			switch {
			case fi.Mode().IsRegular():
				seen[m] = true
			case fi.IsDir():
				_ = fs.WalkDir(root, m, func(p string, d fs.DirEntry, err error) error {
					if err == nil && d.Type().IsRegular() {
						seen[p] = true
					}
					return nil
				})
			}
		}
	}
	out := make([]string, 0, len(seen))
	for p := range seen {
		out = append(out, p)
	}
	sort.Strings(out)
	return out, nil
}

func sendFile(ctx context.Context, path, rel string, buf []byte, send func(*pb.JobEvent) error) error {
	f, err := os.Open(path)
	if err != nil {
		return err
	}
	defer f.Close()
	var mode uint32
	if fi, err := f.Stat(); err == nil {
		mode = uint32(fi.Mode().Perm())
	}
	first := true
	for {
		n, err := io.ReadFull(f, buf)
		last := errors.Is(err, io.EOF) || errors.Is(err, io.ErrUnexpectedEOF)
		if err != nil && !last {
			return err
		}
		if ctx.Err() != nil {
			return ctx.Err()
		}
		chunk := &pb.JobArtifactChunk{Path: rel, Data: append([]byte(nil), buf[:n]...), Last: last}
		if first {
			chunk.Mode, first = mode, false
		}
		ev := &pb.JobEvent{Event: &pb.JobEvent_Artifact{Artifact: chunk}}
		if err := send(ev); err != nil {
			return err
		}
		if last {
			return nil
		}
	}
}

const (
	maxArchiveBytes   = 4 << 30
	maxArchiveEntries = 200000
)

// extractZip unpacks the zip at src below dst, keeping permission bits where
// the OS has them. Entries that escape dst and symlinks are refused.
func extractZip(src, dst string) error {
	zr, err := zip.OpenReader(src)
	if err != nil {
		return err
	}
	defer zr.Close()
	if len(zr.File) > maxArchiveEntries {
		return fmt.Errorf("archive has %d entries", len(zr.File))
	}
	if err := os.MkdirAll(dst, 0o700); err != nil {
		return err
	}
	var total uint64
	for _, zf := range zr.File {
		if !fs.ValidPath(strings.TrimSuffix(zf.Name, "/")) || zf.Mode()&fs.ModeSymlink != 0 {
			return fmt.Errorf("unsafe archive entry %q", zf.Name)
		}
		total += zf.UncompressedSize64
		if total > maxArchiveBytes {
			return errors.New("archive is too large")
		}
		path := filepath.Join(dst, filepath.FromSlash(zf.Name))
		if zf.FileInfo().IsDir() {
			if err := os.MkdirAll(path, 0o700); err != nil {
				return err
			}
			continue
		}
		if err := extractZipFile(zf, path); err != nil {
			return fmt.Errorf("%s: %w", zf.Name, err)
		}
	}
	return nil
}

func extractZipFile(zf *zip.File, path string) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	rc, err := zf.Open()
	if err != nil {
		return err
	}
	defer rc.Close()
	f, err := os.OpenFile(path, os.O_CREATE|os.O_EXCL|os.O_WRONLY, zf.Mode().Perm()|0o600)
	if err != nil {
		return err
	}
	if _, err := io.Copy(f, rc); err != nil {
		f.Close()
		return err
	}
	return f.Close()
}
