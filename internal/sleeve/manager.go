package sleeve

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"sync"
	"time"

	"gopkg.in/yaml.v3"

	"github.com/hotschmoe/protectorate/internal/session"
)

type State string

const (
	Running State = "running"
	Exited  State = "exited"
)

type Usage struct {
	CPUPercent  float64
	CPUSeconds  float64
	MemCurrent  uint64
	MemPeak     uint64
	Pids        uint64
	OOMKills    uint64
	DiskBytes   uint64
	Sampled     time.Time
	DiskSampled time.Time
}

// Info is one sleeve. Everything but Usage is persisted in sleeves.yaml.
type Info struct {
	Name      string    `yaml:"name"`
	Harness   Harness   `yaml:"harness"`
	Workspace string    `yaml:"workspace"`
	Session   string    `yaml:"session"`
	Created   time.Time `yaml:"created"`
	State     State     `yaml:"state"`
	CPU       uint32    `yaml:"cpu_percent,omitempty"`
	Memory    uint64    `yaml:"memory_bytes,omitempty"`
	Args      []string  `yaml:"args,omitempty"`
	Exited    time.Time `yaml:"exited,omitempty"`
	Usage     Usage     `yaml:"-"`
}

func (i Info) Limits() Limits { return Limits{CPUPercent: i.CPU, MemoryBytes: i.Memory} }

type SpawnRequest struct {
	Name      string
	Workspace string // absolute directory or git URL
	Harness   Harness
	Args      []string
	Limits    Limits
	Env       map[string]string
}

type Options struct {
	Config        Config
	Sessions      *session.Manager
	Emit          func(kind, subject string, payload map[string]any)
	UsageInterval time.Duration // default 3s
	DiskInterval  time.Duration // default 5m
	StartTimeout  time.Duration // default 15s
}

type entry struct {
	Info
	cpuPrev uint64
	cpuAt   time.Time
	oomSeen uint64
	peak    uint64
}

type Manager struct {
	opts Options
	cfg  Config
	path string

	mu       sync.Mutex
	sleeves  map[string]*entry
	pending  map[string]bool
	diskBusy bool
	runCtx   context.Context
}

type fileFormat struct {
	Sleeves []Info `yaml:"sleeves"`
}

// Open loads $home/sleeves.yaml and reconciles it with the live tmux
// sessions and scopes.
func Open(ctx context.Context, opts Options) (*Manager, error) {
	if opts.UsageInterval <= 0 {
		opts.UsageInterval = 3 * time.Second
	}
	if opts.DiskInterval <= 0 {
		opts.DiskInterval = 5 * time.Minute
	}
	if opts.StartTimeout <= 0 {
		opts.StartTimeout = 15 * time.Second
	}
	if opts.Emit == nil {
		opts.Emit = func(string, string, map[string]any) {}
	}
	m := &Manager{
		opts: opts, cfg: opts.Config, path: filepath.Join(opts.Config.Home, "sleeves.yaml"),
		sleeves: map[string]*entry{}, pending: map[string]bool{}, runCtx: context.Background(),
	}
	raw, err := os.ReadFile(m.path)
	switch {
	case os.IsNotExist(err):
	case err != nil:
		return nil, fmt.Errorf("read sleeves.yaml: %w", err)
	default:
		var f fileFormat
		if err := yaml.Unmarshal(raw, &f); err != nil {
			return nil, fmt.Errorf("parse %s: %w", m.path, err)
		}
		for _, s := range f.Sleeves {
			m.sleeves[s.Name] = &entry{Info: s}
		}
	}
	if errors.Is(m.cfg.Preflight(), ErrUnsupported) {
		return m, nil
	}
	if err := m.Reconcile(ctx); err != nil {
		return nil, err
	}
	for _, n := range m.cfg.scopeNames() {
		if m.sleeves[n] == nil {
			log.Printf("sleeve scope %s has no record in %s; leaving it running", UnitName(n), m.path)
		}
	}
	m.mu.Lock()
	for _, e := range m.sleeves {
		if e.State == Running {
			m.sampleLocked(e, time.Now())
		}
	}
	m.mu.Unlock()
	return m, nil
}

