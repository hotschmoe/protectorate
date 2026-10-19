package hub

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"log"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/protobuf/types/known/structpb"
	"gopkg.in/yaml.v3"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/fsutil"
	"github.com/hotschmoe/protectorate/internal/mailbox"
	"github.com/hotschmoe/protectorate/internal/node"
)

const (
	// deskWatchEvery is how often the hub asks policy nodes whether the user
	// is back. Nodes enforce the policy themselves, so this only bounds how
	// long a lease outlives the user's return.
	deskWatchEvery = 3 * time.Second
	// deskIdleEvery is the same for Windows nodes without a policy, which are
	// only shown in status.
	deskIdleEvery = 10
	// deskStateMaxAge is how old a cached state may be when a queued lease is
	// granted without asking the node.
	deskStateMaxAge  = 15 * time.Second
	deskStateTimeout = 10 * time.Second
)

type deskPolicyRecord struct {
	Window    string        `yaml:"window"`
	Idle      time.Duration `yaml:"idle"`
	Timezone  string        `yaml:"timezone,omitempty"`
	LockAfter bool          `yaml:"lock_after,omitempty"`
}

func (r deskPolicyRecord) proto() *pb.DeskPolicy {
	return &pb.DeskPolicy{Window: r.Window, Idle: durationpb.New(r.Idle), Timezone: r.Timezone, LockAfter: r.LockAfter}
}

// deskPolicies is the persisted per-node desk policy.
type deskPolicies struct {
	path string
	mu   sync.Mutex
	m    map[string]deskPolicyRecord
}

type deskPolicyFile struct {
	Nodes map[string]deskPolicyRecord `yaml:"nodes"`
}

func openDeskPolicies(path string) (*deskPolicies, error) {
	p := &deskPolicies{path: path, m: map[string]deskPolicyRecord{}}
	raw, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return p, nil
	}
	if err != nil {
		return nil, fmt.Errorf("read desk policies: %w", err)
	}
	var f deskPolicyFile
	dec := yaml.NewDecoder(bytes.NewReader(raw))
	dec.KnownFields(true)
	if err := dec.Decode(&f); err != nil && !errors.Is(err, io.EOF) {
		return nil, fmt.Errorf("parse %s: %w", path, err)
	}
	for name, r := range f.Nodes {
		if _, err := node.PolicyFromProto(r.proto()); err != nil {
			return nil, fmt.Errorf("parse %s: node %s: %w", path, name, err)
		}
		p.m[name] = r
	}
	return p, nil
}

func (p *deskPolicies) get(name string) *pb.DeskPolicy {
	p.mu.Lock()
	defer p.mu.Unlock()
	if r, ok := p.m[name]; ok {
		return r.proto()
	}
	return nil
}

// set stores the policy of a node; nil removes it.
func (p *deskPolicies) set(name string, pol *pb.DeskPolicy) error {
	p.mu.Lock()
	defer p.mu.Unlock()
	prev, had := p.m[name]
	if pol == nil {
		delete(p.m, name)
	} else {
		p.m[name] = deskPolicyRecord{Window: pol.Window, Idle: pol.Idle.AsDuration(), Timezone: pol.Timezone, LockAfter: pol.LockAfter}
	}
	raw, err := yaml.Marshal(deskPolicyFile{Nodes: p.m})
	if err == nil {
		if err = os.MkdirAll(filepath.Dir(p.path), 0o700); err == nil {
			err = fsutil.WriteFile(p.path, raw, 0o600)
		}
	}
	if err != nil {
		if had {
			p.m[name] = prev
		} else {
			delete(p.m, name)
		}
		return connect.NewError(connect.CodeInternal, fmt.Errorf("persist desk policy: %w", err))
	}
	return nil
}

func (p *deskPolicies) names() []string {
	p.mu.Lock()
	defer p.mu.Unlock()
	out := make([]string, 0, len(p.m))
	for n := range p.m {
		out = append(out, n)
	}
	return out
}

type deskEntry struct {
	state *pb.DeskState
	at    time.Time
}

func unknownDesk(detail string) *pb.DeskState {
	return &pb.DeskState{Reason: pb.DeskReason_DESK_REASON_UNKNOWN, Detail: detail}
}

// refreshDesk asks a node for its desk state, sending the policy the hub holds,
// and caches the answer. A node that cannot answer is recorded as unavailable.
func (h *Hub) refreshDesk(ctx context.Context, name string, leaseEnded bool) *pb.DeskState {
	st := h.askDesk(ctx, name, leaseEnded)
	h.deskMu.Lock()
	if h.deskCache == nil {
		h.deskCache = map[string]deskEntry{}
	}
	h.deskCache[name] = deskEntry{state: st, at: time.Now()}
	h.deskMu.Unlock()
	return st
}

