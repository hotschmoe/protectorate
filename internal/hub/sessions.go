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
)

const (
	EventSessionCreated = "session.created"
	EventSessionExited  = "session.exited"
)

func toProto(s session.Session) *pb.Session {
	return &pb.Session{
		Name:            s.Name,
		Cwd:             s.Cwd,
		Command:         s.Command,
		CreatedAt:       timestamppb.New(s.Created),
		AttachedClients: int32(s.AttachedClients),
		LastActivity:    timestamppb.New(s.LastActivity),
		PaneCols:        int32(s.Cols),
		PaneRows:        int32(s.Rows),
	}
}

func sessionErr(err error) error {
	switch {
	case errors.Is(err, session.ErrNotFound):
		return connect.NewError(connect.CodeNotFound, err)
	case errors.Is(err, session.ErrExists):
		return connect.NewError(connect.CodeAlreadyExists, err)
	case errors.Is(err, session.ErrInvalidName):
		return connect.NewError(connect.CodeInvalidArgument, err)
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

func (h *Hub) CapturePane(ctx context.Context, req *connect.Request[pb.CapturePaneRequest]) (*connect.Response[pb.CapturePaneResponse], error) {
	text, err := h.Sessions.Capture(ctx, req.Msg.Name, int(req.Msg.Lines))
	if err != nil {
		return nil, sessionErr(err)
	}
	return connect.NewResponse(&pb.CapturePaneResponse{Text: text}), nil
}

// WatchSessions polls tmux and publishes session.created / session.exited for
// every change, including sessions started outside the hub, until ctx ends.
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
		cur[s.Name] = s
		if _, ok := h.known[s.Name]; !ok && h.seeded {
			h.publishSession(EventSessionCreated, s)
		}
	}
	for name, s := range h.known {
		if _, ok := cur[name]; !ok {
			h.publishSession(EventSessionExited, s)
		}
	}
	h.known, h.seeded = cur, true
}

func (h *Hub) publishSession(kind string, s session.Session) {
	payload, err := structpb.NewStruct(map[string]any{"name": s.Name, "cwd": s.Cwd})
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
