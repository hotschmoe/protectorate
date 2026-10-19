package hub

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/node"
)

const deskTimeout = time.Minute

// nodeOps is what the hub asks of a node besides metrics and jobs: file
// transfers and desktop commands, served by the hub's own process for the
// local node and by the node agent for the rest.
type nodeOps interface {
	Put(context.Context, *pb.PutFileRequest) (*pb.PutFileResponse, error)
	Get(context.Context, *pb.GetFileRequest, func(*pb.GetFileResponse) error) error
	Desk(context.Context, *pb.DeskActionRequest) (*pb.DeskActionResponse, error)
	Apply(context.Context, *pb.ApplyUpdateRequest) (*pb.ApplyUpdateResponse, error)
}

type localOps struct {
	files *node.Files
	desk  *node.Desk
}

func (l localOps) Put(_ context.Context, req *pb.PutFileRequest) (*pb.PutFileResponse, error) {
	if l.files == nil {
		return nil, connect.NewError(connect.CodeUnimplemented, errors.New("the hub node does not serve file transfers"))
	}
	return l.files.Put(req)
}

func (l localOps) Get(ctx context.Context, req *pb.GetFileRequest, fn func(*pb.GetFileResponse) error) error {
	if l.files == nil {
		return connect.NewError(connect.CodeUnimplemented, errors.New("the hub node does not serve file transfers"))
	}
	return l.files.Get(ctx, req, fn)
}

func (l localOps) Desk(ctx context.Context, req *pb.DeskActionRequest) (*pb.DeskActionResponse, error) {
	if l.desk == nil {
		return nil, connect.NewError(connect.CodeUnimplemented, errors.New("the hub node does not serve desktop commands"))
	}
	return l.desk.Do(ctx, req)
}

func (l localOps) Apply(context.Context, *pb.ApplyUpdateRequest) (*pb.ApplyUpdateResponse, error) {
	return nil, connect.NewError(connect.CodeFailedPrecondition, errors.New("the hub's own node is the hub process: replace its binary and restart the hub"))
}

type remoteOps struct {
	c protectoratev1connect.NodeAgentServiceClient
}

func (r remoteOps) Put(ctx context.Context, req *pb.PutFileRequest) (*pb.PutFileResponse, error) {
	ctx, cancel := context.WithTimeout(ctx, rpcTimeout)
	defer cancel()
	res, err := r.c.PutFile(ctx, connect.NewRequest(req))
	if err != nil {
		return nil, err
	}
	return res.Msg, nil
}

func (r remoteOps) Get(ctx context.Context, req *pb.GetFileRequest, fn func(*pb.GetFileResponse) error) error {
	stream, err := r.c.GetFile(ctx, connect.NewRequest(req))
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

func (r remoteOps) Desk(ctx context.Context, req *pb.DeskActionRequest) (*pb.DeskActionResponse, error) {
	ctx, cancel := context.WithTimeout(ctx, deskTimeout)
	defer cancel()
	res, err := r.c.DeskAction(ctx, connect.NewRequest(req))
	if err != nil {
		return nil, err
	}
	return res.Msg, nil
}

func (r remoteOps) Apply(ctx context.Context, req *pb.ApplyUpdateRequest) (*pb.ApplyUpdateResponse, error) {
	ctx, cancel := context.WithTimeout(ctx, rpcTimeout)
	defer cancel()
	res, err := r.c.ApplyUpdate(ctx, connect.NewRequest(req))
	if err != nil {
		return nil, err
	}
	return res.Msg, nil
}

// EnableNodeOps lets the hub's own node serve file transfers and desktop
// commands; either may be nil. Call before Handler or Serve.
func (h *Hub) EnableNodeOps(files *node.Files, d *node.Desk) {
	h.local = localOps{files: files, desk: d}
}

// opsFor resolves a named node that can be used now.
func (h *Hub) opsFor(name string, allowFamily bool) (nodeOps, error) {
	name = strings.ToLower(strings.TrimSpace(name))
	if !nodeNameRe.MatchString(name) {
		return nil, connect.NewError(connect.CodeInvalidArgument, fmt.Errorf("invalid node name %q", name))
	}
	if err := h.nodes.namedNode(name, nil, allowFamily); err != nil {
		return nil, err
	}
	st, err := h.nodes.Status(name)
	if err != nil {
		return nil, nodeErr(err)
	}
	if st.Node.Local {
		return h.local, nil
	}
	return remoteOps{h.nodes.agentClient(st.Node)}, nil
}

func (h *Hub) PutFile(ctx context.Context, req *connect.Request[pb.PutFileRequest]) (*connect.Response[pb.PutFileResponse], error) {
	ops, err := h.opsFor(req.Msg.Name, req.Msg.AllowFamily)
	if err != nil {
		return nil, err
	}
	res, err := ops.Put(ctx, req.Msg)
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(res), nil
}

// GetFile reads files off a node, so it is audited here: the interceptor
// skips streams and does not see their request.
func (h *Hub) GetFile(ctx context.Context, req *connect.Request[pb.GetFileRequest], stream *connect.ServerStream[pb.GetFileResponse]) (err error) {
	defer func() {
		code := ""
		if err != nil {
			code = connect.CodeOf(err).String()
		}
		h.authz.Record(ctx, auth.PrincipalFrom(ctx), req.Spec().Procedure, req.Msg.Name, err, code)
	}()
	ops, err := h.opsFor(req.Msg.Name, req.Msg.AllowFamily)
	if err != nil {
		return err
	}
	return ops.Get(ctx, req.Msg, stream.Send)
}

func (h *Hub) ApplyUpdate(ctx context.Context, req *connect.Request[pb.ApplyUpdateRequest]) (*connect.Response[pb.ApplyUpdateResponse], error) {
	ops, err := h.opsFor(req.Msg.Name, req.Msg.AllowFamily)
	if err != nil {
		return nil, err
	}
	res, err := ops.Apply(ctx, req.Msg)
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(res), nil
}
