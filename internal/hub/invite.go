package hub

import (
	"cmp"
	"context"
	"errors"
	"fmt"
	"log"
	"regexp"
	"slices"
	"strings"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

// TailnetAdmin is the part of the tailnet's administration the hub uses to
// enroll and remove nodes. *tailnet.Admin implements it; the hub does not
// import the tailnet itself.
type TailnetAdmin interface {
	// ControlURL is where machines enroll.
	ControlURL() string
	// InviteNode issues a single-use pre-auth key for a node called name.
	InviteNode(ctx context.Context, name, tier string, pools []string, ttl time.Duration) (key string, id uint64, err error)
	// ExpireKey makes an unused pre-auth key unusable.
	ExpireKey(ctx context.Context, id uint64) error
	// Revoke expires and deletes an enrolled machine; false means none has
	// that name.
	Revoke(ctx context.Context, name string) (bool, error)
	// SetTier replaces the tier tag of an enrolled machine; false means none
	// has that name.
	SetTier(ctx context.Context, name, tier string) (bool, error)
}

// SetTailnetAdmin enables InviteNode, the install scripts and the tailnet
// part of RemoveNode. Call before Handler or Serve.
func (h *Hub) SetTailnetAdmin(a TailnetAdmin) { h.tnAdmin = a }

const (
	defaultInviteTTL = time.Hour
	maxInviteTTL     = 24 * time.Hour
	defaultSetupPoll = time.Second
)

var (
	errNoTailnet = errors.New("this hub has no tailnet: run `protectorate hub tailnet init` and restart the hub")
	// inviteNameRe is a tailnet machine name (a DNS label) that is also an
	// inventory name, so the node is reached under the name it was invited as.
	inviteNameRe = regexp.MustCompile(`^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$`)
)

func validInviteName(name string) bool { return inviteNameRe.MatchString(name) }

func (h *Hub) InviteNode(ctx context.Context, req *connect.Request[pb.InviteNodeRequest]) (*connect.Response[pb.InviteNodeResponse], error) {
	m := req.Msg
	name := m.Name
	if !validInviteName(name) {
		return nil, connect.NewError(connect.CodeInvalidArgument, fmt.Errorf("name %q must be a lowercase DNS label: a-z, 0-9 and '-', at most 63 characters, not starting or ending with '-'", name))
	}
	if h.tnAdmin == nil {
		return nil, connect.NewError(connect.CodeFailedPrecondition, errNoTailnet)
	}
	tier := cmp.Or(m.Tier, TierShared)
	if !ValidTier(tier) {
		return nil, connect.NewError(connect.CodeInvalidArgument, fmt.Errorf("tier must be %s or %s", TierShared, TierCare))
	}
	ttl := defaultInviteTTL
	if m.Ttl != nil {
		ttl = m.Ttl.AsDuration()
	}
	if ttl <= 0 || ttl > maxInviteTTL {
		return nil, connect.NewError(connect.CodeInvalidArgument, fmt.Errorf("ttl must be positive and at most %s", maxInviteTTL))
	}
	base, err := h.installURL(ctx, m.InstallUrl)
	if err != nil {
		return nil, connect.NewError(connect.CodeInvalidArgument, err)
	}
	if name == h.nodes.self.Name {
		return nil, connect.NewError(connect.CodeAlreadyExists, fmt.Errorf("%s is the hub's own node", name))
	}
	prev, err := h.nodes.inv.Get(name)
	switch {
	case err == nil && !prev.Invited:
		return nil, connect.NewError(connect.CodeAlreadyExists, fmt.Errorf("%s is already in the inventory: remove it first", name))
	case err != nil && !errors.Is(err, ErrNodeNotFound):
		return nil, nodeErr(err)
	}

	var invTags []string
	for _, p := range m.Pools {
		invTags = append(invTags, "pool-"+p)
	}
	key, keyID, err := h.tnAdmin.InviteNode(ctx, name, tier, m.Pools, ttl)
	if err != nil {
		return nil, connect.NewError(connect.CodeFailedPrecondition, err)
	}
	expiresAt := time.Now().Add(ttl)
	undo := func() {
		if err := h.tnAdmin.ExpireKey(context.WithoutCancel(ctx), keyID); err != nil {
			log.Printf("invite %s: expire unused key %d: %v", name, keyID, err)
		}
	}
	cmds, err := InstallCommands(base, key, name)
	if err != nil {
		undo()
		return nil, connect.NewError(connect.CodeInternal, err)
	}
	info, err := h.nodes.inv.Upsert(&pb.NodeInfo{Name: name, Address: name, Tier: tier, Tags: invTags, Invited: true, InviteKeyId: keyID})
	if err != nil {
		undo()
		return nil, nodeErr(err)
	}
	if prev != nil && prev.InviteKeyId != 0 && prev.InviteKeyId != keyID {
		if err := h.tnAdmin.ExpireKey(ctx, prev.InviteKeyId); err != nil {
			log.Printf("invite %s: expire the previous key %d: %v", name, prev.InviteKeyId, err)
		}
	}
	h.emit("node.invited", name, nil)
	return connect.NewResponse(&pb.InviteNodeResponse{
		Node: info, ControlUrl: h.tnAdmin.ControlURL(), Key: key, ExpiresAt: timestamppb.New(expiresAt),
		InstallUrl: base, Commands: cmds,
	}), nil
}

// enrolled reports whether a machine joined the tailnet as name or with the
// invite key keyID.
func (m *NodeMonitor) enrolled(ctx context.Context, name string, keyID uint64) bool {
	if m.opts.Peers == nil {
		return false
	}
	peers, err := m.opts.Peers(ctx)
	if err != nil {
		return false
	}
	return slices.ContainsFunc(peers, func(p Peer) bool { return p.Name == name || (keyID != 0 && p.KeyID == keyID) })
}

// setupChecks judges a node that answered the hub: problems keep it from
// READY, pending means a check has no data yet.
func setupChecks(st *pb.NodeStatus, hubVersion string, deskChecked bool) (problems, notes []string, pending string) {
	m := st.Metrics
	if st.Freshness != pb.NodeFreshness_NODE_FRESHNESS_FRESH {
		problems = append(problems, "the hub cannot reach the agent now: "+cmp.Or(st.Error, "no recent poll"))
	}
	if v := m.GetAgentVersion(); v != "" && hubVersion != "" && v != hubVersion {
		problems = append(problems, fmt.Sprintf("the agent runs %s and the hub %s: update the node", v, hubVersion))
	}
	if strings.EqualFold(m.GetOs(), "windows") && deskChecked {
		switch d := st.Desk; {
		case d == nil:
			pending = "waiting for the desktop check"
		case d.Reason == pb.DeskReason_DESK_REASON_NOT_INTERACTIVE:
			problems = append(problems, "the agent is not in an interactive desktop session, so desk commands cannot run: start it with the logon task `node install` registers, not as a service or over SSH ("+d.Detail+")")
		case d.Reason == pb.DeskReason_DESK_REASON_UNKNOWN:
			notes = append(notes, "desktop state unknown: "+d.Detail)
		}
	}
	notes = append(notes, m.GetWarnings()...)
	for _, a := range st.Alerts {
		if a.Kind != pb.AlertKind_ALERT_KIND_UNREACHABLE {
			notes = append(notes, a.Message)
		}
	}
	return problems, notes, pending
}

// setupProgress works out where a node is between its invite and ready.
func (h *Hub) setupProgress(ctx context.Context, name string, keyID uint64) (*pb.NodeSetupProgress, error) {
	st, err := h.nodes.Status(name)
	if err != nil {
		return nil, err
	}
	deskChecked := h.deskPol != nil
	if deskChecked && st.Metrics != nil && strings.EqualFold(st.Metrics.Os, "windows") {
		if d, _ := h.cachedDesk(name); d == nil {
			h.refreshDesk(ctx, name, false)
		}
	}
	st = h.withDesk(st)
	p := &pb.NodeSetupProgress{Status: st}
	switch {
	case st.Node.Invited:
		for _, a := range st.Alerts {
			p.Problems = append(p.Problems, a.Message)
		}
		p.Stage, p.Message = pb.NodeSetupStage_NODE_SETUP_STAGE_INVITED, "waiting for the machine to run the installer"
		if h.nodes.enrolled(ctx, name, keyID) {
			p.Stage, p.Message = pb.NodeSetupStage_NODE_SETUP_STAGE_JOINED, "enrolled in the tailnet; waiting for the hub to reach the agent"
		}
		if len(p.Problems) > 0 {
			p.Stage = pb.NodeSetupStage_NODE_SETUP_STAGE_ATTENTION
		}
	case st.Metrics == nil:
		p.Stage, p.Message = pb.NodeSetupStage_NODE_SETUP_STAGE_JOINED, cmp.Or(st.Error, "waiting for the hub to reach the agent")
	default:
		var pending string
		p.Problems, p.Notes, pending = setupChecks(st, h.Version, deskChecked)
		switch {
		case len(p.Problems) > 0:
			p.Stage, p.Message = pb.NodeSetupStage_NODE_SETUP_STAGE_ATTENTION, "the node answers but needs attention"
		case pending != "":
			p.Stage, p.Message = pb.NodeSetupStage_NODE_SETUP_STAGE_METRICS, pending
		default:
			p.Stage, p.Message = pb.NodeSetupStage_NODE_SETUP_STAGE_READY, fmt.Sprintf("%s/%s, agent %s", st.Metrics.Os, st.Metrics.Arch, st.Metrics.AgentVersion)
		}
	}
	return p, nil
}

func sameSetup(a, b *pb.NodeSetupProgress) bool {
	return a != nil && a.Stage == b.Stage && a.Message == b.Message &&
		slices.Equal(a.Problems, b.Problems) && slices.Equal(a.Notes, b.Notes)
}

func (h *Hub) WatchNodeSetup(ctx context.Context, req *connect.Request[pb.WatchNodeSetupRequest], stream *connect.ServerStream[pb.NodeSetupProgress]) error {
	info, err := h.nodes.info(req.Msg.Name)
	if err != nil {
		return nodeErr(err)
	}
	name, keyID := info.Name, info.InviteKeyId
	poll := cmp.Or(h.setupPoll, defaultSetupPoll)
	var last *pb.NodeSetupProgress
	var kicked time.Time
	for {
		p, err := h.setupProgress(ctx, name, keyID)
		if errors.Is(err, ErrNodeNotFound) {
			return connect.NewError(connect.CodeNotFound, fmt.Errorf("%s was removed", name))
		}
		if err != nil {
			return nodeErr(err)
		}
		if !sameSetup(last, p) {
			if err := stream.Send(p); err != nil {
				return err
			}
			last = p
		}
		if p.Stage == pb.NodeSetupStage_NODE_SETUP_STAGE_READY {
			return nil
		}
		if p.Stage != pb.NodeSetupStage_NODE_SETUP_STAGE_INVITED && time.Since(kicked) >= 3*poll {
			h.nodes.kick(proto.Clone(p.Status.Node).(*pb.NodeInfo))
			kicked = time.Now()
		}
		select {
		case <-ctx.Done():
			return nil
		case <-time.After(poll):
		}
	}
}