func (h *Hub) askDesk(ctx context.Context, name string, leaseEnded bool) *pb.DeskState {
	info, err := h.nodes.info(name)
	switch {
	case err != nil:
		return unknownDesk(err.Error())
	case info.Local:
		return unknownDesk("the hub's own node does not take a desk policy")
	}
	ctx, cancel := context.WithTimeout(ctx, deskStateTimeout)
	defer cancel()
	res, err := h.nodes.agentClient(info).GetDeskState(ctx, connect.NewRequest(&pb.GetDeskStateRequest{Policy: h.deskPol.get(name), LeaseEnded: leaseEnded}))
	switch {
	case connect.CodeOf(err) == connect.CodeUnimplemented:
		return unknownDesk("the node agent does not report desk availability: update it")
	case err != nil:
		return unknownDesk("node did not answer: " + err.Error())
	}
	return res.Msg
}

// cachedDesk returns the last state of a node and how old it is.
func (h *Hub) cachedDesk(name string) (*pb.DeskState, time.Duration) {
	h.deskMu.Lock()
	defer h.deskMu.Unlock()
	e, ok := h.deskCache[name]
	if !ok {
		return nil, 0
	}
	return e.state, time.Since(e.at)
}

func deskRefused(name string, s *pb.DeskState) error {
	return connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("%s is unavailable: %s", deskResource(name), s.Detail))
}

// gateDesk refuses a desk lease while the node's policy says agents may not
// have the desktop. Nodes without a policy are never gated. live asks the node
// now; without it the answer may be a few seconds old, which is all a queued
// lease granted under the lease lock can use.
func (h *Hub) gateDesk(ctx context.Context, name string, live bool) error {
	if h.deskPol == nil || h.deskPol.get(name) == nil {
		return nil
	}
	if live {
		if s := h.refreshDesk(ctx, name, false); !s.Available {
			if s.Reason != pb.DeskReason_DESK_REASON_UNKNOWN {
				h.endDeskLeases(name, s.Detail)
			}
			return deskRefused(name, s)
		}
		return nil
	}
	s, age := h.cachedDesk(name)
	switch {
	case s == nil:
		return deskRefused(name, unknownDesk("availability not reported yet"))
	case age > deskStateMaxAge:
		return deskRefused(name, unknownDesk("availability is stale"))
	case !s.Available:
		return deskRefused(name, s)
	}
	return nil
}

// leaseEvent forwards lease events to subscribers and tells a node whose
// desktop lease just ended, so it can lock the workstation when its policy
// asks. It runs under the lease table's lock.
func (h *Hub) leaseEvent(kind, subject string, payload *structpb.Struct) {
	h.emit(kind, subject, payload)
	if kind != "lease.released" && kind != "lease.expired" {
		return
	}
	res, ok := strings.CutPrefix(payload.GetFields()["resource"].GetStringValue(), "desk:")
	if !ok || h.deskPol == nil || h.deskPol.get(res) == nil {
		return
	}
	go func() {
		ctx, cancel := context.WithTimeout(context.Background(), deskStateTimeout)
		defer cancel()
		h.refreshDesk(ctx, res, true)
	}()
}

// endDeskLeases ends every lease on a node's desktop because the user is back
// and tells each holder.
func (h *Hub) endDeskLeases(name, detail string) {
	for _, l := range h.leases.List() {
		if l.Resource != deskResource(name) || l.State == pb.LeaseState_LEASE_STATE_QUEUED {
			continue
		}
		if err := h.leases.Release(l.Id); err != nil {
			continue
		}
		h.noticeDeskEnded(name, l, detail)
	}
}

func (h *Hub) noticeDeskEnded(name string, l *pb.Lease, detail string) {
	if h.coord == nil || h.sleeves == nil {
		return
	}
	who := l.Holder
	for _, p := range []string{autoHolder, "token:", sleeveTokenPrefix} {
		who = strings.TrimPrefix(who, p)
	}
	if _, err := h.sleeves.Get(who); err != nil {
		return
	}
	body := fmt.Sprintf("Your desktop lease %s on %s was ended by the hub: the user is back at the machine (%s). Stop driving the desktop. The desktop is refused to agents until the user has been idle again inside the availability window; check `protectorate desk policy %s`.",
		l.Id, name, detail, name)
	if _, err := h.coord.send(hubName, sendArgs{To: who, Direct: true, Body: body, Kind: mailbox.Notice, RequestID: "desk-away-" + l.Id}); err != nil {
		log.Printf("desk lease %s: notify %s: %v", l.Id, who, err)
	}
}