func (m *Manager) saveLocked() error {
	list := make([]Info, 0, len(m.sleeves))
	for _, e := range m.sleeves {
		list = append(list, e.Info)
	}
	sort.Slice(list, func(a, b int) bool { return list[a].Name < list[b].Name })
	raw, err := yaml.Marshal(fileFormat{Sleeves: list})
	if err != nil {
		return fmt.Errorf("encode sleeves: %w", err)
	}
	if err := os.MkdirAll(m.cfg.Home, 0o700); err != nil {
		return fmt.Errorf("create %s: %w", m.cfg.Home, err)
	}
	return writeFileAtomic(m.path, raw, 0o600)
}

func writeFileAtomic(path string, data []byte, perm os.FileMode) (err error) {
	f, err := os.CreateTemp(filepath.Dir(path), "."+filepath.Base(path)+".*")
	if err != nil {
		return err
	}
	tmp := f.Name()
	defer func() {
		if err != nil {
			f.Close()
			os.Remove(tmp)
		}
	}()
	if err = f.Chmod(perm); err != nil {
		return err
	}
	if _, err = f.Write(data); err != nil {
		return err
	}
	if err = f.Sync(); err != nil {
		return err
	}
	if err = f.Close(); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}

func (m *Manager) List() []Info {
	m.mu.Lock()
	defer m.mu.Unlock()
	out := make([]Info, 0, len(m.sleeves))
	for _, e := range m.sleeves {
		out = append(out, e.Info)
	}
	sort.Slice(out, func(a, b int) bool { return out[a].Name < out[b].Name })
	return out
}

func (m *Manager) Get(name string) (Info, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	e := m.sleeves[name]
	if e == nil {
		return Info{}, ErrNotFound
	}
	return e.Info, nil
}

// reserve claims name for an operation in progress.
func (m *Manager) reserve(name string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.pending[name] {
		return fmt.Errorf("%w: %s is being changed", ErrExists, name)
	}
	m.pending[name] = true
	return nil
}

func (m *Manager) release(name string) {
	m.mu.Lock()
	delete(m.pending, name)
	m.mu.Unlock()
}

// Spawn starts a sleeve and returns once its scope is running.
func (m *Manager) Spawn(ctx context.Context, req SpawnRequest) (Info, error) {
	start := time.Now()
	if err := m.cfg.Preflight(); err != nil {
		return Info{}, err
	}
	if err := ValidName(req.Name); err != nil {
		return Info{}, err
	}
	if req.Harness == "" {
		return Info{}, fmt.Errorf("%w: harness is required", ErrInvalid)
	}
	if _, err := ParseHarness(string(req.Harness)); err != nil {
		return Info{}, err
	}
	if err := req.Limits.validate(); err != nil {
		return Info{}, err
	}
	if err := m.reserve(req.Name); err != nil {
		return Info{}, err
	}
	defer m.release(req.Name)

	m.mu.Lock()
	prev := m.sleeves[req.Name]
	m.mu.Unlock()
	if prev != nil && prev.State == Running {
		return Info{}, fmt.Errorf("%w: %s", ErrExists, req.Name)
	}
	if m.cfg.scopeAlive(req.Name) {
		return Info{}, fmt.Errorf("%w: scope %s is still running", ErrExists, UnitName(req.Name))
	}

	ws, err := m.resolveWorkspace(ctx, req.Name, req.Workspace)
	if err != nil {
		return Info{}, err
	}
	bin, err := m.cfg.Locate(req.Harness)
	if err != nil {
		return Info{}, err
	}
	spec := Spec{Name: req.Name, Harness: req.Harness, Args: req.Args, Workspace: ws, Env: req.Env, Limits: req.Limits}
	launch, err := m.cfg.Plan(spec, bin)
	if err != nil {
		return Info{}, err
	}
	if err := launch.Layout.Apply(); err != nil {
		return Info{}, fmt.Errorf("prepare sleeve layout: %w", err)
	}

	info := Info{
		Name: req.Name, Harness: req.Harness, Workspace: ws, Session: SessionName(req.Name),
		Created: time.Now().UTC().Truncate(time.Second), State: Running,
		CPU: req.Limits.CPUPercent, Memory: req.Limits.MemoryBytes, Args: req.Args,
	}
	e := &entry{Info: info}
	m.mu.Lock()
	m.sleeves[req.Name] = e
	err = m.saveLocked()
	if err != nil {
		if prev != nil {
			m.sleeves[req.Name] = prev
		} else {
			delete(m.sleeves, req.Name)
		}
	}
	m.mu.Unlock()
	if err != nil {
		return Info{}, fmt.Errorf("persist sleeve: %w", err)
	}

	if err := m.start(ctx, launch, info); err != nil {
		m.mu.Lock()
		if prev != nil {
			m.sleeves[req.Name] = prev
		} else {
			delete(m.sleeves, req.Name)
		}
		if serr := m.saveLocked(); serr != nil {
			log.Printf("sleeve %s: persist after failed start: %v", req.Name, serr)
		}
		m.mu.Unlock()
		return Info{}, err
	}

	m.mu.Lock()
	m.sampleLocked(e, time.Now())
	out := e.Info
	m.mu.Unlock()
	m.opts.Emit("sleeve.spawned", req.Name, map[string]any{
		"harness": string(req.Harness), "workspace": ws, "session": info.Session,
		"spawn_ms": float64(time.Since(start).Milliseconds()),
	})
	return out, nil
}

