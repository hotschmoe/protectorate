package hub

import (
	"archive/zip"
	"cmp"
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"log"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/encoding/protojson"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/protobuf/types/known/structpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/fsutil"
	"github.com/hotschmoe/protectorate/internal/node"
)

const (
	defaultJobTimeout = time.Hour
	maxJobTimeout     = 24 * time.Hour
	rpcTimeout        = time.Minute
	cancelBackstop    = 10 * time.Second
	cancelWait        = 8 * time.Second
	logReadChunk      = 64 << 10
	minJobMemory      = 64 << 20
)

var jobIDRe = regexp.MustCompile(`^[0-9]{8}-[0-9]{6}-[0-9a-f]{8}$`)

// nodeExec is the job half of a node agent, in-process for the hub's own node.
type nodeExec interface {
	Prepare(ctx context.Context, req *pb.PrepareJobRequest) (*pb.PrepareJobResponse, error)
	Upload(ctx context.Context, req *pb.UploadJobDataRequest) (*pb.UploadJobDataResponse, error)
	Run(ctx context.Context, req *pb.RunJobRequest, fn func(*pb.JobEvent) error) error
	Cancel(ctx context.Context, jobID string) error
}

type localExec struct{ jobs *node.Jobs }

func (l localExec) Prepare(ctx context.Context, req *pb.PrepareJobRequest) (*pb.PrepareJobResponse, error) {
	return l.jobs.Prepare(ctx, req)
}

func (l localExec) Upload(_ context.Context, req *pb.UploadJobDataRequest) (*pb.UploadJobDataResponse, error) {
	return l.jobs.Upload(req)
}

func (l localExec) Run(ctx context.Context, req *pb.RunJobRequest, fn func(*pb.JobEvent) error) error {
	return l.jobs.Run(ctx, req, fn)
}

func (l localExec) Cancel(_ context.Context, id string) error {
	l.jobs.Cancel(id)
	return nil
}

type remoteExec struct {
	c protectoratev1connect.NodeAgentServiceClient
}

func (r remoteExec) Prepare(ctx context.Context, req *pb.PrepareJobRequest) (*pb.PrepareJobResponse, error) {
	ctx, cancel := context.WithTimeout(ctx, rpcTimeout)
	defer cancel()
	res, err := r.c.PrepareJob(ctx, connect.NewRequest(req))
	if err != nil {
		return nil, err
	}
	return res.Msg, nil
}

func (r remoteExec) Upload(ctx context.Context, req *pb.UploadJobDataRequest) (*pb.UploadJobDataResponse, error) {
	ctx, cancel := context.WithTimeout(ctx, rpcTimeout)
	defer cancel()
	res, err := r.c.UploadJobData(ctx, connect.NewRequest(req))
	if err != nil {
		return nil, err
	}
	return res.Msg, nil
}

func (r remoteExec) Run(ctx context.Context, req *pb.RunJobRequest, fn func(*pb.JobEvent) error) error {
	stream, err := r.c.RunJob(ctx, connect.NewRequest(req))
	if err != nil {
		return err
	}
	defer stream.Close()
	for stream.Receive() {
		if err := fn(stream.Msg()); err != nil {
			return err
		}
	}
	return stream.Err()
}

func (r remoteExec) Cancel(ctx context.Context, id string) error {
	ctx, cancel := context.WithTimeout(ctx, rpcTimeout)
	defer cancel()
	_, err := r.c.CancelJob(ctx, connect.NewRequest(&pb.NodeAgentServiceCancelJobRequest{JobId: id}))
	return err
}

func terminal(s pb.JobState) bool {
	switch s {
	case pb.JobState_JOB_STATE_PREPARING, pb.JobState_JOB_STATE_RUNNING:
		return false
	}
	return true
}

type jobEntry struct {
	dir    string
	node   string
	cancel context.CancelFunc

	mu              sync.Mutex
	job             *pb.Job
	wake            chan struct{}
	log             *os.File
	streaming       bool
	cancelRequested bool
}

func (e *jobEntry) snapshot() (*pb.Job, <-chan struct{}) {
	e.mu.Lock()
	defer e.mu.Unlock()
	return proto.Clone(e.job).(*pb.Job), e.wake
}

func (e *jobEntry) state() pb.JobState {
	e.mu.Lock()
	defer e.mu.Unlock()
	return e.job.State
}

// JobManager runs the hub side of jobs: it places them, ships sources, relays
// logs and artifacts and keeps the records under dir.
type JobManager struct {
	dir    string
	nodes  *NodeMonitor
	leases *LeaseTable
	local  *node.Jobs
	emit   func(kind, subject string, payload *structpb.Struct)

	// submitMu makes placement and registration atomic, so concurrent
	// selector submits see each other's load.
	submitMu sync.Mutex

	keys *submitKeys

	mu   sync.Mutex
	jobs map[string]*jobEntry

	ctx    context.Context
	cancel context.CancelFunc
	wg     sync.WaitGroup
}

