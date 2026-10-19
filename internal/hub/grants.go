package hub

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

const defaultGrantTTL = 4 * time.Hour

type grantService struct{ h *Hub }

// Admin mints an admin grant. The owner may name any node or glob; the
// orchestrator one shared node by name, and only with OrchestratorGrants.
// Everyone else is refused whatever their scopes.
func (g grantService) Admin(ctx context.Context, req *connect.Request[pb.GrantServiceAdminRequest]) (*connect.Response[pb.GrantServiceAdminResponse], error) {
	h, m := g.h, req.Msg
	node := strings.ToLower(strings.TrimSpace(m.Node))
	ttl := defaultGrantTTL
	if m.Ttl != nil {
		ttl = m.Ttl.AsDuration()
	}
	p := auth.PrincipalFrom(ctx)
	minter := auth.MintedByOwner
	switch {
	case p != nil && p.Owner:
	case p != nil && p.Name == sleeve.OrchestratorTokenName:
		if err := h.orchestratorMayGrant(node); err != nil {
			return nil, err
		}
		minter = p.Name
	default:
		return nil, connect.NewError(connect.CodePermissionDenied, errors.New("only the owner mints admin grants (and the orchestrator, for shared nodes, when the hub allows it)"))
	}
	auth.Annotate(ctx, node, "task: "+strings.TrimSpace(m.Task))
	t, secret, err := h.authz.Store().CreateGrant(auth.Grant{Node: node, Task: m.Task, MintedBy: minter, TTL: ttl}, time.Now())
	if err != nil {
		return nil, connect.NewError(connect.CodeInvalidArgument, err)
	}
	h.emit("grant.minted", t.ID, nil)
	return connect.NewResponse(&pb.GrantServiceAdminResponse{Token: toProtoToken(t), Secret: secret}), nil
}

func (h *Hub) orchestratorMayGrant(node string) error {
	if !h.OrchestratorGrants {
		return connect.NewError(connect.CodePermissionDenied, errors.New("the hub does not let the orchestrator mint admin grants (hub serve --orchestrator-admin-grants): ask the owner, protectorate msg send owner"))
	}
	if !nodeNameRe.MatchString(node) {
		return connect.NewError(connect.CodeInvalidArgument, fmt.Errorf("the orchestrator grants one node by name, not %q", node))
	}
	if h.nodes == nil {
		return connect.NewError(connect.CodeFailedPrecondition, errors.New("this hub has no node inventory"))
	}
	info, err := h.nodes.info(node)
	if err != nil {
		return nodeErr(err)
	}
	if info.Tier != TierShared {
		return connect.NewError(connect.CodePermissionDenied, fmt.Errorf("node %s is tier %s: only the owner grants admin on it", node, info.Tier))
	}
	return nil
}
