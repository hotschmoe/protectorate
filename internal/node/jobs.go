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
)

var (
	idRe     = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$`)
	commitRe = regexp.MustCompile(`^([0-9a-f]{40}|[0-9a-f]{64})$`)
)

// Jobs executes jobs for the hub. Layout under Dir:
//
//	repos/<key>/       persistent git worktree of one hub repository
//	jobs/<id>/         scratch space of one job, removed when it ends
type Jobs struct {
	dir   string
	slots chan struct{}

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
	scratch := filepath.Join(dir, "jobs")
	if err := os.RemoveAll(scratch); err != nil {
		return nil, fmt.Errorf("clear stale jobs: %w", err)
	}
	if err := os.MkdirAll(scratch, 0o700); err != nil {
		return nil, fmt.Errorf("create job dir: %w", err)
	}
	return &Jobs{
		dir:     dir,
		slots:   make(chan struct{}, concurrency),
		uploads: map[string]*upload{},
		runs:    map[string]*jobRun{},
		repos:   map[string]chan struct{}{},
	}, nil
}

// Dir is the job root.
func (j *Jobs) Dir() string { return j.dir }

func (j *Jobs) jobDir(id string) string { return filepath.Join(j.dir, "jobs", id) }

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

// Cancel kills a running job's process tree.
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
	r.canceled.Store(true)
	r.cancel()
	return true
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

// Run executes one job and reports through send. The job dies with ctx. A job
// that cannot be set up ends with a JOB_END_ERROR event, not a failed call.
func (j *Jobs) Run(ctx context.Context, req *pb.RunJobRequest, send func(*pb.JobEvent) error) error {
	if err := validateRun(req); err != nil {
		return err
	}
	runCtx, cancel := context.WithCancel(ctx)
	defer cancel()
	r := &jobRun{cancel: cancel}
	j.mu.Lock()
	if j.runs[req.JobId] != nil {
		j.mu.Unlock()
		return connect.NewError(connect.CodeAlreadyExists, errors.New("job is already running"))
	}
	j.runs[req.JobId] = r
	j.mu.Unlock()
	jobDir := j.jobDir(req.JobId)
	defer func() {
		j.mu.Lock()
		delete(j.runs, req.JobId)
		delete(j.uploads, req.JobId)
		j.mu.Unlock()
		os.RemoveAll(jobDir)
	}()

	finish := func(f *pb.JobFinished) error {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		return send(&pb.JobEvent{Event: &pb.JobEvent_Finished{Finished: f}})
	}
	setupFailed := func(err error) error {
		if r.canceled.Load() {
			return finish(&pb.JobFinished{End: pb.JobEnd_JOB_END_CANCELED})
		}
		if ctx.Err() != nil {
			return ctx.Err()
		}
		return finish(&pb.JobFinished{End: pb.JobEnd_JOB_END_ERROR, ExitCode: -1, Error: err.Error()})
	}

	if req.RepoKey != "" {
		unlock, err := j.lockRepo(runCtx, req.RepoKey)
		if err != nil {
			return setupFailed(err)
		}
		defer unlock()
	}
	select {
	case j.slots <- struct{}{}:
		defer func() { <-j.slots }()
	case <-runCtx.Done():
		return setupFailed(runCtx.Err())
	}

	workdir, reused, err := j.workspace(runCtx, req, jobDir)
	if err != nil {
		return setupFailed(err)
	}
	if err := send(&pb.JobEvent{Event: &pb.JobEvent_Started{Started: &pb.JobStarted{WorkspaceReused: reused}}}); err != nil {
		return err
	}

	timeout := req.Timeout.AsDuration()
	if timeout <= 0 {
		timeout = defaultJobTimeout
	}
	fin := j.execute(runCtx, r, req, workdir, jobDir, timeout, send)
	if fin.End == pb.JobEnd_JOB_END_EXITED || fin.End == pb.JobEnd_JOB_END_TIMED_OUT {
		if err := sendArtifacts(runCtx, workdir, req.ArtifactGlobs, send); err != nil && fin.Error == "" {
			fin.Error = "send artifacts: " + err.Error()
		}
	}
	return finish(fin)
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

func (j *Jobs) execute(ctx context.Context, r *jobRun, req *pb.RunJobRequest, workdir, jobDir string, timeout time.Duration, send func(*pb.JobEvent) error) *pb.JobFinished {
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
	w := &logWriter{send: send, onFail: r.cancel}
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
		case ctx.Err() != nil:
			end = pb.JobEnd_JOB_END_CANCELED
			errText = "caller disconnected"
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

// logWriter forwards merged stdout and stderr as output events. Output past
// maxLogBytes is replaced by one notice. A failed send cancels the job.
type logWriter struct {
	send      func(*pb.JobEvent) error
	onFail    func()
	sent      int
	truncated bool
	failed    bool
}

const truncatedNotice = "\n[log truncated]\n"

func (w *logWriter) Write(p []byte) (int, error) {
	if w.failed || w.truncated {
		return len(p), nil
	}
	chunk := append([]byte(nil), p...)
	if room := maxLogBytes - w.sent; len(p) > room {
		chunk = append(chunk[:room], truncatedNotice...)
		w.truncated = true
	}
	w.sent += len(p)
	if err := w.send(&pb.JobEvent{Event: &pb.JobEvent_Output{Output: &pb.JobOutput{Data: chunk}}}); err != nil {
		w.failed = true
		w.onFail()
	}
	return len(p), nil
}

// sendArtifacts streams files matching globs, relative to workdir. Matched
// directories contribute the regular files below them; symlinks are skipped.
func sendArtifacts(ctx context.Context, workdir string, globs []string, send func(*pb.JobEvent) error) error {
	files, err := matchArtifacts(workdir, globs)
	if err != nil {
		return err
	}
	buf := make([]byte, ChunkSize)
	for _, rel := range files {
		if err := sendFile(ctx, filepath.Join(workdir, filepath.FromSlash(rel)), rel, buf, send); err != nil {
			return err
		}
	}
	return nil
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