// OpenJobs loads the job records under dir. Jobs that were still active when
// the previous hub stopped are marked failed.
func OpenJobs(dir string, nodes *NodeMonitor, leases *LeaseTable, local *node.Jobs, emit func(kind, subject string, payload *structpb.Struct)) (*JobManager, error) {
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return nil, fmt.Errorf("create jobs dir: %w", err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	m := &JobManager{dir: dir, nodes: nodes, leases: leases, local: local, emit: emit, jobs: map[string]*jobEntry{}, keys: newSubmitKeys(time.Now), ctx: ctx, cancel: cancel}
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil, err
	}
	for _, de := range entries {
		if !de.IsDir() || !jobIDRe.MatchString(de.Name()) {
			continue
		}
		e, err := m.load(filepath.Join(dir, de.Name()))
		if err != nil {
			log.Printf("jobs: skip %s: %v", de.Name(), err)
			continue
		}
		m.jobs[e.job.Id] = e
	}
	return m, nil
}

func (m *JobManager) load(dir string) (*jobEntry, error) {
	raw, err := os.ReadFile(filepath.Join(dir, "job.json"))
	if err != nil {
		return nil, err
	}
	j := &pb.Job{}
	if err := (protojson.UnmarshalOptions{DiscardUnknown: true}).Unmarshal(raw, j); err != nil {
		return nil, err
	}
	if fi, err := os.Stat(filepath.Join(dir, "job.log")); err == nil {
		j.LogBytes = uint64(fi.Size())
	}
	e := &jobEntry{dir: dir, node: j.Node, job: j, wake: make(chan struct{})}
	if !terminal(j.State) {
		j.State, j.Error = pb.JobState_JOB_STATE_FAILED, "hub restarted while the job was active"
		j.FinishedAt = timestamppb.Now()
		if err := writeJob(dir, j); err != nil {
			return nil, err
		}
	}
	return e, nil
}

func writeJob(dir string, j *pb.Job) error {
	raw, err := protojson.MarshalOptions{Multiline: true}.Marshal(j)
	if err != nil {
		return err
	}
	return fsutil.WriteFile(filepath.Join(dir, "job.json"), raw, 0o600)
}

// update applies fn to the record, persists it and wakes watchers. State
// changes are announced as job.updated.
func (m *JobManager) update(e *jobEntry, fn func(*pb.Job)) {
	e.mu.Lock()
	before := e.job.State
	fn(e.job)
	if err := writeJob(e.dir, e.job); err != nil {
		log.Printf("jobs: persist %s: %v", e.job.Id, err)
	}
	after := e.job.State
	id := e.job.Id
	close(e.wake)
	e.wake = make(chan struct{})
	e.mu.Unlock()
	if after != before && m.emit != nil {
		p, _ := structpb.NewStruct(map[string]any{"state": after.String(), "node": e.node})
		m.emit("job.updated", id, p)
	}
}

func (m *JobManager) appendLog(e *jobEntry, data []byte) {
	e.mu.Lock()
	defer e.mu.Unlock()
	if _, err := e.log.Write(data); err != nil {
		log.Printf("jobs: write log %s: %v", e.job.Id, err)
		return
	}
	e.job.LogBytes += uint64(len(data))
	close(e.wake)
	e.wake = make(chan struct{})
}

func (m *JobManager) get(id string) (*jobEntry, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	e := m.jobs[id]
	if e == nil {
		return nil, connect.NewError(connect.CodeNotFound, fmt.Errorf("job %q not found", id))
	}
	return e, nil
}

// busy counts the active jobs placed on a node.
func (m *JobManager) busy(name string) int {
	m.mu.Lock()
	defer m.mu.Unlock()
	n := 0
	for _, e := range m.jobs {
		if e.node == name && !terminal(e.state()) {
			n++
		}
	}
	return n
}

