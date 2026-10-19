package hub

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/node"
)

const (
	updateStage          = "updates/protectorate-next"
	defaultUpdateTimeout = 3 * time.Minute
	defaultUpdatePoll    = 2 * time.Second
	probeTimeout         = 5 * time.Second
)

// BinaryName is the file `make cross` writes for a platform.
func BinaryName(goos, arch string) string {
	n := "protectorate-" + goos + "-" + arch
	if goos == "windows" {
		n += ".exe"
	}
	return n
}

// pickBinary chooses what to push to a node: the hub's own executable for the
// hub's platform, otherwise the build in dir.
func pickBinary(dir, goos, arch, selfOS, selfArch, self string) string {
	if goos == selfOS && arch == selfArch && self != "" {
		return self
	}
	return filepath.Join(dir, BinaryName(goos, arch))
}

// NeedsUpdate reports whether a node should be updated and why not if not.
func NeedsUpdate(nodeVersion, hubVersion string, force bool) (bool, string) {
	switch {
	case nodeVersion == "":
		return true, ""
	case nodeVersion == hubVersion && !force:
		return false, "already at " + hubVersion
	}
	return true, ""
}

// updateStep judges one probe of a node after its binary was swapped. down
// means the probe failed; sawDown that an earlier probe did, so the node has
// restarted since.
func updateStep(want, prev, version string, down, sawDown bool) (pb.UpdatePhase, bool) {
	switch {
	case down:
		return pb.UpdatePhase_UPDATE_PHASE_RESTARTING, false
	case version == want:
		return pb.UpdatePhase_UPDATE_PHASE_CONFIRMED, true
	case version == prev && sawDown:
		return pb.UpdatePhase_UPDATE_PHASE_ROLLED_BACK, true
	}
	return pb.UpdatePhase_UPDATE_PHASE_RESTARTING, false
}

func fileDigest(path string) (size uint64, sum string, err error) {
	fh, err := os.Open(path)
	if err != nil {
		return 0, "", err
	}
	defer fh.Close()
	h := sha256.New()
	n, err := io.Copy(h, fh)
	if err != nil {
		return 0, "", err
	}
	return uint64(n), hex.EncodeToString(h.Sum(nil)), nil
}

// UpdateNode pushes the hub's build to a node and follows it through the
// restart. The interceptor does not see streams' requests, so it is audited
// here.
func (h *Hub) UpdateNode(ctx context.Context, req *connect.Request[pb.UpdateNodeRequest], stream *connect.ServerStream[pb.UpdateNodeProgress]) (err error) {
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
	st, err := h.nodes.Status(req.Msg.Name)
	if err != nil {
		return nodeErr(err)
	}
	name := st.Node.Name
	if st.Node.Local {
		return connect.NewError(connect.CodeFailedPrecondition, errors.New("the hub's own node is the hub process: replace its binary and restart the hub"))
	}
	if st.Freshness != pb.NodeFreshness_NODE_FRESHNESS_FRESH || st.Metrics == nil {
		return connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("node %s is not reachable", name))
	}
	prev := st.Metrics.AgentVersion
	if ok, why := NeedsUpdate(prev, h.Version, req.Msg.Force); !ok {
		return stream.Send(&pb.UpdateNodeProgress{Phase: pb.UpdatePhase_UPDATE_PHASE_SKIPPED, Version: prev, PreviousVersion: prev, Message: why})
	}

	bin := h.binaryPath(st.Node.Os, st.Node.Arch)
	size, sum, err := fileDigest(bin)
	if err != nil {
		return connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("read %s (run `make cross`, or start the hub with --binary-dir): %w", bin, err))
	}
	send := func(p *pb.UpdateNodeProgress) error { return stream.Send(p) }
	if err := send(&pb.UpdateNodeProgress{Phase: pb.UpdatePhase_UPDATE_PHASE_SENDING, TotalBytes: size, Message: filepath.Base(bin)}); err != nil {
		return err
	}
	var sendErr error
	base := &pb.PutFileRequest{Name: name, AllowFamily: req.Msg.AllowFamily, Path: updateStage, Overwrite: true}
	put, _, _, err := node.Push(bin, true, base, func(r *pb.PutFileRequest) (*pb.PutFileResponse, error) { return ops.Put(ctx, r) }, func(sent uint64) {
		if sendErr == nil {
			sendErr = send(&pb.UpdateNodeProgress{Phase: pb.UpdatePhase_UPDATE_PHASE_SENDING, SentBytes: sent, TotalBytes: size})
		}
	})
	if err != nil {
		return err
	}
	if sendErr != nil {
		return sendErr
	}
	if err := send(&pb.UpdateNodeProgress{Phase: pb.UpdatePhase_UPDATE_PHASE_VERIFYING, SentBytes: size, TotalBytes: size}); err != nil {
		return err
	}
	res, err := ops.Apply(ctx, &pb.ApplyUpdateRequest{Name: name, AllowFamily: req.Msg.AllowFamily, Path: put.Path, Size: size, Sha256: sum})
	if err != nil {
		return fmt.Errorf("apply: %w", err)
	}
	swapped := &pb.UpdateNodeProgress{Phase: pb.UpdatePhase_UPDATE_PHASE_SWAPPING, SentBytes: size, TotalBytes: size, Version: res.Version, PreviousVersion: res.PreviousVersion, Restart: res.Restart}
	if err := send(swapped); err != nil {
		return err
	}
	return h.followRestart(ctx, st.Node, swapped, send)
}

// followRestart probes the node directly until it reports the new version or
// comes back on the old one, and sends the final phase.
func (h *Hub) followRestart(ctx context.Context, n *pb.NodeInfo, p *pb.UpdateNodeProgress, send func(*pb.UpdateNodeProgress) error) error {
	timeout, poll := h.UpdateTimeout, h.updatePoll
	if timeout <= 0 {
		timeout = defaultUpdateTimeout
	}
	if poll <= 0 {
		poll = defaultUpdatePoll
	}
	p.Phase = pb.UpdatePhase_UPDATE_PHASE_RESTARTING
	if p.Version == p.PreviousVersion {
		p.Phase, p.Message = pb.UpdatePhase_UPDATE_PHASE_CONFIRMED, "same version as before; not waiting"
		return send(p)
	}
	if err := send(p); err != nil {
		return err
	}
	cl := h.nodes.agentClient(n)
	deadline := time.Now().Add(timeout)
	sawDown := false
	for time.Now().Before(deadline) {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(poll):
		}
		pctx, cancel := context.WithTimeout(ctx, probeTimeout)
		m, err := cl.GetMetrics(pctx, connect.NewRequest(&pb.GetMetricsRequest{}))
		cancel()
		version := ""
		if err == nil {
			version = m.Msg.AgentVersion
		}
		phase, final := updateStep(p.Version, p.PreviousVersion, version, err != nil, sawDown)
		if err != nil {
			sawDown = true
		}
		if final {
			p.Phase = phase
			p.Message = "node reports " + version
			if phase == pb.UpdatePhase_UPDATE_PHASE_ROLLED_BACK {
				p.Message = "node came back on " + version + "; it put the previous binary back"
			}
			return send(p)
		}
	}
	return connect.NewError(connect.CodeDeadlineExceeded, fmt.Errorf("node %s did not report %s within %s; it puts its previous binary back if the hub cannot reach the new one within a few minutes", n.Name, p.Version, timeout))
}
