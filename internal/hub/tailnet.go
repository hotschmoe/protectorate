package hub

import (
	"context"
	"errors"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

// SetTailnetStatus installs the source for TailnetService.GetStatus. Without
// one the RPC answers FAILED_PRECONDITION. Call before Handler or Serve.
func (h *Hub) SetTailnetStatus(f func(context.Context) (*pb.TailnetServiceGetStatusResponse, error)) {
	h.tailnetStatus = f
}

type tailnetService struct{ h *Hub }

func (s tailnetService) GetStatus(ctx context.Context, _ *connect.Request[pb.TailnetServiceGetStatusRequest]) (*connect.Response[pb.TailnetServiceGetStatusResponse], error) {
	if s.h.tailnetStatus == nil {
		return nil, connect.NewError(connect.CodeFailedPrecondition, errors.New("tailnet is not enabled on this hub (run `protectorate hub tailnet init`)"))
	}
	res, err := s.h.tailnetStatus(ctx)
	if err != nil {
		return nil, connect.NewError(connect.CodeUnavailable, err)
	}
	return connect.NewResponse(res), nil
}