func (m *Manager) start(ctx context.Context, launch Launch, info Info) error {
	env := map[string]string{}
	for _, k := range []string{"XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS"} {
		if v := os.Getenv(k); v != "" {
			env[k] = v
		}
	}
	_, err := m.opts.Sessions.Create(ctx, session.CreateOptions{
		Name: info.Session, Cwd: info.Workspace, Command: launch.Argv, Env: env,
	})
	if err != nil {
		return fmt.Errorf("start sleeve session: %w", err)
	}
	fail := func(cause error) error {
		if kerr := m.opts.Sessions.Kill(context.WithoutCancel(ctx), info.Session); kerr != nil && !errors.Is(kerr, session.ErrNotFound) {
			log.Printf("sleeve %s: kill session: %v", info.Name, kerr)
		}
		if kerr := m.cfg.killScope(context.WithoutCancel(ctx), info.Name); kerr != nil {
			log.Printf("sleeve %s: kill scope: %v", info.Name, kerr)
		}
		return cause
	}
	deadline := time.Now().Add(m.opts.StartTimeout)
	for n := 0; ; n++ {
		if m.cfg.scopeRunning(info.Name) {
			return nil
		}
		if err := ctx.Err(); err != nil {
			return fail(err)
		}
		if time.Now().After(deadline) {
			return fail(fmt.Errorf("%w: scope %s did not appear within %s", ErrUnavailable, launch.Unit, m.opts.StartTimeout))
		}
		if n%25 == 24 {
			if _, err := m.opts.Sessions.Get(ctx, info.Session); errors.Is(err, session.ErrNotFound) {
				return fail(fmt.Errorf("%w: sleeve exited during startup (run the printed command by hand to see why): %s",
					ErrUnavailable, strings.Join(launch.Argv, " ")))
			}
		}
		time.Sleep(10 * time.Millisecond)
	}
}

