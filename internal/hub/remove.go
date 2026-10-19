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
)

const (
	defaultDrainWait = time.Hour
	drainPoll        = 2 * time.Second
	uninstallTimeout = 30 * time.Second
)

// startDrain marks a node as being removed, so no new job or lease goes to
// it. It reports false when a removal is already under way.
func (m *NodeMonitor) startDrain(name string) bool {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.draining[name] {
		return false
	}
	m.draining[name] = true
	return true
}

func (m *NodeMonitor) endDrain(name string) {
	m.mu.Lock()
	delete(m.draining, name)
	m.mu.Unlock()
}

// activeOn lists the unfinished jobs placed on a node. It waits out a
// submission that is placing a job right now, so none slips past a drain.
func (m *JobManager) activeOn(name string) []string {
	m.submitMu.Lock()
	defer m.submitMu.Unlock()
	m.mu.Lock()
	defer m.mu.Unlock()
	var ids []string
	for id, e := range m.jobs {
		if e.node == name && !terminal(e.state()) {
			ids = append(ids, id)
		}
	}
	return ids
}

// RemoveNode drains, optionally uninstalls, revokes and forgets a node. The
// interceptor does not see a stream's request, so it is audited here.
func (h *Hub) RemoveNode(ctx context.Context, req *connect.Request[pb.RemoveNodeRequest], stream *connect.ServerStream[pb.RemoveNodeProgress]) (err error) {
	defer func() {
		code := ""
		if err != nil {
			code = connect.CodeOf(err).String()
		}
		h.authz.Record(ctx, auth.PrincipalFrom(ctx), req.Spec().Procedure, req.Msg.Name, err, code)
	}()
	name := strings.ToLower(strings.TrimSpace(req.Msg.Name))
	if name == h.nodes.self.Name {
		return connect.NewError(connect.CodeFailedPrecondition, errors.New("the hub's own node cannot be removed"))
	}
	info, err := h.nodes.inv.Get(name)
	if err != nil {
		return nodeErr(err)
	}
	if !h.nodes.startDrain(name) {
		return connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("%s is already being removed", name))
	}
	defer h.nodes.endDrain(name)
	send := func(phase pb.RemovePhase, running int, msg string) error {
		return stream.Send(&pb.RemoveNodeProgress{Phase: phase, RunningJobs: uint32(running), Message: msg})
	}

	if err := h.drainJobs(ctx, name, req.Msg, send); err != nil {
		return err
	}
	if req.Msg.Uninstall {
		msg, err := h.uninstallAgent(ctx, info)
		if err != nil {
			return err
		}
		if err := send(pb.RemovePhase_REMOVE_PHASE_UNINSTALLING, 0, msg); err != nil {
			return err
		}
	}
	msg, err := h.revokeNode(ctx, info)
	if err != nil {
		return err
	}
	if err := send(pb.RemovePhase_REMOVE_PHASE_REVOKING, 0, msg); err != nil {
		return err
	}
	released := h.releaseNodeLeases(name)
	if err := h.nodes.inv.Remove(name); err != nil {
		return nodeErr(err)
	}
	h.emit("node.removed", name, nil)
	done := "removed " + name + " from the inventory"
	if released > 0 {
		done += fmt.Sprintf("; released %d lease(s) on it", released)
	}
	return send(pb.RemovePhase_REMOVE_PHASE_REMOVED, 0, done)
}