// warmNodes lists the nodes that already ran a job from repo and so hold its
// worktree.
func (m *JobManager) warmNodes(repo string) map[string]bool {
	if repo == "" {
		return nil
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	warm := map[string]bool{}
	for _, e := range m.jobs {
		e.mu.Lock()
		if g := e.job.GetSource().GetGit(); g != nil && g.Repo == repo && e.job.StartedAt != nil {
			warm[e.node] = true
		}
		e.mu.Unlock()
	}
	return warm
}

func (m *JobManager) execFor(n *pb.NodeInfo) nodeExec {
	if n.Local {
		return localExec{m.local}
	}
	return remoteExec{m.nodes.agentClient(n)}
}

func invalid(format string, args ...any) error {
	return connect.NewError(connect.CodeInvalidArgument, fmt.Errorf(format, args...))
}

func validateSubmit(req *pb.SubmitJobRequest) (time.Duration, error) {
	if len(req.Command) == 0 || req.Command[0] == "" {
		return 0, invalid("command is empty")
	}
	timeout := req.Timeout.AsDuration()
	switch {
	case timeout < 0 || timeout > maxJobTimeout:
		return 0, invalid("timeout must be at most %s", maxJobTimeout)
	case timeout == 0:
		timeout = defaultJobTimeout
	}
	for k := range req.Env {
		if k == "" || len(k) > 256 {
			return 0, invalid("invalid environment variable name %q", k)
		}
	}
	for _, g := range req.ArtifactGlobs {
		if _, err := filepath.Match(g, ""); err != nil || !fs.ValidPath(g) {
			return 0, invalid("invalid artifact glob %q", g)
		}
	}
	if m := req.GetLimits().GetMemoryBytes(); m != 0 && m < minJobMemory {
		return 0, invalid("a memory limit must be at least 64 MiB")
	}
	return timeout, nil
}

// sourcePlan is a validated source ready to ship.
type sourcePlan struct {
	repo, commit, key string
	file, name        string
	dir, workdir      string
}

func (m *JobManager) planSource(ctx context.Context, src *pb.JobSource) (*sourcePlan, *pb.JobSource, error) {
	switch k := src.GetKind().(type) {
	case nil:
		return &sourcePlan{}, nil, nil
	case *pb.JobSource_Git:
		repo, commit, err := resolveRepoCommit(ctx, k.Git.Repo, k.Git.Commit)
		if err != nil {
			return nil, nil, connect.NewError(connect.CodeInvalidArgument, err)
		}
		return &sourcePlan{repo: repo, commit: commit, key: repoKey(repo)},
			&pb.JobSource{Kind: &pb.JobSource_Git{Git: &pb.GitSource{Repo: repo, Commit: commit}}}, nil
	case *pb.JobSource_Artifact:
		abs, err := filepath.Abs(k.Artifact.Path)
		if err != nil {
			return nil, nil, invalid("artifact path: %v", err)
		}
		fi, err := os.Stat(abs)
		if err != nil {
			return nil, nil, invalid("artifact %q is not a file or directory on the hub", k.Artifact.Path)
		}
		if fi.IsDir() {
			wd := k.Artifact.Workdir
			if wd != "" && !fs.ValidPath(wd) {
				return nil, nil, invalid("invalid artifact workdir %q", wd)
			}
			return &sourcePlan{dir: abs, workdir: wd},
				&pb.JobSource{Kind: &pb.JobSource_Artifact{Artifact: &pb.ArtifactSource{Path: abs, Workdir: wd}}}, nil
		}
		if !fi.Mode().IsRegular() || k.Artifact.Workdir != "" {
			return nil, nil, invalid("artifact %q is not a file on the hub, or workdir was given for a file", k.Artifact.Path)
		}
		name := filepath.Base(abs)
		if !node.ValidArtifactName(name) {
			return nil, nil, invalid("artifact file name %q is not usable on a node", name)
		}
		return &sourcePlan{file: abs, name: name},
			&pb.JobSource{Kind: &pb.JobSource_Artifact{Artifact: &pb.ArtifactSource{Path: abs}}}, nil
	}
	return nil, nil, invalid("unknown source kind")
}

// place picks the node for a target. The caller holds submitMu. A named
// node or a lease's node of tier care needs the caller in ctx to be admitted
// by careGate; a selector never picks one.
func (m *JobManager) place(ctx context.Context, target *pb.JobTarget, repo string) (n *pb.NodeInfo, gpu *int, err error) {
	var name string
	switch t := target.GetTarget().(type) {
	case *pb.JobTarget_LeaseId:
		l, err := m.leases.Get(t.LeaseId)
		if err != nil {
			return nil, nil, err
		}
		if strings.HasPrefix(l.Node, "desk:") {
			return nil, nil, invalid("lease %s is on %s, which is a desktop and cannot run jobs", t.LeaseId, l.Resource)
		}
		name = l.Node
		if l.GpuIndex != nil {
			g := int(*l.GpuIndex)
			gpu = &g
		}
		if err := m.nodes.namedNode(name, gpu); err != nil {
			return nil, nil, err
		}
		if err := m.nodes.careGate(ctx, name); err != nil {
			return nil, nil, err
		}
	case *pb.JobTarget_Node:
		name = strings.ToLower(t.Node)
		if err := m.nodes.namedNode(name, nil); err != nil {
			return nil, nil, err
		}
		if err := m.nodes.careGate(ctx, name); err != nil {
			return nil, nil, err
		}
		if l := m.leases.NodeLease(name); l != nil {
			return nil, nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("node %s is leased by %s", name, l.Holder))
		}
	case *pb.JobTarget_Selector:
		blocked := func(node string, gpu *int) *pb.Lease {
			if gpu == nil {
				return m.leases.NodeLease(node)
			}
			return m.leases.Blocked(node, gpu)
		}
		name, gpu, err = pickNode(m.nodes.Statuses(), t.Selector, m.busy, blocked, m.warmNodes(repo))
		if err != nil {
			return nil, nil, err
		}
	default:
		return nil, nil, invalid("set a target: lease_id, node or selector")
	}
	n, err = m.nodes.info(name)
	if err != nil {
		return nil, nil, nodeErr(err)
	}
	return n, gpu, nil
}