// Kill stops the sleeve's processes and forgets it. Its workspace and home
// directory stay on disk.
func (m *Manager) Kill(ctx context.Context, name string) error {
	if err := m.reserve(name); err != nil {
		return err
	}
	defer m.release(name)
	m.mu.Lock()
	e := m.sleeves[name]
	m.mu.Unlock()
	if e == nil {
		return ErrNotFound
	}
	if e.State == Running {
		if err := m.opts.Sessions.Kill(ctx, e.Session); err != nil && !errors.Is(err, session.ErrNotFound) {
			return fmt.Errorf("kill session: %w", err)
		}
		if err := m.cfg.killScope(ctx, name); err != nil {
			return err
		}
	}
	m.mu.Lock()
	wasRunning := e.State == Running
	delete(m.sleeves, name)
	err := m.saveLocked()
	m.mu.Unlock()
	if wasRunning {
		m.opts.Emit("sleeve.exited", name, map[string]any{"reason": "killed"})
	}
	if err != nil {
		return fmt.Errorf("persist sleeves: %w", err)
	}
	return nil
}

// Reconcile marks running sleeves whose tmux session or scope has gone as
// exited and cleans up whichever half is left.
func (m *Manager) Reconcile(ctx context.Context) error {
	live, err := m.opts.Sessions.List(ctx)
	if err != nil {
		return fmt.Errorf("list sessions: %w", err)
	}
	have := make(map[string]bool, len(live))
	for _, s := range live {
		have[s.Name] = true
	}
	m.mu.Lock()
	var gone []*entry
	for name, e := range m.sleeves {
		if e.State == Running && !m.pending[name] && (!have[e.Session] || !m.cfg.scopeAlive(name)) {
			gone = append(gone, e)
		}
	}
	m.mu.Unlock()
	for _, e := range gone {
		if err := m.markExited(ctx, e, have[e.Session]); err != nil {
			log.Printf("sleeve %s: %v", e.Name, err)
		}
	}
	return nil
}

func (m *Manager) markExited(ctx context.Context, e *entry, sessionLive bool) error {
	if err := m.reserve(e.Name); err != nil {
		return nil
	}
	defer m.release(e.Name)
	ctx = context.WithoutCancel(ctx)
	if sessionLive {
		if err := m.opts.Sessions.Kill(ctx, e.Session); err != nil && !errors.Is(err, session.ErrNotFound) {
			log.Printf("sleeve %s: kill session: %v", e.Name, err)
		}
	}
	if err := m.cfg.killScope(ctx, e.Name); err != nil {
		return err
	}
	m.mu.Lock()
	if m.sleeves[e.Name] != e || e.State != Running {
		m.mu.Unlock()
		return nil
	}
	e.State, e.Exited = Exited, time.Now().UTC().Truncate(time.Second)
	e.Usage.CPUPercent, e.Usage.Pids = 0, 0
	err := m.saveLocked()
	m.mu.Unlock()
	m.opts.Emit("sleeve.exited", e.Name, map[string]any{"reason": "exited"})
	return err
}

// Run reconciles and samples usage until ctx is done.
func (m *Manager) Run(ctx context.Context) {
	m.mu.Lock()
	m.runCtx = ctx
	m.mu.Unlock()
	tick := time.NewTicker(m.opts.UsageInterval)
	defer tick.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-tick.C:
		}
		if err := m.Reconcile(ctx); err != nil && ctx.Err() == nil {
			log.Printf("sleeves: %v", err)
		}
		m.sample()
	}
}

func (m *Manager) sample() {
	now := time.Now()
	m.mu.Lock()
	defer m.mu.Unlock()
	var due []*entry
	for _, e := range m.sleeves {
		if e.State != Running {
			continue
		}
		m.sampleLocked(e, now)
		if now.Sub(e.Usage.DiskSampled) >= m.opts.DiskInterval {
			due = append(due, e)
		}
	}
	if len(due) > 0 && !m.diskBusy {
		m.diskBusy = true
		go m.measureDisk(m.runCtx, due)
	}
}

