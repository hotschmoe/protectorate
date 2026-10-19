package hub

import (
	"context"
	"errors"
	"strconv"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/structpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/session"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

const (
	EventSessionCreated = "session.created"
	EventSessionExited  = "session.exited"
	EventSessionRemoved = "session.removed"
)

func toProto(s session.Session) *pb.Session {
	p := &pb.Session{
		Name:            s.Name,
		Cwd:             s.Cwd,
		Command:         s.Command,
		CreatedAt:       timestamppb.New(s.Created),
		AttachedClients: int32(s.AttachedClients),
		LastActivity:    timestamppb.New(s.LastActivity),
		PaneCols:        int32(s.Cols),
		PaneRows:        int32(s.Rows),
		State:           pb.SessionState_SESSION_STATE_RUNNING,
	}
	if s.State == session.Exited {
		p.State = pb.SessionState_SESSION_STATE_EXITED
		p.ExitCode = int32(s.ExitCode)
		p.ExitSignal = int32(s.ExitSignal)
		if !s.Exited.IsZero() {
			p.ExitedAt = timestamppb.New(s.Exited)
		}
	}
	return p
}

func sessionErr(err error) error {
	switch {
	case errors.Is(err, session.ErrNotFound):
		return connect.NewError(connect.CodeNotFound, err)
	case errors.Is(err, session.ErrExists):
		return connect.NewError(connect.CodeAlreadyExists, err)
	case errors.Is(err, session.ErrInvalidName):
		return connect.NewError(connect.CodeInvalidArgument, err)
	case errors.Is(err, session.ErrNotExited):
		return connect.NewError(connect.CodeFailedPrecondition, err)
	}
	return connect.NewError(connect.CodeInternal, err)
}

func (h *Hub) ListSessions(ctx context.Context, _ *connect.Request[pb.ListSessionsRequest]) (*connect.Response[pb.ListSessionsResponse], error) {
	all, err := h.Sessions.List(ctx)
	if err != nil {
		return nil, sessionErr(err)
	}
	res := &pb.ListSessionsResponse{}
	for _, s := range all {
		res.Sessions = append(res.Sessions, toProto(s))
	}
	return connect.NewResponse(res), nil
}

func (h *Hub) CreateSession(ctx context.Context, req *connect.Request[pb.CreateSessionRequest]) (*connect.Response[pb.CreateSessionResponse], error) {
	m := req.Msg
	if err := refuseSleeveSession(m.Name); err != nil {
		return nil, err
	}
	s, err := h.Sessions.Create(ctx, session.CreateOptions{Name: m.Name, Cwd: m.Cwd, Command: m.Command, Env: m.Env})
	if err != nil {
		return nil, sessionErr(err)
	}
	h.scanSessions(ctx)
	return connect.NewResponse(&pb.CreateSessionResponse{Session: toProto(s)}), nil
}

func (h *Hub) KillSession(ctx context.Context, req *connect.Request[pb.KillSessionRequest]) (*connect.Response[pb.KillSessionResponse], error) {
	if err := h.Sessions.Kill(ctx, req.Msg.Name); err != nil {
		return nil, sessionErr(err)
	}
	h.scanSessions(ctx)
	return connect.NewResponse(&pb.KillSessionResponse{}), nil
}

func (h *Hub) RespawnSession(ctx context.Context, req *connect.Request[pb.RespawnSessionRequest]) (*connect.Response[pb.RespawnSessionResponse], error) {
	if err := refuseSleeveSession(req.Msg.Name); err != nil {
		return nil, err
	}
	s, err := h.Sessions.Respawn(ctx, req.Msg.Name)
	if err != nil {
		return nil, sessionErr(err)
	}
	h.scanSessions(ctx)
	return connect.NewResponse(&pb.RespawnSessionResponse{Session: toProto(s)}), nil
}

func (h *Hub) CapturePane(ctx context.Context, req *connect.Request[pb.CapturePaneRequest]) (*connect.Response[pb.CapturePaneResponse], error) {
	text, err := h.Sessions.Capture(ctx, req.Msg.Name, int(req.Msg.Lines))
	if err != nil {
		return nil, sessionErr(err)
	}
	return connect.NewResponse(&pb.CapturePaneResponse{Text: text}), nil
}

// WatchSessions polls tmux and publishes session.created, session.exited
// (command ended, session kept) and session.removed (session gone) for every
// change, including sessions started outside the hub, and removes exited
// sessions older than DeadRetention, until ctx ends.
func (h *Hub) WatchSessions(ctx context.Context) {
	tick := time.NewTicker(h.SessionPoll)
	defer tick.Stop()
	for {
		h.scanSessions(ctx)
		select {
		case <-ctx.Done():
			return
		case <-tick.C:
		}
	}
}

func (h *Hub) scanSessions(ctx context.Context) {
	h.scanMu.Lock()
	defer h.scanMu.Unlock()
	all, err := h.Sessions.List(ctx)
	if err != nil {
		return
	}
	cur := make(map[string]session.Session, len(all))
	for _, s := range all {
		if h.expired(s) {
			if err := h.Sessions.Kill(ctx, s.Name); err == nil || errors.Is(err, session.ErrNotFound) {
				continue
			}
		}
		cur[s.Name] = s
		prev, ok := h.known[s.Name]
		if !ok && h.seeded {
			h.publishSession(EventSessionCreated, s)
		}
		if s.State == session.Exited && (ok || h.seeded) && prev.State != session.Exited {
			h.publishSession(EventSessionExited, s)
		}
	}
	for name, s := range h.known {
		if _, ok := cur[name]; !ok {
			h.publishSession(EventSessionRemoved, s)
		}
	}
	h.known, h.seeded = cur, true
}

func (h *Hub) expired(s session.Session) bool {
	return h.DeadRetention > 0 && s.State == session.Exited && !s.Exited.IsZero() && time.Since(s.Exited) > h.DeadRetention
}

func (h *Hub) publishSession(kind string, s session.Session) {
	fields := map[string]any{"name": s.Name, "cwd": s.Cwd, "state": string(s.State)}
	if s.State == session.Exited {
		fields["exit_code"] = float64(s.ExitCode)
		fields["exit_signal"] = float64(s.ExitSignal)
		if !s.Exited.IsZero() {
			fields["exited_at"] = s.Exited.UTC().Format(time.RFC3339)
		}
	}
	payload, err := structpb.NewStruct(fields)
	if err != nil {
		return
	}
	h.events.publish(&pb.Event{
		Id:      strconv.FormatUint(h.seq.Add(1), 10),
		Time:    timestamppb.Now(),
		Kind:    kind,
		Subject: s.Name,
		Payload: payload,
	})
}

// refuseSleeveSession keeps plain session RPCs from creating or respawning a
// session under a sleeve's reserved name, which would run it outside the
// sandbox.
func refuseSleeveSession(name string) error {
	if sleeve.IsSleeveSession(name) {
		return connect.NewError(connect.CodeFailedPrecondition, errors.New("session names starting with \"sleeve-\" are reserved for sleeves; use SpawnSleeve"))
	}
	return nil
}