// jobMode decides how the node runs the command: admin when asked for and
// allowed, plain (the agent's user, unsandboxed) on a care node, which the
// job maintains, sandboxed everywhere else.
func (m *JobManager) jobMode(ctx context.Context, req *pb.SubmitJobRequest, n *pb.NodeInfo) (pb.JobMode, error) {
	if !req.Admin {
		if n.Tier == TierCare {
			return pb.JobMode_JOB_MODE_PLAIN, nil
		}
		return pb.JobMode_JOB_MODE_SANDBOXED, nil
	}
	if req.Target.GetSelector() != nil {
		return 0, invalid("an admin job names its node (or a lease), not a selector")
	}
	if p := auth.PrincipalFrom(ctx); !p.AdminOn(n.Name) {
		return 0, connect.NewError(connect.CodePermissionDenied, fmt.Errorf("admin jobs on %s need an admin grant for it (protectorate grant admin --node %s --task \"...\")", n.Name, n.Name))
	}
	st, err := m.nodes.Status(n.Name)
	if err != nil {
		return 0, nodeErr(err)
	}
	iso := st.Metrics.GetJobs()
	if iso == nil {
		return 0, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("node %s does not report whether it can run admin jobs: update its agent", n.Name))
	}
	if iso.Admin == "" {
		return 0, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("node %s cannot run admin jobs: %s", n.Name, cmp.Or(iso.Note, "it has no way to elevate")))
	}
	return pb.JobMode_JOB_MODE_ADMIN, nil
}

// maxAuditCommand bounds the command text kept in an audit line.
const maxAuditCommand = 512

// jobAudit is the audit detail of a submitted job. The command is kept for
// admin jobs and for jobs under a grant, whose every command is audited.
func jobAudit(node string, mode pb.JobMode, command []string, withCommand bool) string {
	d := "node " + node + ", " + modeLabel(mode)
	if withCommand {
		c := strconv.Quote(strings.Join(command, " "))
		if len(c) > maxAuditCommand {
			c = c[:maxAuditCommand] + "..."
		}
		d += ", command " + c
	}
	return d
}

func modeLabel(mode pb.JobMode) string {
	switch mode {
	case pb.JobMode_JOB_MODE_ADMIN:
		return "admin (elevated, unsandboxed)"
	case pb.JobMode_JOB_MODE_PLAIN:
		return "plain (agent user, unsandboxed)"
	}
	return "sandboxed"
}

func newJobID() (string, error) {
	var b [4]byte
	if _, err := rand.Read(b[:]); err != nil {
		return "", err
	}
	return time.Now().UTC().Format("20060102-150405") + "-" + hex.EncodeToString(b[:]), nil
}

// Submit validates, places and registers a job, then runs it in the
// background.
func (m *JobManager) Submit(ctx context.Context, req *pb.SubmitJobRequest) (*pb.Job, error) {
	if req.RequestId != "" {
		return m.keys.submit(ctx, req.RequestId, func() (*pb.Job, error) { return m.submit(ctx, req) }, m.snapshotOf)
	}
	return m.submit(ctx, req)
}

