package sleeve

import (
	"bytes"
	"context"
	"encoding/json"
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

	"github.com/hotschmoe/protectorate/internal/driver"
	"github.com/hotschmoe/protectorate/internal/fsutil"
	"github.com/hotschmoe/protectorate/internal/session"
)

type State string

const (
	Running State = "running"
	Exited  State = "exited"
)

type Usage struct {
	CPUPercent        float64
	CPUSeconds        float64
	MemCurrent        uint64
	MemPeak           uint64
	Pids              uint64
	OOMKills          uint64
	DiskBytes         uint64
	HomeBytes         uint64
	CacheBytes        uint64
	GoBuildCacheBytes uint64
	Sampled           time.Time
	DiskSampled       time.Time
}

// Info is one sleeve. Everything but Usage is persisted in sleeves.json.
type Info struct {
	BaseRepo   string    `json:"base_repo,omitempty"`
	BaseBranch string    `json:"base_branch,omitempty"`
	BaseTip    string    `json:"base_tip,omitempty"`
	Name       string    `json:"name"`
	Harness    Harness   `json:"harness"`
	Workspace  string    `json:"workspace"`
	Session    string    `json:"session"`
	Created    time.Time `json:"created"`
	State      State     `json:"state"`
	// Parent is the identity that spawned the sleeve; ParentID its token id.
	Parent        string        `json:"parent,omitempty"`
	ParentID      string        `json:"parent_id,omitempty"`
	ReadOnly      []Mount       `json:"read_only,omitempty"`
	ExitAfterIdle time.Duration `json:"exit_after_idle,omitempty"`
	Nested        bool          `json:"nested,omitempty"`
	// Mode is how the harness runs. A record without one is a terminal
	// sleeve from before modes existed.
	Mode Mode `json:"mode,omitempty"`
	// Chat is set for sleeves in chat mode.
	Chat   *Chat   `json:"chat,omitempty"`
	Result *Result `json:"result,omitempty"`
	// Supervised chat sleeves run the harness in its asking permission mode.
	Supervised bool `json:"supervised,omitempty"`
	// Handoff is set when a resleeve produced this sleeve's harness.
	Handoff *Handoff `json:"handoff,omitempty"`
	// Helper names the hub-internal job of a short-lived sleeve (such as a
	// handoff summary); the directory and the mailbox leave such sleeves out.
	Helper string    `json:"helper,omitempty"`
	CPU    uint32    `json:"cpu_percent,omitempty"`
	Memory uint64    `json:"memory_bytes,omitempty"`
	Args   []string  `json:"args,omitempty"`
	Exited time.Time `json:"exited,omitempty"`
	// ExitCode is nil when the exit status is unknown (the session was
	// killed outside the hub). It is 128+ExitSignal when a signal ended the
	// harness.
	ExitCode   *int `json:"exit_code,omitempty"`
	ExitSignal int  `json:"exit_signal,omitempty"`
	// OOMKilled is set when the harness exited with status 137 after the
	// kernel OOM killer fired in its cgroup.
	OOMKilled bool  `json:"oom_killed,omitempty"`
	Killed    bool  `json:"killed,omitempty"`
	Usage     Usage `json:"-"`
}

func (i Info) Limits() Limits { return Limits{CPUPercent: i.CPU, MemoryBytes: i.Memory} }

type SpawnRequest struct {
	Base          *Info
	Branch        string
	NewBranch     string
	Brief         string
	Parent        string
	ParentID      string
	ReadOnly      []Mount
	ExitAfterIdle time.Duration
	Nested        bool

	Selection      driver.Selection
	EffortFallback bool
	Name           string
	Workspace      string // absolute directory or git URL
	Harness        Harness
	Args           []string
	Limits         Limits
	Env            map[string]string
	// Mode defaults to chat for agent harnesses and terminal for shell.
	Mode Mode
	// Supervised applies to chat mode only.
	Supervised bool
	Helper     string
}

