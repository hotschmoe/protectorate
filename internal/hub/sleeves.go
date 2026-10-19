package hub

import (
	"context"
	"errors"
	"fmt"
	"log"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/structpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/auth"
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
		ContextTokens:    c.Context,
		ContextWindow:    c.Window,
		LastSeq:          c.LastSeq,
	}
	if !c.CacheExpires.IsZero() {
		s.CacheExpiresAt = timestamppb.New(c.CacheExpires)
	}
	if !c.LastActivity.IsZero() {
		s.LastActivityAt = timestamppb.New(c.LastActivity)
	}
	return s
}

func sleeveToProto(i sleeve.Info) *pb.Sleeve {
	s := &pb.Sleeve{
		Name:      i.Name,
		Harness:   harnessToProto[i.Harness],
		Workspace: i.Workspace,
		Session:   i.Session,
		CreatedAt: timestamppb.New(i.Created),
		Args:      i.Args,
		Mode:      modeToProto(i.Mode),
		Chat:      chatToProto(i.Chat),
		Limits:    &pb.SleeveLimits{CpuPercent: i.CPU, MemoryBytes: i.Memory},
		State:     pb.SleeveState_SLEEVE_STATE_RUNNING,
		Usage: &pb.SleeveUsage{
			CpuPercent:         i.Usage.CPUPercent,
			CpuSeconds:         i.Usage.CPUSeconds,
			MemoryCurrentBytes: i.Usage.MemCurrent,
			MemoryPeakBytes:    i.Usage.MemPeak,
			Pids:               uint32(i.Usage.Pids),
			OomKills:           i.Usage.OOMKills,
			WorkspaceDiskBytes: i.Usage.DiskBytes,
		},
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
		if i.State == sleeve.Running {
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
	// so only the owner (through `orchestrator spawn`) may claim it.
	if m.Name == sleeve.OrchestratorName {
		if p := auth.PrincipalFrom(ctx); p == nil || !p.Owner {
			return nil, connect.NewError(connect.CodePermissionDenied, fmt.Errorf("sleeve name %q is reserved for the orchestrator; spawn it with the owner token via `protectorate orchestrator spawn`", m.Name))
		}
	}
	r := sleeve.SpawnRequest{
		Name: m.Name, Workspace: m.Workspace, Harness: harnessFromProto(m.Harness),
		Args: m.Args, Env: m.Env, Mode: modeFromProto(m.Mode),
	}
	// the orchestrator carries its own token, minted by `orchestrator spawn`
	var identity auth.Token
	var secret string
	if m.Name != sleeve.OrchestratorName {
		var err error
		if identity, secret, err = h.issueIdentity(m.Name); err != nil {
			return nil, connect.NewError(connect.CodeInternal, err)
		}
		r.Env = map[string]string{}
		for k, v := range m.Env {
			r.Env[k] = v
		}
		for k, v := range h.identityEnv() {
			r.Env[k] = v
		}
	}
	revokeNew := func() {
		if identity.ID != "" {
			if _, err := h.authz.Store().Revoke(identity.ID, time.Now()); err != nil {
				log.Printf("sleeve %s: revoke unused token: %v", m.Name, err)
			}
		}
	}
	if l := m.Limits; l != nil {
		r.Limits = sleeve.Limits{CPUPercent: l.CpuPercent, MemoryBytes: l.MemoryBytes}
	}
	info, err := h.sleeves.Spawn(ctx, r)
	if err != nil {
		revokeNew()
		return nil, sleeveErr(err)
	}
	if identity.ID != "" {
		if err := h.installIdentity(info.Name, secret); err != nil {
			revokeNew()
			if kerr := h.sleeves.Kill(context.WithoutCancel(ctx), info.Name); kerr != nil {
				log.Printf("sleeve %s: kill after failed token install: %v", info.Name, kerr)
			}
			return nil, connect.NewError(connect.CodeInternal, err)
		}
		h.dropIdentity(info.Name, identity.ID)
	}
	if info.Mode == sleeve.ModeChat {
		if err := h.chat.start(ctx, info.Name); err != nil {
			h.chat.remove(info.Name)
			if kerr := h.sleeves.Kill(context.WithoutCancel(ctx), info.Name); kerr != nil {
				log.Printf("sleeve %s: kill after failed chat start: %v", info.Name, kerr)
			}
			h.dropIdentity(info.Name, "")
			return nil, err
		}
		info, _ = h.sleeves.Get(info.Name)
	}
	h.scanSessions(ctx)
	return connect.NewResponse(&pb.SpawnSleeveResponse{Sleeve: h.sleeveProto(info)}), nil
}

func (h *Hub) ListSleeves(context.Context, *connect.Request[pb.ListSleevesRequest]) (*connect.Response[pb.ListSleevesResponse], error) {
	res := &pb.ListSleevesResponse{}
	for _, i := range h.sleeves.List() {
		res.Sleeves = append(res.Sleeves, h.sleeveProto(i))
	}
	return connect.NewResponse(res), nil
}

func (h *Hub) GetSleeve(_ context.Context, req *connect.Request[pb.GetSleeveRequest]) (*connect.Response[pb.GetSleeveResponse], error) {
	i, err := h.sleeves.Get(req.Msg.Name)
	if err != nil {
		return nil, sleeveErr(err)
	}
	return connect.NewResponse(&pb.GetSleeveResponse{Sleeve: h.sleeveProto(i)}), nil
}

func (h *Hub) KillSleeve(ctx context.Context, req *connect.Request[pb.KillSleeveRequest]) (*connect.Response[pb.KillSleeveResponse], error) {
	h.chat.remove(req.Msg.Name)
	if err := h.sleeves.Kill(ctx, req.Msg.Name); err != nil {
		return nil, sleeveErr(err)
	}
	h.dropIdentity(req.Msg.Name, "")
	h.scanSessions(ctx)
	return connect.NewResponse(&pb.KillSleeveResponse{}), nil
}