func (m *JobManager) submit(ctx context.Context, req *pb.SubmitJobRequest) (*pb.Job, error) {
	timeout, err := validateSubmit(req)
	if err != nil {
		return nil, err
	}
	plan, src, err := m.planSource(ctx, req.Source)
	if err != nil {
		return nil, err
	}
	if m.ctx.Err() != nil {
		return nil, connect.NewError(connect.CodeUnavailable, errors.New("hub is shutting down"))
	}

	m.submitMu.Lock()
	defer m.submitMu.Unlock()
	n, gpu, err := m.place(ctx, req.Target, plan.repo)
	if err != nil {
		return nil, err
	}
	mode, err := m.jobMode(ctx, req, n)
	if err != nil {
		return nil, err
	}
	p := auth.PrincipalFrom(ctx)
	id, err := newJobID()
	if err != nil {
		return nil, connect.NewError(connect.CodeInternal, err)
	}
	auth.Annotate(ctx, id, jobAudit(n.Name, mode, req.Command, mode == pb.JobMode_JOB_MODE_ADMIN || p.GetTask() != ""))
	dir := filepath.Join(m.dir, id)
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return nil, connect.NewError(connect.CodeInternal, err)
	}
	logf, err := os.OpenFile(filepath.Join(dir, "job.log"), os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o600)
	if err != nil {
		return nil, connect.NewError(connect.CodeInternal, err)
	}
	envKeys := make([]string, 0, len(req.Env))
	for k := range req.Env {
		envKeys = append(envKeys, k)
	}
	sort.Strings(envKeys)
	job := &pb.Job{
		Id: id, State: pb.JobState_JOB_STATE_PREPARING, Target: req.Target, Node: n.Name, Source: src,
		Command: req.Command, EnvKeys: envKeys, Timeout: durationpb.New(timeout), ArtifactGlobs: req.ArtifactGlobs,
		SubmittedAt: timestamppb.Now(), Mode: mode, Task: p.GetTask(), Limits: req.Limits,
	}
	if gpu != nil {
		g := int32(*gpu)
		job.GpuIndex = &g
	}
	jctx, cancel := context.WithCancel(m.ctx)
	e := &jobEntry{dir: dir, node: n.Name, cancel: cancel, job: job, wake: make(chan struct{}), log: logf}
	if err := writeJob(dir, job); err != nil {
		cancel()
		logf.Close()
		return nil, connect.NewError(connect.CodeInternal, err)
	}
	m.mu.Lock()
	m.jobs[id] = e
	m.mu.Unlock()
	if m.emit != nil {
		p, _ := structpb.NewStruct(map[string]any{"state": job.State.String(), "node": n.Name})
		m.emit("job.updated", id, p)
	}

	env := map[string]string{}
	if gpu != nil {
		idx := strconv.Itoa(*gpu)
		env["PROTECTORATE_GPU"], env["CUDA_VISIBLE_DEVICES"], env["HIP_VISIBLE_DEVICES"] = idx, idx, idx
	}
	for k, v := range req.Env {
		env[k] = v
	}
	run := &pb.RunJobRequest{
		JobId: id, Command: req.Command, Env: env, Timeout: durationpb.New(timeout), ArtifactGlobs: req.ArtifactGlobs,
		RepoKey: plan.key, Commit: plan.commit, ArtifactName: plan.name,
		ArtifactArchive: plan.dir != "", Workdir: plan.workdir, Mode: mode, Limits: req.Limits,
	}
	snap := proto.Clone(job).(*pb.Job)
	m.wg.Add(1)
	go m.run(jctx, e, m.execFor(n), plan, run)
	return snap, nil
}

func (m *JobManager) run(ctx context.Context, e *jobEntry, ex nodeExec, plan *sourcePlan, run *pb.RunJobRequest) {
	defer m.wg.Done()
	defer e.cancel()
	err := m.drive(ctx, e, ex, plan, run)
	if err != nil {
		cctx, cancel := context.WithTimeout(context.Background(), cancelBackstop)
		_ = ex.Cancel(cctx, run.JobId)
		cancel()
	}
	e.mu.Lock()
	canceled := e.cancelRequested
	e.mu.Unlock()
	e.log.Close()
	m.update(e, func(j *pb.Job) {
		if j.FinishedAt == nil {
			j.FinishedAt = timestamppb.Now()
		}
		if terminal(j.State) {
			return
		}
		switch {
		case canceled:
			j.State, j.Error = pb.JobState_JOB_STATE_CANCELED, "canceled"
		case m.ctx.Err() != nil:
			j.State, j.Error = pb.JobState_JOB_STATE_FAILED, "hub stopped"
		default:
			j.State, j.Error = pb.JobState_JOB_STATE_FAILED, err.Error()
		}
		j.ExitCode = -1
	})
}

func (m *JobManager) drive(ctx context.Context, e *jobEntry, ex nodeExec, plan *sourcePlan, run *pb.RunJobRequest) error {
	t0 := time.Now()
	var sent uint64
	var err error
	switch {
	case plan.key != "":
		sent, err = m.shipGit(ctx, ex, plan, run)
	case plan.file != "":
		sent, err = shipFile(ctx, ex, run.JobId, plan.file)
	case plan.dir != "":
		sent, err = shipDir(ctx, ex, run.JobId, plan.dir)
	}
	if err != nil {
		return fmt.Errorf("ship source: %w", err)
	}
	m.update(e, func(j *pb.Job) {
		j.SourceBytes, j.TransferTime = sent, durationpb.New(time.Since(t0))
	})
	return m.stream(ctx, e, ex, run)
}