func (h *Hub) drainJobs(ctx context.Context, name string, req *pb.RemoveNodeRequest, send func(pb.RemovePhase, int, string) error) error {
	if h.jobs == nil {
		return send(pb.RemovePhase_REMOVE_PHASE_DRAINING, 0, "no new jobs or leases; no jobs running")
	}
	ids := h.jobs.activeOn(name)
	switch {
	case len(ids) == 0:
		return send(pb.RemovePhase_REMOVE_PHASE_DRAINING, 0, "no new jobs or leases; no jobs running")
	case req.Jobs == pb.RemoveJobs_REMOVE_JOBS_UNSPECIFIED:
		return connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("%d job(s) running on %s (%s): wait for them or cancel them", len(ids), name, strings.Join(ids, ", ")))
	case req.Jobs == pb.RemoveJobs_REMOVE_JOBS_CANCEL:
		if err := send(pb.RemovePhase_REMOVE_PHASE_DRAINING, len(ids), "no new jobs or leases; cancelling "+strings.Join(ids, ", ")); err != nil {
			return err
		}
		for _, id := range ids {
			if _, err := h.jobs.Cancel(ctx, id); err != nil {
				return fmt.Errorf("cancel job %s: %w", id, err)
			}
		}
	}
	wait := defaultDrainWait
	if req.WaitTimeout != nil && req.WaitTimeout.AsDuration() > 0 {
		wait = req.WaitTimeout.AsDuration()
	}
	deadline := time.Now().Add(wait)
	last := -1
	for {
		n := len(h.jobs.activeOn(name))
		if n == 0 {
			return send(pb.RemovePhase_REMOVE_PHASE_DRAINING, 0, "no jobs running")
		}
		if n != last {
			if err := send(pb.RemovePhase_REMOVE_PHASE_DRAINING, n, fmt.Sprintf("waiting for %d job(s) to finish", n)); err != nil {
				return err
			}
			last = n
		}
		if time.Now().After(deadline) {
			return connect.NewError(connect.CodeDeadlineExceeded, fmt.Errorf("%d job(s) still running on %s after %s: cancel them or wait longer", n, name, wait))
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(drainPoll):
		}
	}
}

// uninstallAgent asks a reachable agent to unregister its startup and stop.
func (h *Hub) uninstallAgent(ctx context.Context, info *pb.NodeInfo) (string, error) {
	st, err := h.nodes.Status(info.Name)
	if err != nil {
		return "", nodeErr(err)
	}
	if st.Freshness != pb.NodeFreshness_NODE_FRESHNESS_FRESH {
		return "not uninstalled: the node is not reachable now; run `protectorate node uninstall` on the machine", nil
	}
	ctx, cancel := context.WithTimeout(ctx, uninstallTimeout)
	defer cancel()
	res, err := h.nodes.agentClient(info).Uninstall(ctx, connect.NewRequest(&pb.UninstallRequest{}))
	if connect.CodeOf(err) == connect.CodeUnimplemented {
		return "not uninstalled: the agent is too old to uninstall itself; run `protectorate node uninstall` on the machine", nil
	}
	if err != nil {
		return "", fmt.Errorf("uninstall on %s: %w", info.Name, err)
	}
	msg := "the agent unregisters its startup and stops: " + strings.Join(res.Msg.Actions, "; ")
	if len(res.Msg.Left) > 0 {
		msg += ". Nothing is deleted; left on the machine: " + strings.Join(res.Msg.Left, ", ")
	}
	return msg, nil
}

// revokeNode expires a pending invite key and deletes the tailnet
// enrollment, so the machine cannot come back without a new invite.
func (h *Hub) revokeNode(ctx context.Context, info *pb.NodeInfo) (string, error) {
	if h.tnAdmin == nil {
		return "no tailnet on this hub: nothing to revoke", nil
	}
	var did []string
	if info.Invited && info.InviteKeyId != 0 {
		if err := h.tnAdmin.ExpireKey(ctx, info.InviteKeyId); err != nil {
			return "", err
		}
		did = append(did, "expired the invite key")
	}
	revoked, err := h.tnAdmin.Revoke(ctx, info.Name)
	switch {
	case err != nil:
		return "", err
	case revoked:
		did = append(did, "revoked the tailnet enrollment")
	default:
		did = append(did, "not enrolled in the tailnet")
	}
	return strings.Join(did, "; "), nil
}

// releaseNodeLeases ends the leases and queued tickets on a node and its
// desktop; their holders get the usual lease events.
func (h *Hub) releaseNodeLeases(name string) int {
	if h.leases == nil {
		return 0
	}
	n := 0
	for _, l := range h.leases.List() {
		if l.Node != name && l.Resource != name && l.Resource != deskResource(name) && !strings.HasPrefix(l.Resource, name+":") {
			continue
		}
		if h.leases.Release(l.Id) == nil {
			n++
		}
	}
	return n
}