// watchDesks keeps desk state fresh and ends a desktop lease the moment its
// node reports the desktop unavailable.
func (h *Hub) watchDesks(ctx context.Context) {
	tick := time.NewTicker(deskWatchEvery)
	defer tick.Stop()
	for n := 0; ; n++ {
		select {
		case <-ctx.Done():
			return
		case <-tick.C:
		}
		h.deskPass(ctx, n%deskIdleEvery == 0)
	}
}

// deskPass refreshes the desk state of the policy nodes (and, with all, of the
// other Windows nodes) and ends the leases of those that are unavailable.
func (h *Hub) deskPass(ctx context.Context, all bool) {
	for _, name := range h.deskTargets(all) {
		s := h.refreshDesk(ctx, name, false)
		if ctx.Err() != nil {
			return
		}
		if !s.Available && s.Reason != pb.DeskReason_DESK_REASON_UNKNOWN && h.deskPol.get(name) != nil {
			h.endDeskLeases(name, s.Detail)
		}
	}
}

func (h *Hub) deskTargets(all bool) []string {
	seen := map[string]bool{}
	var out []string
	for _, name := range h.deskPol.names() {
		seen[name] = true
		out = append(out, name)
	}
	if all {
		for _, n := range h.nodes.inv.List() {
			if !seen[n.Name] && !n.Invited && strings.EqualFold(n.Os, "windows") {
				out = append(out, n.Name)
			}
		}
	}
	return out
}

// deskView is what status shows for a node: its desk state and who holds the
// desktop lease.
func (h *Hub) deskView(name string) (*pb.DeskState, string) {
	s, _ := h.cachedDesk(name)
	holder := ""
	if h.leases != nil {
		if l := h.leases.Blocked(deskResource(name), nil); l != nil {
			holder = l.Holder
		}
	}
	return s, holder
}

func (h *Hub) withDesk(s *pb.NodeStatus) *pb.NodeStatus {
	d, holder := h.deskView(s.Node.Name)
	inputs := h.deskUse.get(strings.ToLower(s.Node.Name))
	if d == nil && holder == "" && len(inputs) == 0 {
		return s
	}
	s = proto.Clone(s).(*pb.NodeStatus)
	s.Desk, s.DeskHolder, s.DeskInputs = d, holder, inputs
	return s
}

func (h *Hub) deskNode(name string, allowFamily bool) (string, error) {
	name = strings.ToLower(strings.TrimSpace(name))
	if !nodeNameRe.MatchString(name) {
		return "", connect.NewError(connect.CodeInvalidArgument, fmt.Errorf("invalid node name %q", name))
	}
	st, err := h.nodes.Status(name)
	switch {
	case err != nil:
		return "", nodeErr(err)
	case st.Node.Local:
		return "", connect.NewError(connect.CodeFailedPrecondition, errors.New("the hub's own node does not take a desk policy"))
	case st.Node.Tier == TierFamily && !allowFamily:
		return "", connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("node %s is tier family: set allow_family to use it", name))
	case h.deskPol == nil:
		return "", connect.NewError(connect.CodeUnimplemented, errors.New("desk policies need the lease store (hub started without jobs)"))
	}
	return name, nil
}

func (h *Hub) deskStatus(ctx context.Context, name string) *pb.DeskPolicyStatus {
	s := h.refreshDesk(ctx, name, false)
	_, holder := h.deskView(name)
	return &pb.DeskPolicyStatus{Policy: h.deskPol.get(name), State: s, Holder: holder, Inputs: h.deskUse.get(name)}
}

func (h *Hub) SetDeskPolicy(ctx context.Context, req *connect.Request[pb.SetDeskPolicyRequest]) (*connect.Response[pb.DeskPolicyStatus], error) {
	name, err := h.deskNode(req.Msg.Name, req.Msg.AllowFamily)
	if err != nil {
		return nil, err
	}
	if _, err := node.PolicyFromProto(req.Msg.Policy); err != nil {
		return nil, connect.NewError(connect.CodeInvalidArgument, err)
	}
	if err := h.deskPol.set(name, req.Msg.Policy); err != nil {
		return nil, err
	}
	st := h.deskStatus(ctx, name)
	if st.Policy != nil && !st.State.Available {
		h.endDeskLeases(name, st.State.Detail)
	}
	return connect.NewResponse(st), nil
}

func (h *Hub) GetDeskPolicy(ctx context.Context, req *connect.Request[pb.GetDeskPolicyRequest]) (*connect.Response[pb.DeskPolicyStatus], error) {
	name, err := h.deskNode(req.Msg.Name, req.Msg.AllowFamily)
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(h.deskStatus(ctx, name)), nil
}

// deskRefusal reports whether err is a node's refusal because the policy no
// longer allows agents, with the reason.
func deskRefusal(err error) bool {
	var ce *connect.Error
	return errors.As(err, &ce) && ce.Meta().Get(node.DeskUnavailableHeader) != ""
}