func (m *JobManager) shipGit(ctx context.Context, ex nodeExec, plan *sourcePlan, run *pb.RunJobRequest) (uint64, error) {
	prep, err := ex.Prepare(ctx, &pb.PrepareJobRequest{JobId: run.JobId, RepoKey: plan.key, Commit: plan.commit})
	if err != nil {
		return 0, fmt.Errorf("prepare: %w", err)
	}
	if prep.HasCommit {
		return 0, nil
	}
	prereqs := usablePrerequisites(ctx, plan.repo, plan.commit, prep.KnownCommits)
	r, finish, err := openBundle(ctx, plan.repo, plan.commit, prereqs)
	if err != nil {
		return 0, err
	}
	n, err := upload(ctx, ex, run.JobId, r, finish)
	if err != nil {
		return 0, err
	}
	run.BundleUploaded = true
	return n, nil
}

func shipFile(ctx context.Context, ex nodeExec, jobID, path string) (uint64, error) {
	f, err := os.Open(path)
	if err != nil {
		return 0, err
	}
	defer f.Close()
	return upload(ctx, ex, jobID, f, nil)
}

// shipDir uploads dir as a zip built on the fly.
func shipDir(ctx context.Context, ex nodeExec, jobID, dir string) (uint64, error) {
	pr, pw := io.Pipe()
	done := make(chan error, 1)
	go func() {
		err := writeZip(pw, dir)
		pw.CloseWithError(err)
		done <- err
	}()
	n, err := upload(ctx, ex, jobID, pr, func() error { return <-done })
	pr.CloseWithError(err)
	return n, err
}

// writeZip archives the regular files and directories below dir. Symlinks
// are skipped.
func writeZip(w io.Writer, dir string) error {
	zw := zip.NewWriter(w)
	err := filepath.WalkDir(dir, func(path string, d fs.DirEntry, err error) error {
		if err != nil || path == dir {
			return err
		}
		rel, err := filepath.Rel(dir, path)
		if err != nil {
			return err
		}
		fi, err := d.Info()
		if err != nil {
			return err
		}
		if !fi.IsDir() && !fi.Mode().IsRegular() {
			return nil
		}
		hdr, err := zip.FileInfoHeader(fi)
		if err != nil {
			return err
		}
		hdr.Name = filepath.ToSlash(rel)
		if fi.IsDir() {
			hdr.Name += "/"
		} else {
			hdr.Method = zip.Deflate
		}
		fw, err := zw.CreateHeader(hdr)
		if err != nil || fi.IsDir() {
			return err
		}
		f, err := os.Open(path)
		if err != nil {
			return err
		}
		defer f.Close()
		_, err = io.Copy(fw, f)
		return err
	})
	if err != nil {
		return err
	}
	return zw.Close()
}

// upload sends r in ChunkSize frames. before runs once r is drained, before
// the last frame goes out, so a failed producer never yields a "complete"
// upload.
func upload(ctx context.Context, ex nodeExec, jobID string, r io.Reader, before func() error) (uint64, error) {
	h := sha256.New()
	buf := make([]byte, node.ChunkSize)
	var off uint64
	for {
		n, err := io.ReadFull(r, buf)
		last := errors.Is(err, io.EOF) || errors.Is(err, io.ErrUnexpectedEOF)
		if err != nil && !last {
			return off, err
		}
		if last && before != nil {
			if err := before(); err != nil {
				return off, err
			}
		}
		h.Write(buf[:n])
		req := &pb.UploadJobDataRequest{JobId: jobID, Offset: off, Data: buf[:n], Last: last}
		if last {
			req.Sha256 = hex.EncodeToString(h.Sum(nil))
		}
		if _, err := ex.Upload(ctx, req); err != nil {
			return off, fmt.Errorf("upload at %d: %w", off, err)
		}
		off += uint64(n)
		if last {
			return off, nil
		}
	}
}

