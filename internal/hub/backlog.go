package hub

import (
	"context"
	"errors"
	"strconv"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/structpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/backlog"
)

// SetBacklog installs the backlog store. Call before Handler or Serve.
func (h *Hub) SetBacklog(s *backlog.Store) { h.backlog = s }

func (h *Hub) ListItems(_ context.Context, req *connect.Request[pb.ListItemsRequest]) (*connect.Response[pb.ListItemsResponse], error) {
	f := backlog.Filter{Project: req.Msg.Project, Harness: req.Msg.Harness}
	if st, ok := fromProtoState(req.Msg.State); ok {
		f.State = &st
	}
	items, err := h.backlog.List(f)
	if err != nil {
		return nil, backlogError(err)
	}
	res := &pb.ListItemsResponse{Items: make([]*pb.Item, len(items))}
	for i, it := range items {
		res.Items[i] = toProtoItem(it)
	}
	return connect.NewResponse(res), nil
}

func (h *Hub) AddItem(_ context.Context, req *connect.Request[pb.AddItemRequest]) (*connect.Response[pb.AddItemResponse], error) {
	m := req.Msg
	it, err := h.backlog.Add(backlog.NewItem{
		Project: m.Project, Title: m.Title, IssueURL: m.IssueUrl,
		Priority: int(m.Priority), Harness: m.Harness,
	})
	if err != nil {
		return nil, backlogError(err)
	}
	h.publishBacklog("backlog.added", it)
	return connect.NewResponse(&pb.AddItemResponse{Item: toProtoItem(it)}), nil
}

func (h *Hub) UpdateItem(_ context.Context, req *connect.Request[pb.UpdateItemRequest]) (*connect.Response[pb.UpdateItemResponse], error) {
	st, ok := fromProtoState(req.Msg.State)
	if !ok {
		return nil, connect.NewError(connect.CodeInvalidArgument, errors.New("state is required"))
	}
	it, prev, err := h.backlog.Update(req.Msg.Id, st, req.Msg.GetClaim().GetBy())
	if err != nil {
		return nil, backlogError(err)
	}
	if kind := backlogEventKind(prev, it.State); kind != "" {
		h.publishBacklog(kind, it)
	}
	return connect.NewResponse(&pb.UpdateItemResponse{Item: toProtoItem(it)}), nil
}

func backlogEventKind(prev, now backlog.State) string {
	switch {
	case prev == now:
		return ""
	case now == backlog.StateDone:
		return "backlog.done"
	case now == backlog.StateClaimed:
		return "backlog.claimed"
	case prev == backlog.StateClaimed:
		return "backlog.released"
	}
	return "backlog.reopened"
}

func (h *Hub) publishBacklog(kind string, it backlog.Item) {
	m := map[string]any{
		"id": it.ID, "project": it.Project, "title": it.Title,
		"state": it.State.String(), "donate": it.Donate,
	}
	if it.Claim != nil {
		m["by"] = it.Claim.By
	}
	payload, err := structpb.NewStruct(m)
	if err != nil {
		return
	}
	h.events.publish(&pb.Event{
		Id:      strconv.FormatUint(h.seq.Add(1), 10),
		Time:    timestamppb.Now(),
		Kind:    kind,
		Subject: it.ID,
		Payload: payload,
	})
}

func backlogError(err error) error {
	code := connect.CodeInternal
	switch {
	case errors.Is(err, backlog.ErrNotFound):
		code = connect.CodeNotFound
	case errors.Is(err, backlog.ErrInvalid), errors.Is(err, backlog.ErrAmbiguous):
		code = connect.CodeInvalidArgument
	case errors.Is(err, backlog.ErrUnavailable):
		code = connect.CodeFailedPrecondition
	}
	return connect.NewError(code, err)
}

func fromProtoState(s pb.ItemState) (backlog.State, bool) {
	switch s {
	case pb.ItemState_ITEM_STATE_TODO:
		return backlog.StateTodo, true
	case pb.ItemState_ITEM_STATE_CLAIMED:
		return backlog.StateClaimed, true
	case pb.ItemState_ITEM_STATE_DONE:
		return backlog.StateDone, true
	}
	return 0, false
}

func toProtoItem(it backlog.Item) *pb.Item {
	out := &pb.Item{
		Id: it.ID, Project: it.Project, Title: it.Title, IssueUrl: it.IssueURL,
		Priority: int32(it.Priority), Harness: it.Harness, Notes: it.Notes, Donate: it.Donate,
	}
	switch it.State {
	case backlog.StateClaimed:
		out.State = pb.ItemState_ITEM_STATE_CLAIMED
	case backlog.StateDone:
		out.State = pb.ItemState_ITEM_STATE_DONE
	default:
		out.State = pb.ItemState_ITEM_STATE_TODO
	}
	if it.Claim != nil {
		out.Claim = &pb.Claim{By: it.Claim.By}
		if !it.Claim.At.IsZero() {
			out.Claim.At = timestamppb.New(it.Claim.At)
		}
	}
	return out
}
