package hub

import (
	"context"
	"errors"
	"fmt"
	"log"
	"strings"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/protobuf/types/known/structpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/driver"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

// SleeveOptions returns the manager options wired to this hub's sessions
// and event bus.
func (h *Hub) SleeveOptions(cfg sleeve.Config) sleeve.Options {
	return sleeve.Options{
		Config:   cfg,
		Sessions: h.Sessions,
		Emit: func(kind, subject string, payload map[string]any) {
			p, err := structpb.NewStruct(payload)
			if err != nil {
				return
			}
			h.emit(kind, subject, p)
		},
	}
}

// SetSleeves enables SleeveService. Call before Handler or Serve.
func (h *Hub) SetSleeves(m *sleeve.Manager) {
	h.sleeves = m
	h.chat = newChatManager(h)
	h.dropHelpers(context.Background())
	h.chat.recover()
	h.migrateEnvoyTokens(time.Now())
}

// sleeveProto is sleeveToProto with the live chat status laid over the
// persisted one.
func (h *Hub) sleeveProto(i sleeve.Info) *pb.Sleeve {
	s := sleeveToProto(i)
	h.chat.overlay(s)
	return s
}

var harnessToProto = map[sleeve.Harness]pb.Harness{
	sleeve.Claude: pb.Harness_HARNESS_CLAUDE,
	sleeve.Codex:  pb.Harness_HARNESS_CODEX,
	sleeve.Grok:   pb.Harness_HARNESS_GROK,
	sleeve.Pi:     pb.Harness_HARNESS_PI,
	sleeve.Shell:  pb.Harness_HARNESS_SHELL,
}

func harnessFromProto(h pb.Harness) sleeve.Harness {
	for k, v := range harnessToProto {
		if v == h {
			return k
		}
	}
	return ""
}

func modeFromProto(m pb.SleeveMode) sleeve.Mode {
	switch m {
	case pb.SleeveMode_SLEEVE_MODE_CHAT:
		return sleeve.ModeChat
	case pb.SleeveMode_SLEEVE_MODE_TERMINAL:
		return sleeve.ModeTerminal
	}
	return ""
}

func modeToProto(m sleeve.Mode) pb.SleeveMode {
	if m == sleeve.ModeChat {
		return pb.SleeveMode_SLEEVE_MODE_CHAT
	}
	return pb.SleeveMode_SLEEVE_MODE_TERMINAL
}

func chatToProto(c *sleeve.Chat) *pb.ChatStatus {
	if c == nil {
		return nil
	}
	s := &pb.ChatStatus{
		State:            pb.ChatState(pb.ChatState_value[c.State]),
		HarnessSessionId: c.SessionID,
		Model:            c.Model,
		ReasoningEffort:  c.Effort,
		Settings:         settingsToProto(driver.SelectionState{Requested: c.Requested, Effective: driver.Selection{Model: c.Model, Effort: c.Effort}, Defaults: c.Defaults}),
		ContextTokens:    c.Context,
		ContextWindow:    c.Window,
		LastSeq:          c.LastSeq,
		Provider:         c.Provider,
		LocalModel:       c.LocalModel,
	}
	if !c.CacheExpires.IsZero() {
		s.CacheExpiresAt = timestamppb.New(c.CacheExpires)
	}
	if !c.LastActivity.IsZero() {
		s.LastActivityAt = timestamppb.New(c.LastActivity)
	}
	return s
}

func handoffToProto(h *sleeve.Handoff) *pb.Handoff {
	if h == nil {
		return nil
	}
	return &pb.Handoff{
		FromHarness: h.From, FromSessionId: h.FromSession, ToHarness: h.To,
		SummarizerHarness: h.Summarizer, SummarizerModel: h.SummarizerModel, SummarizerProvider: h.SummarizerProvider,
		SummarizerLocal: h.SummarizerLocal, SummaryError: h.SummaryError,
		Path: h.Path, ArchivePath: h.Archive, At: timestamppb.New(h.At),
	}
}

func sleeveToProto(i sleeve.Info) *pb.Sleeve {
	s := &pb.Sleeve{
		BaseRepo: i.BaseRepo, BaseBranch: i.BaseBranch, BaseTip: i.BaseTip,
		Parent:     i.Parent,
		Killed:     i.Killed,
		Name:       i.Name,
		Harness:    harnessToProto[i.Harness],
		Workspace:  i.Workspace,
		GitBranch:  sleeve.WorkspaceBranch(i.Workspace),
		Session:    i.Session,
		CreatedAt:  timestamppb.New(i.Created),
		Args:       i.Args,
		Mode:       modeToProto(i.Mode),
		Chat:       chatToProto(i.Chat),
		Supervised: i.Supervised,
		Nested:     i.Nested,
		Handoff:    handoffToProto(i.Handoff),
		Result:     resultToProto(i.Result),
		Limits:     &pb.SleeveLimits{CpuPercent: i.CPU, MemoryBytes: i.Memory},
		State:      pb.SleeveState_SLEEVE_STATE_RUNNING,
		Usage: &pb.SleeveUsage{
			CpuPercent:            i.Usage.CPUPercent,
			CpuSeconds:            i.Usage.CPUSeconds,
			MemoryCurrentBytes:    i.Usage.MemCurrent,
			MemoryPeakBytes:       i.Usage.MemPeak,
			Pids:                  uint32(i.Usage.Pids),
			OomKills:              i.Usage.OOMKills,
			WorkspaceDiskBytes:    i.Usage.DiskBytes,
			HomeDiskBytes:         i.Usage.HomeBytes,
			CacheDiskBytes:        i.Usage.CacheBytes,
			GoBuildCacheDiskBytes: i.Usage.GoBuildCacheBytes,
		},
	}
	s.ExitAfterIdle = durationpb.New(i.ExitAfterIdle)
	for _, m := range i.ReadOnly {
		s.ReadOnly = append(s.ReadOnly, &pb.ReadOnlyMount{Source: m.Source, Target: m.Target})
	}
	if !i.Usage.Sampled.IsZero() {
		s.Usage.SampledAt = timestamppb.New(i.Usage.Sampled)
	}
	if !i.Usage.DiskSampled.IsZero() {
		s.Usage.DiskSampledAt = timestamppb.New(i.Usage.DiskSampled)
	}
	if i.State == sleeve.Exited {
		s.State = pb.SleeveState_SLEEVE_STATE_EXITED
		s.ExitedAt = timestamppb.New(i.Exited)
		s.ExitSignal = int32(i.ExitSignal)
		s.OomKilled = i.OOMKilled
		if i.ExitCode != nil {
			c := int32(*i.ExitCode)
			s.ExitCode = &c
		}
	}
	return s
}

func (h *Hub) sleeveRows() []*pb.Sleeve {
	if h.sleeves == nil {
		return nil
	}
	var out []*pb.Sleeve
	for _, i := range h.sleeves.List() {
		if i.State == sleeve.Running && i.Helper == "" {
			out = append(out, h.sleeveProto(i))
		}
	}
	return out
}

func sleeveErr(err error) error {
	switch {
	case errors.Is(err, sleeve.ErrNotFound):
		return connect.NewError(connect.CodeNotFound, err)
	case errors.Is(err, sleeve.ErrExists):
		return connect.NewError(connect.CodeAlreadyExists, err)
	case errors.Is(err, sleeve.ErrInvalid):
		return connect.NewError(connect.CodeInvalidArgument, err)
	case errors.Is(err, sleeve.ErrUnsupported), errors.Is(err, sleeve.ErrUnavailable):
		return connect.NewError(connect.CodeFailedPrecondition, err)
	}
	return connect.NewError(connect.CodeInternal, err)
}

func (h *Hub) SpawnSleeve(ctx context.Context, req *connect.Request[pb.SpawnSleeveRequest]) (*connect.Response[pb.SpawnSleeveResponse], error) {
	m := req.Msg
	// The orchestrator's name owns the orchestrator directory and its token,
	// so only SpawnEnvoy, which mints that token first, may claim it.
	if m.Name == sleeve.OrchestratorName && !spawningEnvoy(ctx) {
		return nil, connect.NewError(connect.CodePermissionDenied, fmt.Errorf("sleeve name %q is reserved for the orchestrator; start it with the owner token via `protectorate orchestrator spawn` or START ENVOY in the app", m.Name))
	}
	r := sleeve.SpawnRequest{
		Branch: m.Branch, NewBranch: m.NewBranch, Brief: m.Brief,
		Selection: selectionFromProto(m.Selection),
		Name:      m.Name, Workspace: m.Workspace, Harness: harnessFromProto(m.Harness),
		Args: m.Args, Env: m.Env, Mode: modeFromProto(m.Mode), Supervised: m.Supervised,
		Nested: m.Nested,
	}
	if p := auth.PrincipalFrom(ctx); p != nil {
		r.Parent, r.ParentID = "token:"+p.Name, p.ID
		switch {
		case p.Owner:
			r.Parent = "owner"
		case p.Name == sleeve.OrchestratorTokenName:
			r.Parent = sleeve.OrchestratorName
		case strings.HasPrefix(p.Name, sleeveTokenPrefix):
			r.Parent = p.Name
		}
	}
	if d := m.ExitAfterIdle; d != nil {
		if err := d.CheckValid(); err != nil {
			return nil, connect.NewError(connect.CodeInvalidArgument, err)
		}
		r.ExitAfterIdle = d.AsDuration()
	}
	for _, mount := range m.ReadOnly {
		r.ReadOnly = append(r.ReadOnly, sleeve.Mount{Source: mount.Source, Target: mount.Target})
	}
	if l := m.Limits; l != nil {
		r.Limits = sleeve.Limits{CPUPercent: l.CpuPercent, MemoryBytes: l.MemoryBytes}
	}
	if r.Mode != sleeve.ModeTerminal {
		r.Selection, r.EffortFallback = h.spawnSelection(driver.Harness(r.Harness), r.Selection)
	}
	if !spawningEnvoy(ctx) {
		if err := h.validateSpawnSelection(ctx, driver.Harness(r.Harness), r.Selection, r.EffortFallback); err != nil {
			return nil, err
		}
	}
	info, err := h.spawn(ctx, r, spawnOpts{})
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(&pb.SpawnSleeveResponse{Sleeve: h.sleeveProto(info)}), nil
}

type spawnOpts struct {
	selection      driver.Selection
	defaults       driver.Selection
	effortFallback bool
	// keepLog continues the sleeve's chat log instead of moving it aside.
	keepLog bool
	// resume is a native session id the chat driver resumes instead of
	// starting a new session.
	resume string
	// hold leaves the new chat taken over by the hub.
	hold bool
}

// spawn starts a sleeve with its own identity token and, in chat mode, its
// driver, undoing all of it if any step fails.
func (h *Hub) spawn(ctx context.Context, r sleeve.SpawnRequest, o spawnOpts) (sleeve.Info, error) {
	start := time.Now()
	// the orchestrator carries its own token, minted by `orchestrator spawn`
	var identity auth.Token
	var secret string
	if r.Name != sleeve.OrchestratorName {
		var err error
		if identity, secret, err = h.issueIdentity(r.Name); err != nil {
			return sleeve.Info{}, connect.NewError(connect.CodeInternal, err)
		}
		env := map[string]string{}
		for k, v := range r.Env {
			env[k] = v
		}
		for k, v := range h.identityEnv() {
			env[k] = v
		}
		r.Env = env
	}
	revokeNew := func() {
		if identity.ID != "" {
			if _, err := h.authz.Store().Revoke(identity.ID, time.Now()); err != nil {
				log.Printf("sleeve %s: revoke unused token: %v", r.Name, err)
			}
		}
	}
	info, err := h.sleeves.Spawn(ctx, r)
	if err != nil {
		revokeNew()
		return sleeve.Info{}, sleeveErr(err)
	}
	if identity.ID != "" {
		if err := h.installIdentity(info.Name, secret); err != nil {
			revokeNew()
			if kerr := h.sleeves.Kill(context.WithoutCancel(ctx), info.Name); kerr != nil {
				log.Printf("sleeve %s: kill after failed token install: %v", info.Name, kerr)
			}
			return sleeve.Info{}, connect.NewError(connect.CodeInternal, err)
		}
		h.dropIdentity(info.Name, identity.ID)
	}
	if info.Mode == sleeve.ModeChat {
		if o.resume != "" {
			if _, err := h.sleeves.SetChat(info.Name, func(c *sleeve.Chat) { c.SessionID = o.resume; c.Defaults = o.defaults }); err != nil {
				log.Printf("sleeve %s: record session to resume: %v", info.Name, err)
			}
		}
		if err := h.chat.start(ctx, info.Name, !o.keepLog, o.hold, r.Brief); err != nil {
			h.chat.remove(info.Name)
			if kerr := h.sleeves.Kill(context.WithoutCancel(ctx), info.Name); kerr != nil {
				log.Printf("sleeve %s: kill after failed chat start: %v", info.Name, kerr)
			}
			h.dropIdentity(info.Name, "")
			return sleeve.Info{}, err
		}
		info, _ = h.sleeves.Get(info.Name)
		payload, err := structpb.NewStruct(map[string]any{
			"harness": string(info.Harness), "workspace": info.Workspace, "mode": string(info.Mode),
			"spawn_ms": float64(time.Since(start).Milliseconds()),
		})
		if err == nil {
			h.emit("sleeve.spawned", info.Name, payload)
		}
	}
	h.scanSessions(ctx)
	h.pokeCoord()
	return info, nil
}

func (h *Hub) ListSleeves(context.Context, *connect.Request[pb.ListSleevesRequest]) (*connect.Response[pb.ListSleevesResponse], error) {
	res := &pb.ListSleevesResponse{}
	for _, i := range h.sleeves.List() {
		if i.Helper == "" {
			res.Sleeves = append(res.Sleeves, h.sleeveProto(i))
		}
	}
	return connect.NewResponse(res), nil
}

func (h *Hub) ListWorkspaces(context.Context, *connect.Request[pb.ListWorkspacesRequest]) (*connect.Response[pb.ListWorkspacesResponse], error) {
	res := &pb.ListWorkspacesResponse{}
	for _, w := range h.sleeves.Workspaces() {
		res.Workspaces = append(res.Workspaces, &pb.Workspace{
			Name: w.Name, Path: w.Path, GitRemote: w.Remote, GitBranch: w.Branch,
			Sleeve: w.Sleeve, Busy: w.Busy, RequiredName: w.RequiredName, ReadOnly: w.ReadOnly,
		})
	}
	return connect.NewResponse(res), nil
}

func (h *Hub) BrowseWorkspaces(ctx context.Context, req *connect.Request[pb.BrowseWorkspacesRequest]) (*connect.Response[pb.BrowseWorkspacesResponse], error) {
	if _, ok := callerSleeve(ctx); ok {
		return nil, connect.NewError(connect.CodePermissionDenied, errors.New("sleeve identity tokens cannot browse workspace directories"))
	}
	dirs, path, parent, truncated, err := h.sleeves.Browse(ctx, req.Msg.Path)
	if err != nil {
		return nil, sleeveErr(err)
	}
	res := &pb.BrowseWorkspacesResponse{Path: path, Parent: parent, Truncated: truncated}
	for _, d := range dirs {
		res.Directories = append(res.Directories, &pb.WorkspaceDirectory{Name: d.Name, Path: d.Path, IsGit: d.IsGit, GitBranch: d.Branch, GitRemote: d.Remote, Acceptable: d.Acceptable, Reason: d.Reason, RequiredName: d.RequiredName})
	}
	return connect.NewResponse(res), nil
}

func (c *coordinator) reapIdle(ctx context.Context) {
	c.passMu.Lock()
	defer c.passMu.Unlock()
	for _, info := range c.h.sleeves.List() {
		if info.State != sleeve.Running || info.Mode != sleeve.ModeChat || info.ExitAfterIdle <= 0 {
			continue
		}
		c.h.chat.mu.Lock()
		s := c.h.chat.sleeves[info.Name]
		c.h.chat.mu.Unlock()
		if s == nil {
			continue
		}
		if !s.opMu.TryLock() {
			continue
		}
		s.mu.Lock()
		idle := c.h.chatIdleLocked(s)
		if !idle {
			s.idleSince = time.Time{}
		} else if s.idleSince.IsZero() {
			s.idleSince = time.Now()
		}
		due := idle && time.Since(s.idleSince) >= info.ExitAfterIdle
		if due {
			s.hubOwned = true
		}
		s.mu.Unlock()
		s.opMu.Unlock()
		if due {
			if _, err := c.h.KillSleeve(ctx, connect.NewRequest(&pb.KillSleeveRequest{Name: info.Name})); err != nil {
				s.giveBack()
				log.Printf("reap idle sleeve %s: %v", info.Name, err)
			}
		}
	}
}

func (h *Hub) GetSleeve(_ context.Context, req *connect.Request[pb.GetSleeveRequest]) (*connect.Response[pb.GetSleeveResponse], error) {
	i, err := h.sleeves.Get(req.Msg.Name)
	if err != nil {
		return nil, sleeveErr(err)
	}
	return connect.NewResponse(&pb.GetSleeveResponse{Sleeve: h.sleeveProto(i)}), nil
}

func (h *Hub) KillSleeve(ctx context.Context, req *connect.Request[pb.KillSleeveRequest]) (*connect.Response[pb.KillSleeveResponse], error) {
	if req.Msg.DropCaches {
		p := auth.PrincipalFrom(ctx)
		if p == nil || !p.Owner {
			return nil, connect.NewError(connect.CodePermissionDenied, errors.New("dropping caches requires the owner token"))
		}
	}
	h.chat.remove(req.Msg.Name)
	if err := h.sleeves.Kill(ctx, req.Msg.Name); err != nil {
		return nil, sleeveErr(err)
	}
	h.dropIdentity(req.Msg.Name, "")
	h.emit("sleeve.killed", req.Msg.Name, nil)
	if req.Msg.DropCaches {
		if err := h.sleeves.DropCaches(ctx, req.Msg.Name); err != nil {
			return nil, sleeveErr(err)
		}
	}
	h.scanSessions(ctx)
	h.pokeCoord()
	return connect.NewResponse(&pb.KillSleeveResponse{}), nil
}

func (h *Hub) ownsSleeve(p *auth.Principal, msg any) bool {
	if p == nil || h.sleeves == nil {
		return false
	}
	var name string
	switch r := msg.(type) {
	case *pb.KillSleeveRequest:
		if r.DropCaches {
			return false
		}
		name = r.Name
	default:
		return false
	}
	if name == sleeve.OrchestratorName {
		return false
	}
	info, err := h.sleeves.Get(name)
	if err != nil {
		return false
	}
	return info.ParentID != "" && info.ParentID == p.ID ||
		info.Parent == sleeve.OrchestratorName && p.Name == sleeve.OrchestratorTokenName ||
		info.Parent == p.Name && strings.HasPrefix(p.Name, sleeveTokenPrefix)
}

func (h *Hub) PruneSleeves(ctx context.Context, req *connect.Request[pb.PruneSleevesRequest]) (*connect.Response[pb.PruneSleevesResponse], error) {
	r := req.Msg
	entries, err := h.sleeves.Prune(ctx, sleeve.PruneOptions{Yes: r.Yes, Caches: r.Caches, Homes: r.Homes, Workspaces: r.Workspaces, Running: r.Running})
	if err != nil {
		return nil, sleeveErr(err)
	}
	res := &pb.PruneSleevesResponse{}
	for _, e := range entries {
		res.Entries = append(res.Entries, &pb.SleeveDiskEntry{Sleeve: e.Sleeve, Path: e.Path, Kind: e.Kind, Bytes: e.Bytes, Eligible: e.Eligible, Removed: e.Removed, Reason: e.Reason})
	}
	return connect.NewResponse(res), nil
}