func (m *JobManager) stream(ctx context.Context, e *jobEntry, ex nodeExec, run *pb.RunJobRequest) error {
	e.mu.Lock()
	if e.cancelRequested {
		e.mu.Unlock()
		return errors.New("canceled")
	}
	e.streaming = true
	e.mu.Unlock()

	art := &artifactSink{dir: filepath.Join(e.dir, "artifacts")}
	defer art.close()
	var fin *pb.JobFinished
	err := ex.Run(ctx, run, func(ev *pb.JobEvent) error {
		switch v := ev.Event.(type) {
		case *pb.JobEvent_Started:
			m.update(e, func(j *pb.Job) {
				j.State, j.StartedAt, j.WorkspaceReused = pb.JobState_JOB_STATE_RUNNING, timestamppb.Now(), v.Started.WorkspaceReused
				j.Isolation = v.Started.Isolation
			})
		case *pb.JobEvent_Output:
			m.appendLog(e, v.Output.Data)
		case *pb.JobEvent_Artifact:
			return art.write(v.Artifact)
		case *pb.JobEvent_Finished:
			fin = v.Finished
		}
		return nil
	})
	if err := art.close(); err != nil && fin != nil && fin.Error == "" {
		fin.Error = "store artifacts: " + err.Error()
	}
	if fin == nil {
		if err == nil {
			err = errors.New("node ended the stream without a result")
		}
		return fmt.Errorf("node connection lost: %w", err)
	}
	m.update(e, func(j *pb.Job) {
		j.ExitCode, j.Error, j.RunTime, j.FinishedAt = fin.ExitCode, fin.Error, fin.RunTime, timestamppb.Now()
		j.Artifacts, j.ArtifactBytes = art.files, art.total
		switch fin.End {
		case pb.JobEnd_JOB_END_EXITED:
			j.State = pb.JobState_JOB_STATE_SUCCEEDED
			if fin.ExitCode != 0 {
				j.State = pb.JobState_JOB_STATE_FAILED
				if j.Error == "" {
					j.Error = fmt.Sprintf("exit status %d", fin.ExitCode)
				}
			}
		case pb.JobEnd_JOB_END_TIMED_OUT:
			j.State = pb.JobState_JOB_STATE_TIMED_OUT
		case pb.JobEnd_JOB_END_CANCELED:
			j.State = pb.JobState_JOB_STATE_CANCELED
		default:
			j.State = pb.JobState_JOB_STATE_FAILED
		}
	})
	return nil
}

// artifactSink writes artifact frames below dir.
type artifactSink struct {
	dir   string
	f     *os.File
	cur   *pb.JobArtifact
	files []*pb.JobArtifact
	total uint64
}

func (a *artifactSink) write(c *pb.JobArtifactChunk) error {
	if a.cur != nil && a.cur.Path != c.Path {
		return errors.New("artifact frames interleaved")
	}
	if a.cur == nil {
		rel := filepath.FromSlash(c.Path)
		if !filepath.IsLocal(rel) {
			return fmt.Errorf("unsafe artifact path %q", c.Path)
		}
		dst := filepath.Join(a.dir, rel)
		if err := os.MkdirAll(filepath.Dir(dst), 0o700); err != nil {
			return err
		}
		f, err := os.OpenFile(dst, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o600)
		if err != nil {
			return err
		}
		a.f, a.cur = f, &pb.JobArtifact{Path: c.Path, Mode: c.Mode}
	}
	if _, err := a.f.Write(c.Data); err != nil {
		return err
	}
	a.cur.SizeBytes += uint64(len(c.Data))
	a.total += uint64(len(c.Data))
	if c.Last {
		err := a.f.Close()
		if err == nil && a.cur.Mode != 0 {
			err = os.Chmod(filepath.Join(a.dir, filepath.FromSlash(a.cur.Path)), os.FileMode(a.cur.Mode).Perm()|0o600)
		}
		a.files, a.f, a.cur = append(a.files, a.cur), nil, nil
		return err
	}
	return nil
}

func (a *artifactSink) close() error {
	if a.f == nil {
		return nil
	}
	err := a.f.Close()
	a.f, a.cur = nil, nil
	return err
}

// Cancel stops a job and waits briefly for its final state.
func (m *JobManager) Cancel(ctx context.Context, id string) (*pb.Job, error) {
	e, err := m.get(id)
	if err != nil {
		return nil, err
	}
	e.mu.Lock()
	if terminal(e.job.State) {
		e.mu.Unlock()
		j, _ := e.snapshot()
		return j, nil
	}
	e.cancelRequested = true
	streaming := e.streaming
	e.mu.Unlock()

	n, err := m.nodes.info(e.node)
	if err == nil {
		ex := m.execFor(n)
		go func() {
			cctx, cancel := context.WithTimeout(m.ctx, rpcTimeout)
			defer cancel()
			if err := ex.Cancel(cctx, id); err != nil {
				log.Printf("jobs: cancel %s on %s: %v", id, e.node, err)
			}
		}()
	}
	if streaming {
		time.AfterFunc(cancelBackstop, e.cancel)
	} else {
		e.cancel()
	}

	deadline := time.After(cancelWait)
	for {
		j, wake := e.snapshot()
		if terminal(j.State) {
			return j, nil
		}
		select {
		case <-wake:
		case <-deadline:
			return j, nil
		case <-ctx.Done():
			return j, nil
		}
	}
}

