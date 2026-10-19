package hub

import (
	"context"
	"errors"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/structpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
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
func (h *Hub) SetSleeves(m *sleeve.Manager) { h.sleeves = m }

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

func sleeveToProto(i sleeve.Info) *pb.Sleeve {
	s := &pb.Sleeve{
		Name:      i.Name,
		Harness:   harnessToProto[i.Harness],
		Workspace: i.Workspace,
		Session:   i.Session,
		CreatedAt: timestamppb.New(i.Created),
		Args:      i.Args,
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
			out = append(out, sleeveToProto(i))
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
	r := sleeve.SpawnRequest{
		Name: m.Name, Workspace: m.Workspace, Harness: harnessFromProto(m.Harness),
		Args: m.Args, Env: m.Env,
	}
	if l := m.Limits; l != nil {
		r.Limits = sleeve.Limits{CPUPercent: l.CpuPercent, MemoryBytes: l.MemoryBytes}
	}
	info, err := h.sleeves.Spawn(ctx, r)
	if err != nil {
		return nil, sleeveErr(err)
	}
	h.scanSessions(ctx)
	return connect.NewResponse(&pb.SpawnSleeveResponse{Sleeve: sleeveToProto(info)}), nil
}

func (h *Hub) ListSleeves(context.Context, *connect.Request[pb.ListSleevesRequest]) (*connect.Response[pb.ListSleevesResponse], error) {
	res := &pb.ListSleevesResponse{}
	for _, i := range h.sleeves.List() {
		res.Sleeves = append(res.Sleeves, sleeveToProto(i))
	}
	return connect.NewResponse(res), nil
}

func (h *Hub) GetSleeve(_ context.Context, req *connect.Request[pb.GetSleeveRequest]) (*connect.Response[pb.GetSleeveResponse], error) {
	i, err := h.sleeves.Get(req.Msg.Name)
	if err != nil {
		return nil, sleeveErr(err)
	}
	return connect.NewResponse(&pb.GetSleeveResponse{Sleeve: sleeveToProto(i)}), nil
}

func (h *Hub) KillSleeve(ctx context.Context, req *connect.Request[pb.KillSleeveRequest]) (*connect.Response[pb.KillSleeveResponse], error) {
	if err := h.sleeves.Kill(ctx, req.Msg.Name); err != nil {
		return nil, sleeveErr(err)
	}
	h.scanSessions(ctx)
	return connect.NewResponse(&pb.KillSleeveResponse{}), nil
}