type Options struct {
	Config        Config
	Sessions      *session.Manager
	Emit          func(kind, subject string, payload map[string]any)
	UsageInterval time.Duration // default 3s
	OOMInterval   time.Duration // how often limited sleeves' OOM counters are read; default 50ms
	DiskInterval  time.Duration // default 5m
	StartTimeout  time.Duration // default 15s
}

type entry struct {
	Info
	cpuPrev uint64
	cpuAt   time.Time
	oomSeen uint64
	peak    uint64
	// scopeGone is when Reconcile first saw the scope missing under a session
	// that tmux still reports as running.
	scopeGone time.Time
}

// scopeGrace is how long tmux gets to record the exit status of a command
// whose scope has already vanished. The scope goes the moment the last process
// exits, slightly before tmux reaps the pane.
const scopeGrace = 5 * time.Second

type Manager struct {
	opts Options
	cfg  Config
	path string

	mu       sync.Mutex
	sleeves  map[string]*entry
	pending  map[string]chan struct{}
	diskBusy bool
	runCtx   context.Context
}

func (m *Manager) Runtime() Config { return m.cfg }

type fileFormat struct {
	Sleeves []Info `json:"sleeves"`
}

// Open loads $home/sleeves.json and reconciles it with the live tmux
// sessions and scopes.
func Open(ctx context.Context, opts Options) (*Manager, error) {
	if opts.UsageInterval <= 0 {
		opts.UsageInterval = 3 * time.Second
	}
	if opts.OOMInterval <= 0 {
		opts.OOMInterval = 50 * time.Millisecond
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
		opts: opts, cfg: opts.Config, path: filepath.Join(opts.Config.Home, "sleeves.json"),
		sleeves: map[string]*entry{}, pending: map[string]chan struct{}{}, runCtx: context.Background(),
	}
	m.initGoCaches()
	raw, err := os.ReadFile(m.path)
	switch {
	case os.IsNotExist(err):
	case err != nil:
		return nil, fmt.Errorf("read sleeves.json: %w", err)
	default:
		var f fileFormat
		if err := json.Unmarshal(raw, &f); err != nil {
			return nil, fmt.Errorf("parse %s: %w", m.path, err)
		}
		for _, s := range f.Sleeves {
			m.sleeves[s.Name] = &entry{Info: s}
		}
	}
	if errors.Is(m.cfg.Preflight(), ErrUnsupported) {
		m.discoverHomes()
		return m, nil
	}
	if err := m.renameRecord(ctx, previousOrchestratorName, OrchestratorName); err != nil {
		log.Printf("migrate orchestrator sleeve: %v", err)
	}
	m.discoverHomes()
	if err := m.Reconcile(ctx); err != nil {
		return nil, err
	}
	for _, n := range m.cfg.scopeNames() {
		if m.sleeves[n] == nil {
			log.Printf("sleeve scope %s has no record in %s; leaving it running", m.cfg.UnitName(n), m.path)
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
	raw, err := json.MarshalIndent(fileFormat{Sleeves: list}, "", "  ")
	if err != nil {
		return fmt.Errorf("encode sleeves: %w", err)
	}
	if err := os.MkdirAll(m.cfg.Home, 0o700); err != nil {
		return fmt.Errorf("create %s: %w", m.cfg.Home, err)
	}
	return fsutil.WriteFile(m.path, raw, 0o600)
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

// reserve claims name for an operation, waiting for one already in progress
// (such as recording an exit) until ctx ends.
func (m *Manager) reserve(ctx context.Context, name string) error {
	for {
		m.mu.Lock()
		busy, ok := m.pending[name]
		if !ok {
			m.pending[name] = make(chan struct{})
			m.mu.Unlock()
			return nil
		}
		m.mu.Unlock()
		select {
		case <-busy:
		case <-ctx.Done():
			return fmt.Errorf("%w: %s is being changed: %w", ErrExists, name, ctx.Err())
		}
	}
}

// tryReserve claims name only if no operation is in progress.
func (m *Manager) tryReserve(name string) bool {
	m.mu.Lock()
	defer m.mu.Unlock()
	if _, ok := m.pending[name]; ok {
		return false
	}
	m.pending[name] = make(chan struct{})
	return true
}

func (m *Manager) release(name string) {
	m.mu.Lock()
	if busy, ok := m.pending[name]; ok {
		close(busy)
		delete(m.pending, name)
	}
	m.mu.Unlock()
}

// Spawn starts a sleeve and returns once its scope is running.
func (m *Manager) validateSpawn(req SpawnRequest) (SpawnRequest, error) {
	if err := m.cfg.Preflight(); err != nil {
		return SpawnRequest{}, err
	}
	if err := ValidName(req.Name); err != nil {
		return SpawnRequest{}, err
	}
	if req.Harness == "" {
		return SpawnRequest{}, fmt.Errorf("%w: harness is required", ErrInvalid)
	}
	if _, err := ParseHarness(string(req.Harness)); err != nil {
		return SpawnRequest{}, err
	}
	if err := req.Limits.Validate(); err != nil {
		return SpawnRequest{}, err
	}
	if req.Nested {
		if req.Limits.CPUPercent == 0 || req.Limits.MemoryBytes == 0 {
			return SpawnRequest{}, fmt.Errorf("%w: nested sleeves require CPU and memory limits", ErrInvalid)
		}
		for _, binary := range []string{"unshare", "flock"} {
			if _, err := exec.LookPath(binary); err != nil {
				return SpawnRequest{}, fmt.Errorf("%w: nested sleeves require %s: %w", ErrUnavailable, binary, err)
			}
		}
	}
	mode, err := resolveMode(req.Harness, req.Mode, req.Args)
	if err != nil {
		return SpawnRequest{}, err
	}
	if req.Brief != "" && mode != ModeChat {
		return SpawnRequest{}, fmt.Errorf("%w: brief requires chat mode", ErrInvalid)
	}
	if req.Brief != "" && strings.TrimSpace(req.Brief) == "" {
		return SpawnRequest{}, fmt.Errorf("%w: brief is empty", ErrInvalid)
	}
	for _, branch := range []string{req.Branch, req.NewBranch} {
		if branch != "" && !validBranch(branch) {
			return SpawnRequest{}, fmt.Errorf("%w: invalid branch %q", ErrInvalid, branch)
		}
	}
	if req.ExitAfterIdle < 0 || (req.ExitAfterIdle > 0 && mode != ModeChat) {
		return SpawnRequest{}, fmt.Errorf("%w: exit-after-idle requires chat mode and a nonnegative duration", ErrInvalid)
	}
	req.ReadOnly, err = m.cfg.ResolveMounts(req.ReadOnly)
	if err != nil {
		return SpawnRequest{}, err
	}
	if req.Selection != (driver.Selection{}) && mode != ModeChat {
		return SpawnRequest{}, fmt.Errorf("%w: selection requires chat mode", ErrInvalid)
	}
	if err := driver.CheckSelection(driver.Harness(req.Harness), req.Selection); err != nil {
		return SpawnRequest{}, fmt.Errorf("%w: %w", ErrInvalid, err)
	}
	if req.Supervised && mode != ModeChat {
		return SpawnRequest{}, fmt.Errorf("%w: supervised applies to chat mode; a terminal sleeve's TUI asks in the terminal", ErrInvalid)
	}
	req.Mode = mode
	return req, nil
}

func (m *Manager) Spawn(ctx context.Context, req SpawnRequest) (Info, error) {
	start := time.Now()
	req, err := m.validateSpawn(req)
	if err != nil {
		return Info{}, err
	}
	mode := req.Mode
	if err := m.reserve(ctx, req.Name); err != nil {
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
		return Info{}, fmt.Errorf("%w: scope %s is still running", ErrExists, m.cfg.UnitName(req.Name))
	}

	var base Info
	if req.Base != nil {
		base = *req.Base
	}
	ws, err := m.resolveWorkspace(ctx, req, &base)
	if err != nil {
		return Info{}, err
	}
	if req.Base == nil && prev != nil && ws == prev.Workspace {
		m.mu.Lock()
		base.BaseRepo, base.BaseBranch, base.BaseTip = prev.BaseRepo, prev.BaseBranch, prev.BaseTip
		m.mu.Unlock()
	}
	req.Base = &base
	launch, err := m.planSpawn(req, ws)
	if err != nil {
		return Info{}, err
	}
	if err := launch.Layout.Apply(ctx); err != nil {
		return Info{}, fmt.Errorf("prepare sleeve layout: %w", err)
	}
	if err := m.cfg.writeEnvFile(req.Name, launch); err != nil {
		return Info{}, err
	}

	if prev != nil && prev.Session != "" {
		if err := m.opts.Sessions.Kill(ctx, prev.Session); err != nil && !errors.Is(err, session.ErrNotFound) {
			return Info{}, fmt.Errorf("remove exited session: %w", err)
		}
	}

	info := Info{
		BaseRepo: base.BaseRepo, BaseBranch: base.BaseBranch, BaseTip: base.BaseTip,
		Parent: req.Parent, ParentID: req.ParentID,
		ReadOnly: req.ReadOnly, ExitAfterIdle: req.ExitAfterIdle,
		Nested: req.Nested,
		Name:   req.Name, Harness: req.Harness, Workspace: ws, Mode: mode,
		Created: time.Now().UTC().Truncate(time.Second), State: Running,
		CPU: req.Limits.CPUPercent, Memory: req.Limits.MemoryBytes, Args: req.Args,
		Supervised: req.Supervised, Helper: req.Helper,
	}
	if mode == ModeTerminal {
		info.Session = SessionName(req.Name)
	}
	if mode == ModeChat {
		info.Chat = &Chat{Requested: req.Selection, EffortFallback: req.EffortFallback, Starting: true}
	}
	e := &entry{Info: info}
	if err := m.installSleeve(e, prev); err != nil {
		return Info{}, fmt.Errorf("persist sleeve: %w", err)
	}

	if mode != ModeChat {
		if err := m.start(ctx, launch, info); err != nil {
			m.rollbackSpawn(req.Name, prev)
			return Info{}, err
		}
	}

	m.mu.Lock()
	if mode != ModeChat {
		m.sampleLocked(e, time.Now())
	}
	out := e.Info
	m.mu.Unlock()
	fields := map[string]any{
		"harness": string(req.Harness), "workspace": ws, "mode": string(mode),
		"spawn_ms": float64(time.Since(start).Milliseconds()),
	}
	if mode != ModeChat {
		fields["session"] = info.Session
	}
	if mode != ModeChat {
		m.opts.Emit("sleeve.spawned", req.Name, fields)
	}
	return out, nil
}

func (m *Manager) restoreSleeveLocked(name string, prev *entry) {
	if prev != nil {
		m.sleeves[name] = prev
	} else {
		delete(m.sleeves, name)
	}
}

func (m *Manager) installSleeve(e, prev *entry) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.sleeves[e.Name] = e
	if err := m.saveLocked(); err != nil {
		m.restoreSleeveLocked(e.Name, prev)
		return err
	}
	return nil
}

func (m *Manager) rollbackSpawn(name string, prev *entry) {
	m.mu.Lock()
	m.restoreSleeveLocked(name, prev)
	if err := m.saveLocked(); err != nil {
		log.Printf("sleeve %s: persist after failed start: %v", name, err)
	}
	m.mu.Unlock()
	if err := m.cfg.removeEnvFile(name); err != nil {
		log.Printf("sleeve %s: %v", name, err)
	}
}

func (m *Manager) planSpawn(req SpawnRequest, workspace string) (Launch, error) {
	bin, err := m.cfg.Locate(req.Harness)
	if err != nil {
		return Launch{}, err
	}
	args := req.Args
	if req.Mode == ModeTerminal {
		args = terminalArgs(req.Harness, args)
	}
	return m.cfg.Plan(Spec{
		GoCacheSnapshot: m.cfg.pinGoCache(req.Name),
		BaseRepo:        req.Base.BaseRepo,
		BaseTip:         req.Base.BaseTip,
		Name:            req.Name, Harness: req.Harness, Args: args, Workspace: workspace,
		Env: req.Env, Limits: req.Limits, ReadOnly: req.ReadOnly,
		Nested: req.Nested,
	}, bin)
}

// cleanupTimeout bounds teardown after a failed start, which must finish
// even when the request context is already canceled.
const cleanupTimeout = 15 * time.Second

func (m *Manager) start(ctx context.Context, launch Launch, info Info) error {
	env := map[string]string{}
	for _, k := range []string{"XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS"} {
		if v := os.Getenv(k); v != "" {
			env[k] = v
		}
	}
	fail := func(cause error) error {
		cctx, cancel := context.WithTimeout(context.WithoutCancel(ctx), cleanupTimeout)
		defer cancel()
		if kerr := m.opts.Sessions.Kill(cctx, info.Session); kerr != nil && !errors.Is(kerr, session.ErrNotFound) {
			log.Printf("sleeve %s: kill session: %v", info.Name, kerr)
		}
		if kerr := m.cfg.killScope(cctx, info.Name); kerr != nil {
			log.Printf("sleeve %s: kill scope: %v", info.Name, kerr)
		}
		return cause
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	// tmux limits each command message, including the saved respawn argv.
	var script strings.Builder
	script.WriteString("#!/bin/sh\nexec")
	for _, arg := range launch.Argv {
		script.WriteString(" '")
		script.WriteString(strings.ReplaceAll(arg, "'", "'\\''"))
		script.WriteByte('\'')
	}
	script.WriteByte('\n')
	launcher := filepath.Join(m.cfg.sleeveDir(info.Name), "launch.sh")
	if err := fsutil.WriteFile(launcher, []byte(script.String()), 0o700); err != nil {
		return fmt.Errorf("write sleeve launcher: %w", err)
	}
	if _, err := m.opts.Sessions.Create(ctx, session.CreateOptions{
		Name: info.Session, Cwd: info.Workspace, Command: []string{"/bin/sh", launcher}, Env: env,
	}); err != nil {
		return fail(fmt.Errorf("start sleeve session: %w", err))
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
			s, err := m.opts.Sessions.Get(ctx, info.Session)
			switch {
			case errors.Is(err, session.ErrNotFound):
				return fail(fmt.Errorf("%w: sleeve exited during startup (run the printed command by hand to see why): %s",
					ErrUnavailable, strings.Join(launch.Argv, " ")))
			case err == nil && s.State == session.Exited:
				tail, _ := m.opts.Sessions.Capture(ctx, info.Session, 40)
				return fail(fmt.Errorf("%w: sleeve exited during startup with %s: %s\n%s",
					ErrUnavailable, exitLabel(s), strings.Join(launch.Argv, " "), session.Tail(tail, 20)))
			}
		}
		select {
		case <-ctx.Done():
			return fail(ctx.Err())
		case <-time.After(scopePollInterval):
		}
	}
}

// Kill stops the sleeve's processes and retains its record. Its workspace and home
// directory stay on disk.
func (m *Manager) Kill(ctx context.Context, name string) error {
	if err := m.reserve(ctx, name); err != nil {
		return err
	}
	defer m.release(name)
	m.mu.Lock()
	e := m.sleeves[name]
	m.mu.Unlock()
	if e == nil {
		return ErrNotFound
	}
	if e.Session != "" {
		if err := m.opts.Sessions.Kill(ctx, e.Session); err != nil && !errors.Is(err, session.ErrNotFound) {
			return fmt.Errorf("kill session: %w", err)
		}
	}
	if e.State == Running {
		if err := m.cfg.killScope(ctx, name); err != nil {
			return err
		}
	}
	if err := m.cfg.removeEnvFile(name); err != nil {
		log.Printf("sleeve %s: %v", name, err)
	}
	m.mu.Lock()
	wasRunning := e.State == Running
	previous := e.Info
	e.State, e.Killed, e.Session, e.Chat = Exited, true, "", nil
	if e.Exited.IsZero() {
		e.Exited = time.Now()
	}
	e.Usage.CPUPercent, e.Usage.Pids = 0, 0
	err := m.saveLocked()
	if err != nil {
		e.Info = previous
	}
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
// exited. A session whose command ended is kept for diagnosis and its exit
// status recorded; a session whose scope vanished is killed.
func (m *Manager) Reconcile(ctx context.Context) error {
	list, err := m.opts.Sessions.List(ctx)
	if err != nil {
		return fmt.Errorf("list sessions: %w", err)
	}
	live := make(map[string]session.Session, len(list))
	for _, s := range list {
		live[s.Name] = s
	}
	type gone struct {
		e *entry
		s *session.Session
	}
	m.mu.Lock()
	var out []gone
	for name, e := range m.sleeves {
		if _, busy := m.pending[name]; e.State != Running || e.Mode == ModeChat || busy {
			continue
		}
		s, ok := live[e.Session]
		alive := m.cfg.scopeAlive(name)
		if alive {
			e.scopeGone = time.Time{}
		}
		switch {
		case !ok:
			out = append(out, gone{e: e})
		case s.State == session.Exited:
			out = append(out, gone{e: e, s: &s})
		case !alive:
			if e.scopeGone.IsZero() {
				e.scopeGone = time.Now()
			}
			if time.Since(e.scopeGone) >= scopeGrace {
				out = append(out, gone{e: e, s: &s})
			}
		}
	}
	m.mu.Unlock()
	for _, g := range out {
		if err := m.markExited(ctx, g.e, g.s); err != nil {
			log.Printf("sleeve %s: %v", g.e.Name, err)
		}
	}
	return nil
}

func exitLabel(s session.Session) string {
	if s.ExitSignal > 0 {
		return fmt.Sprintf("signal %d (status %d)", s.ExitSignal, s.ExitCode)
	}
	return fmt.Sprintf("status %d", s.ExitCode)
}

// markExited records e as exited. s is its tmux session if one still exists.
func (m *Manager) markExited(ctx context.Context, e *entry, s *session.Session) error {
	if !m.tryReserve(e.Name) {
		return nil
	}
	defer m.release(e.Name)
	ctx = context.WithoutCancel(ctx)
	if s != nil && s.State == session.Running {
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
	if s != nil && s.State == session.Exited {
		code := s.ExitCode
		e.ExitCode, e.ExitSignal = &code, s.ExitSignal
		if !s.Exited.IsZero() {
			e.Exited = s.Exited.UTC()
		}
		e.OOMKilled = e.oomSeen > 0 && (code == 137 || s.ExitSignal == 9)
	}
	e.Usage.CPUPercent, e.Usage.Pids = 0, 0
	err := m.saveLocked()
	payload := map[string]any{"reason": "exited", "oom_killed": e.OOMKilled}
	if e.ExitCode != nil {
		payload["exit_code"] = float64(*e.ExitCode)
		payload["exit_signal"] = float64(e.ExitSignal)
	}
	m.mu.Unlock()
	m.opts.Emit("sleeve.exited", e.Name, payload)
	return err
}

// Run reconciles and samples usage until ctx is done.
func (m *Manager) Run(ctx context.Context) {
	m.mu.Lock()
	m.runCtx = ctx
	m.mu.Unlock()
	cacheDone := make(chan struct{})
	go func() {
		defer close(cacheDone)
		m.cfg.goCaches.run(ctx)
	}()
	defer func() { <-cacheDone }()
	tick := time.NewTicker(m.opts.UsageInterval)
	defer tick.Stop()
	oomTick := time.NewTicker(m.opts.OOMInterval)
	defer oomTick.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-oomTick.C:
			m.pollOOM()
			continue
		case <-tick.C:
		}
		if err := m.Reconcile(ctx); err != nil && ctx.Err() == nil {
			log.Printf("sleeves: %v", err)
		}
		m.sample()
	}
}

// pollOOM reads the OOM counter of memory-limited sleeves often, because a
// harness killed by the OOM killer takes its scope, and the counter, with it
// within milliseconds.
func (m *Manager) pollOOM() {
	m.mu.Lock()
	defer m.mu.Unlock()
	for _, e := range m.sleeves {
		if e.State != Running || e.Memory == 0 {
			continue
		}
		if n, ok := m.cfg.readOOMKills(e.Name); ok {
			m.recordOOMLocked(e, n)
		}
	}
}

func (m *Manager) sample() {
	now := time.Now()
	m.mu.Lock()
	defer m.mu.Unlock()
	var due []*entry
	for _, e := range m.sleeves {
		if e.State == Running {
			m.sampleLocked(e, now)
		}
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
	m.recordOOMLocked(e, raw.oomKills)
}

func (m *Manager) recordOOMLocked(e *entry, kills uint64) {
	if kills > e.oomSeen {
		m.opts.Emit("sleeve.oom", e.Name, map[string]any{
			"oom_kills": float64(kills), "new_kills": float64(kills - e.oomSeen),
			"memory_max_bytes": float64(e.Memory), "memory_peak_bytes": float64(e.Usage.MemPeak),
		})
		e.oomSeen = kills
	}
	e.Usage.OOMKills = e.oomSeen
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
		home, caches, goBuild, homeErr := homeSize(ctx, m.cfg.sleeveHome(e.Name))
		if ctx.Err() != nil {
			return
		}
		m.mu.Lock()
		e.Usage.DiskSampled = time.Now()
		if err == nil {
			e.Usage.DiskBytes = n
		}
		if homeErr == nil {
			e.Usage.HomeBytes, e.Usage.CacheBytes, e.Usage.GoBuildCacheBytes = home, caches, goBuild
		}
		m.mu.Unlock()
		if err != nil {
			log.Printf("sleeve %s: measure workspace disk: %v", e.Name, err)
		}
		if homeErr != nil {
			log.Printf("sleeve %s: measure home disk: %v", e.Name, homeErr)
		}
	}
}

var scpURL = regexp.MustCompile(`^[A-Za-z0-9._-]+@[A-Za-z0-9._-]+:`)

func isGitURL(s string) bool { return strings.Contains(s, "://") || scpURL.MatchString(s) }

func (m *Manager) resolveWorkspace(ctx context.Context, req SpawnRequest, base *Info) (string, error) {
	name, ws := req.Name, req.Workspace
	if ws == "" {
		return "", fmt.Errorf("%w: workspace is required", ErrInvalid)
	}
	local := !isGitURL(ws)
	origin := ""
	if local {
		if !filepath.IsAbs(ws) {
			return "", fmt.Errorf("%w: workspace %q must be an absolute path or a git URL", ErrInvalid, ws)
		}
		real, err := filepath.EvalSymlinks(ws)
		if err != nil {
			return "", fmt.Errorf("%w: workspace: %w", ErrInvalid, err)
		}
		if fi, err := os.Stat(real); err != nil || !fi.IsDir() {
			return "", fmt.Errorf("%w: workspace %s is not a directory", ErrInvalid, real)
		}
		if err := m.cfg.CheckWorkspace(name, real); err != nil {
			return "", err
		}
		if req.Branch == "" && req.NewBranch == "" {
			return real, nil
		}
		dirs, err := m.cfg.baseMounts(real)
		if err != nil {
			return "", err
		}
		origin, base.BaseBranch = gitDetails(real)
		origin = gitOrigin(dirs[len(dirs)-1].Source)
		if req.Branch != "" {
			base.BaseBranch = req.Branch
		} else if base.BaseBranch == "" {
			if b, err := os.ReadFile(filepath.Join(dirs[0].Source, "HEAD")); err == nil {
				if ref, ok := strings.CutPrefix(strings.TrimSpace(string(b)), "ref: refs/heads/"); ok {
					base.BaseBranch = ref
				}
			}
		}
		if !validBranch(base.BaseBranch) {
			return "", fmt.Errorf("%w: local source has no current branch; specify --branch", ErrInvalid)
		}
		base.BaseRepo = real
		base.BaseTip, err = BranchTip(real, base.BaseBranch)
		if err != nil {
			return "", err
		}
		req.Branch = base.BaseBranch
		ws = real
	}
	if strings.HasPrefix(ws, "-") {
		return "", fmt.Errorf("%w: invalid git URL", ErrInvalid)
	}
	dest := filepath.Join(m.cfg.workspacesDir(), name)
	if err := m.cfg.CheckWorkspace(name, dest); err != nil {
		return "", err
	}
	if real, err := filepath.EvalSymlinks(dest); err == nil && real != dest {
		return "", fmt.Errorf("%w: clone destination must not be a symlink", ErrInvalid)
	}
	if _, err := os.Stat(filepath.Join(dest, ".git")); err == nil {
		if req.Branch != "" || req.NewBranch != "" {
			return "", fmt.Errorf("%w: branch dispatch requires a fresh workspace; %s already exists", ErrExists, dest)
		}
		return dest, nil
	}
	if ents, err := os.ReadDir(dest); err == nil && len(ents) > 0 {
		return "", fmt.Errorf("%w: %s exists and is not a git checkout", ErrExists, dest)
	}
	if err := os.MkdirAll(filepath.Dir(dest), 0o700); err != nil {
		return "", fmt.Errorf("create workspaces dir: %w", err)
	}
	if real, err := filepath.EvalSymlinks(filepath.Dir(dest)); err != nil || real != filepath.Dir(dest) {
		return "", fmt.Errorf("%w: workspaces directory must not be a symlink", ErrInvalid)
	}
	cctx, cancel := context.WithTimeout(ctx, 10*time.Minute)
	defer cancel()
	run := func(args ...string) error {
		cmd := exec.CommandContext(cctx, "git", append([]string{"-c", "core.hooksPath=/dev/null", "-c", "core.fsmonitor=false", "-c", "init.templateDir="}, args...)...)
		cmd.Dir = m.cfg.Home
		cmd.Env = append(os.Environ(), "GIT_TERMINAL_PROMPT=0", "GIT_ALLOW_PROTOCOL=https:http:ssh:git:file")
		if args[0] != "clone" {
			cmd.Env = append(cmd.Env, "GIT_CONFIG_GLOBAL=/dev/null", "GIT_CONFIG_NOSYSTEM=1")
		}
		var stderr bytes.Buffer
		cmd.Stderr = &stderr
		if err := cmd.Run(); err != nil {
			return fmt.Errorf("%w: git %s: %w: %s", ErrInvalid, args[0], err, strings.TrimSpace(stderr.String()))
		}
		return nil
	}
	args := []string{"clone", "--quiet", "--no-checkout", "--no-hardlinks", "--dissociate"}
	if req.Branch != "" {
		args = append(args, "--branch", req.Branch)
	}
	if err := run(append(args, "--", ws, dest)...); err != nil {
		return "", err
	}
	if local {
		dirs, err := m.cfg.baseMounts(ws)
		if err != nil {
			return "", err
		}
		if err := run("-C", dest, "remote", "rename", "origin", "base"); err != nil {
			return "", err
		}
		if err := run("-C", dest, "remote", "set-url", "base", dirs[0].Target); err != nil {
			return "", err
		}
		if origin != "" {
			if err := run("-C", dest, "remote", "add", "origin", origin); err != nil {
				return "", err
			}
		}
	} else {
		if err := run("-C", dest, "remote", "set-url", "origin", stripCredentials(ws)); err != nil {
			return "", err
		}
	}
	if req.NewBranch != "" {
		if err := run("-C", dest, "switch", "--quiet", "-c", req.NewBranch); err != nil {
			return "", err
		}
	} else {
		if branch := WorkspaceBranch(dest); branch != "" {
			if _, err := BranchTip(dest, branch); err != nil {
				return dest, nil
			}
		}
		if err := run("-C", dest, "checkout", "--quiet"); err != nil {
			return "", err
		}
	}
	return dest, nil
}