func (m *Manager) sampleLocked(e *entry, now time.Time) {
	raw, err := m.cfg.readUsage(e.Name)
	if err != nil {
		return
	}
	u := &e.Usage
	if !e.cpuAt.IsZero() && now.After(e.cpuAt) && raw.cpuUsec >= e.cpuPrev {
		u.CPUPercent = float64(raw.cpuUsec-e.cpuPrev) / float64(now.Sub(e.cpuAt).Microseconds()) * 100
	}
	e.cpuPrev, e.cpuAt = raw.cpuUsec, now
	u.CPUSeconds = float64(raw.cpuUsec) / 1e6
	u.MemCurrent, u.Pids, u.Sampled = raw.memCurrent, raw.pids, now
	if raw.memCurrent > e.peak {
		e.peak = raw.memCurrent
	}
	u.MemPeak = e.peak
	if raw.peakKnown && raw.memPeak > u.MemPeak {
		u.MemPeak = raw.memPeak
	}
	if raw.oomKills > e.oomSeen {
		m.opts.Emit("sleeve.oom", e.Name, map[string]any{
			"oom_kills": float64(raw.oomKills), "new_kills": float64(raw.oomKills - e.oomSeen),
			"memory_max_bytes": float64(e.Memory), "memory_peak_bytes": float64(u.MemPeak),
		})
		e.oomSeen = raw.oomKills
	}
	u.OOMKills = e.oomSeen
}

func (m *Manager) measureDisk(ctx context.Context, list []*entry) {
	defer func() {
		m.mu.Lock()
		m.diskBusy = false
		m.mu.Unlock()
	}()
	for _, e := range list {
		m.mu.Lock()
		ws := e.Workspace
		m.mu.Unlock()
		n, err := dirSize(ctx, ws)
		if ctx.Err() != nil {
			return
		}
		m.mu.Lock()
		if err == nil {
			e.Usage.DiskBytes, e.Usage.DiskSampled = n, time.Now()
		}
		m.mu.Unlock()
	}
}

var scpURL = regexp.MustCompile(`^[A-Za-z0-9._-]+@[A-Za-z0-9._-]+:`)

func isGitURL(s string) bool { return strings.Contains(s, "://") || scpURL.MatchString(s) }

// resolveWorkspace returns the absolute directory to mount, cloning a git
// URL into $home/workspaces/<name> first.
func (m *Manager) resolveWorkspace(ctx context.Context, name, ws string) (string, error) {
	if ws == "" {
		return "", fmt.Errorf("%w: workspace is required", ErrInvalid)
	}
	if !isGitURL(ws) {
		if !filepath.IsAbs(ws) {
			return "", fmt.Errorf("%w: workspace %q must be an absolute path or a git URL", ErrInvalid, ws)
		}
		real, err := filepath.EvalSymlinks(ws)
		if err != nil {
			return "", fmt.Errorf("%w: workspace: %v", ErrInvalid, err)
		}
		if fi, err := os.Stat(real); err != nil || !fi.IsDir() {
			return "", fmt.Errorf("%w: workspace %s is not a directory", ErrInvalid, real)
		}
		if err := m.cfg.CheckWorkspace(real); err != nil {
			return "", err
		}
		return real, nil
	}
	if strings.HasPrefix(ws, "-") {
		return "", fmt.Errorf("%w: invalid git URL", ErrInvalid)
	}
	dest := filepath.Join(m.cfg.workspacesDir(), name)
	if _, err := os.Stat(filepath.Join(dest, ".git")); err == nil {
		return dest, nil
	}
	if ents, err := os.ReadDir(dest); err == nil && len(ents) > 0 {
		return "", fmt.Errorf("%w: %s exists and is not a git checkout", ErrExists, dest)
	}
	if err := os.MkdirAll(filepath.Dir(dest), 0o700); err != nil {
		return "", fmt.Errorf("create workspaces dir: %w", err)
	}
	cctx, cancel := context.WithTimeout(ctx, 10*time.Minute)
	defer cancel()
	cmd := exec.CommandContext(cctx, "git", "clone", "--quiet", "--", ws, dest)
	cmd.Env = append(os.Environ(), "GIT_TERMINAL_PROMPT=0", "GIT_ALLOW_PROTOCOL=https:http:ssh:git:file")
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		return "", fmt.Errorf("%w: git clone %s: %s", ErrInvalid, ws, strings.TrimSpace(stderr.String()))
	}
	return dest, nil
}