// List returns jobs newest first.
func (m *JobManager) List(state pb.JobState, limit int) []*pb.Job {
	if limit <= 0 {
		limit = 50
	}
	limit = min(limit, 500)
	m.mu.Lock()
	entries := make([]*jobEntry, 0, len(m.jobs))
	for _, e := range m.jobs {
		entries = append(entries, e)
	}
	m.mu.Unlock()
	jobs := make([]*pb.Job, 0, len(entries))
	for _, e := range entries {
		j, _ := e.snapshot()
		if state == pb.JobState_JOB_STATE_UNSPECIFIED || j.State == state {
			jobs = append(jobs, j)
		}
	}
	sort.Slice(jobs, func(a, b int) bool { return jobs[a].Id > jobs[b].Id })
	if len(jobs) > limit {
		jobs = jobs[:limit]
	}
	return jobs
}

// Watch replays the log from an offset and, with follow, tracks the job until
// it ends. The job record is sent first and on every state change.
func (m *JobManager) Watch(ctx context.Context, id string, from uint64, follow bool, send func(*pb.WatchJobResponse) error) error {
	e, err := m.get(id)
	if err != nil {
		return err
	}
	f, err := os.Open(filepath.Join(e.dir, "job.log"))
	if err != nil {
		return connect.NewError(connect.CodeInternal, err)
	}
	defer f.Close()
	off := from
	last := pb.JobState_JOB_STATE_UNSPECIFIED
	buf := make([]byte, logReadChunk)
	for {
		j, wake := e.snapshot()
		if j.State != last {
			last = j.State
			if err := send(&pb.WatchJobResponse{Event: &pb.WatchJobResponse_Job{Job: j}}); err != nil {
				return err
			}
		}
		for off < j.LogBytes {
			n, err := f.ReadAt(buf[:min(uint64(len(buf)), j.LogBytes-off)], int64(off))
			if n > 0 {
				chunk := &pb.JobLog{Offset: off, Data: append([]byte(nil), buf[:n]...)}
				if err := send(&pb.WatchJobResponse{Event: &pb.WatchJobResponse_Log{Log: chunk}}); err != nil {
					return err
				}
				off += uint64(n)
			}
			if err != nil && n == 0 {
				return connect.NewError(connect.CodeInternal, err)
			}
		}
		if !follow || terminal(j.State) {
			return nil
		}
		select {
		case <-wake:
		case <-ctx.Done():
			return nil
		}
	}
}

// Shutdown cancels every active job and waits for the records to settle.
func (m *JobManager) Shutdown() {
	m.cancel()
	m.wg.Wait()
}

func (h *Hub) SubmitJob(ctx context.Context, req *connect.Request[pb.SubmitJobRequest]) (*connect.Response[pb.Job], error) {
	j, err := h.jobs.Submit(ctx, req.Msg)
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(j), nil
}

func (h *Hub) GetJob(_ context.Context, req *connect.Request[pb.GetJobRequest]) (*connect.Response[pb.Job], error) {
	e, err := h.jobs.get(req.Msg.Id)
	if err != nil {
		return nil, err
	}
	j, _ := e.snapshot()
	return connect.NewResponse(j), nil
}

func (h *Hub) ListJobs(_ context.Context, req *connect.Request[pb.ListJobsRequest]) (*connect.Response[pb.ListJobsResponse], error) {
	return connect.NewResponse(&pb.ListJobsResponse{Jobs: h.jobs.List(req.Msg.State, int(req.Msg.Limit))}), nil
}

func (h *Hub) CancelJob(ctx context.Context, req *connect.Request[pb.CancelJobRequest]) (*connect.Response[pb.Job], error) {
	var j *pb.Job
	var err error
	switch id, key := req.Msg.Id, req.Msg.RequestId; {
	case (id == "") == (key == ""):
		return nil, invalid("set exactly one of id and request_id")
	case key != "":
		j, err = h.jobs.keys.cancel(ctx, key, func(id string) (*pb.Job, error) { return h.jobs.Cancel(ctx, id) })
	default:
		j, err = h.jobs.Cancel(ctx, id)
	}
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(j), nil
}

func (h *Hub) WatchJob(ctx context.Context, req *connect.Request[pb.WatchJobRequest], stream *connect.ServerStream[pb.WatchJobResponse]) error {
	return h.jobs.Watch(ctx, req.Msg.Id, req.Msg.FromOffset, req.Msg.Follow, stream.Send)
}
