package hub

import (
	"context"
	"errors"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/auth"
)

func (h *Hub) CreateToken(_ context.Context, req *connect.Request[pb.CreateTokenRequest]) (*connect.Response[pb.CreateTokenResponse], error) {
	m := req.Msg
	if m.Ttl != nil && m.Ttl.CheckValid() != nil {
		return nil, connect.NewError(connect.CodeInvalidArgument, errors.New("invalid ttl"))
	}
	t, secret, err := h.authz.Store().Create(m.Name, m.Scopes, m.Ttl.AsDuration(), time.Now())
	if err != nil {
		return nil, connect.NewError(connect.CodeInvalidArgument, err)
	}
	return connect.NewResponse(&pb.CreateTokenResponse{Token: toProtoToken(t), Secret: secret}), nil
}

func (h *Hub) ListTokens(context.Context, *connect.Request[pb.ListTokensRequest]) (*connect.Response[pb.ListTokensResponse], error) {
	toks := h.authz.Store().List()
	res := &pb.ListTokensResponse{Tokens: make([]*pb.TokenInfo, len(toks))}
	for i, t := range toks {
		res.Tokens[i] = toProtoToken(t)
	}
	return connect.NewResponse(res), nil
}

func (h *Hub) RevokeToken(_ context.Context, req *connect.Request[pb.RevokeTokenRequest]) (*connect.Response[pb.RevokeTokenResponse], error) {
	t, err := h.authz.Store().Revoke(req.Msg.Id, time.Now())
	switch {
	case errors.Is(err, auth.ErrUnknownToken):
		return nil, connect.NewError(connect.CodeNotFound, errors.New("no such token"))
	case err != nil:
		return nil, connect.NewError(connect.CodeInternal, err)
	}
	return connect.NewResponse(&pb.RevokeTokenResponse{Token: toProtoToken(t)}), nil
}

func toProtoToken(t auth.Token) *pb.TokenInfo {
	ts := func(v *time.Time) *timestamppb.Timestamp {
		if v == nil {
			return nil
		}
		return timestamppb.New(*v)
	}
	return &pb.TokenInfo{
		Id: t.ID, Name: t.Name, Scopes: t.Scopes,
		CreatedAt: timestamppb.New(t.Created), ExpiresAt: ts(t.Expires),
		LastUsedAt: ts(t.LastUsed), RevokedAt: ts(t.RevokedAt),
	}
}
