package hub

import (
	"context"
	"fmt"
	"strings"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

// deskResource is the lease resource that guards a node's desktop.
func deskResource(node string) string { return "desk:" + strings.ToLower(node) }

// requireDeskLease admits a mutating desktop command only for a caller that
// names a live lease on the node's desktop. The hub does not track who is
// calling beyond the token, so holding the lease id is the proof.
func (h *Hub) requireDeskLease(node, leaseID string) error {
	want := deskResource(node)
	if leaseID == "" {
		return connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("this command needs a lease on %s: acquire one with `protectorate lease acquire --node %s` and pass its id with --lease", want, want))
	}
	l, err := h.leases.Get(leaseID)
	if err != nil {
		return err
	}
	if l.Resource != want {
		return connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("lease %s is on %s, not %s", leaseID, l.Resource, want))
	}
	return nil
}

func (h *Hub) deskDo(ctx context.Context, t *pb.DeskTarget, mutating bool, act *pb.DeskActionRequest) (*pb.DeskActionResponse, error) {
	if t == nil || t.Name == "" {
		return nil, connect.NewError(connect.CodeInvalidArgument, fmt.Errorf("target.name is required"))
	}
	ops, err := h.opsFor(t.Name, t.AllowFamily)
	if err != nil {
		return nil, err
	}
	if mutating {
		if err := h.requireDeskLease(t.Name, t.LeaseId); err != nil {
			return nil, err
		}
	}
	return ops.Desk(ctx, act)
}

func (h *Hub) Screenshot(ctx context.Context, req *connect.Request[pb.ScreenshotRequest]) (*connect.Response[pb.ScreenshotResponse], error) {
	res, err := h.deskDo(ctx, req.Msg.Target, false, &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Screenshot{Screenshot: req.Msg}})
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(res.GetScreenshot()), nil
}

func (h *Hub) ListWindows(ctx context.Context, req *connect.Request[pb.ListWindowsRequest]) (*connect.Response[pb.ListWindowsResponse], error) {
	res, err := h.deskDo(ctx, req.Msg.Target, false, &pb.DeskActionRequest{Action: &pb.DeskActionRequest_ListWindows{ListWindows: req.Msg}})
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(res.GetListWindows()), nil
}

func (h *Hub) deskInput(ctx context.Context, t *pb.DeskTarget, act *pb.DeskActionRequest) (*connect.Response[pb.InputResponse], error) {
	if _, err := h.deskDo(ctx, t, true, act); err != nil {
		return nil, err
	}
	return connect.NewResponse(&pb.InputResponse{}), nil
}

func (h *Hub) Click(ctx context.Context, req *connect.Request[pb.ClickRequest]) (*connect.Response[pb.InputResponse], error) {
	return h.deskInput(ctx, req.Msg.Target, &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Click{Click: req.Msg}})
}

func (h *Hub) Move(ctx context.Context, req *connect.Request[pb.MoveRequest]) (*connect.Response[pb.InputResponse], error) {
	return h.deskInput(ctx, req.Msg.Target, &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Move{Move: req.Msg}})
}

func (h *Hub) Type(ctx context.Context, req *connect.Request[pb.TypeRequest]) (*connect.Response[pb.InputResponse], error) {
	return h.deskInput(ctx, req.Msg.Target, &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Type{Type: req.Msg}})
}

func (h *Hub) Key(ctx context.Context, req *connect.Request[pb.KeyRequest]) (*connect.Response[pb.InputResponse], error) {
	return h.deskInput(ctx, req.Msg.Target, &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Key{Key: req.Msg}})
}

func (h *Hub) Run(ctx context.Context, req *connect.Request[pb.RunRequest]) (*connect.Response[pb.RunResponse], error) {
	res, err := h.deskDo(ctx, req.Msg.Target, true, &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Run{Run: req.Msg}})
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(res.GetRun()), nil
}
