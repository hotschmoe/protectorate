package hub

import (
	"context"
	"errors"
	"fmt"
	"log"
	"path/filepath"
	"runtime"
	"sync"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/structpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/release"
)

type releaseUpdates struct {
	mu      sync.Mutex
	eventMu sync.Mutex
	last    release.Info
}

func (h *Hub) releaseInfo(ctx context.Context, r release.Info) *pb.ReleaseInfo {
	p := auth.PrincipalFrom(ctx)
	allowed := p != nil && h.authz.AuthorizeProcedure(p, protectoratev1connect.ReleaseServiceUpdateHubProcedure) == nil
	return &pb.ReleaseInfo{Version: r.Version, Channel: r.Channel, NotesUrl: r.NotesURL, CanUpdate: allowed}
}

func (h *Hub) CheckRelease(ctx context.Context, req *connect.Request[pb.CheckReleaseRequest]) (*connect.Response[pb.ReleaseInfo], error) {
	if h.Releases == nil {
		return nil, connect.NewError(connect.CodeFailedPrecondition, errors.New("release source is not configured"))
	}
	r, err := h.Releases.Check(ctx, req.Msg.Channel, req.Msg.Version)
	if err != nil {
		// Capability and the last successful result remain useful while offline.
		r = h.Releases.Available()
		if req.Msg.Channel != "" || req.Msg.Version != "" {
			return nil, connect.NewError(connect.CodeUnavailable, err)
		}
		if r.Channel == "" {
			r.Channel = h.Releases.Channel
		}
	} else if req.Msg.Version == "" && (req.Msg.Channel == "" || req.Msg.Channel == h.Releases.Channel) {
		h.publishRelease(r)
	}
	return connect.NewResponse(h.releaseInfo(ctx, r)), nil
}

func (h *Hub) publishRelease(r release.Info) {
	h.releaseUpdates.eventMu.Lock()
	defer h.releaseUpdates.eventMu.Unlock()
	last := h.releaseUpdates.last
	if last.Version == r.Version && last.Channel == r.Channel && last.NotesURL == r.NotesURL {
		return
	}
	h.releaseUpdates.last = r
	payload, _ := structpb.NewStruct(map[string]any{"current_version": h.Version, "available_version": r.Version, "channel": r.Channel, "release_notes_url": r.NotesURL})
	h.emit("release.available", "hub", payload)
}
func (h *Hub) watchReleases(ctx context.Context) {
	tick := time.NewTicker(time.Hour)
	defer tick.Stop()
	for {
		r, err := h.Releases.Check(ctx, "", "")
		if err != nil && ctx.Err() == nil {
			log.Printf("check hub release: %v", err)
		} else if err == nil {
			h.publishRelease(r)
			if r.Version == h.Version && h.Releases.Current().Version != h.Version && h.releaseUpdates.mu.TryLock() {
				previous := h.Releases.Current().Version
				_, err := h.Releases.Fetch(ctx, r, func(string, uint64, uint64) error { return nil })
				if err == nil {
					err = h.Releases.Activate(r.Version)
				}
				if err == nil {
					err = h.Releases.Prune(previous)
				}
				if err != nil && ctx.Err() == nil {
					log.Printf("fetch current hub release: %v", err)
				}
				h.releaseUpdates.mu.Unlock()
			}
		}
		select {
		case <-ctx.Done():
			return
		case <-tick.C:
		}
	}
}

func (h *Hub) UpdateHub(ctx context.Context, req *connect.Request[pb.UpdateHubRequest], stream *connect.ServerStream[pb.UpdateHubProgress]) (err error) {
	defer func() { h.recordStream(ctx, req.Spec().Procedure, "hub", err) }()
	if h.Releases == nil || h.ReplaceHub == nil {
		return connect.NewError(connect.CodeFailedPrecondition, errors.New("hub self-update is not configured"))
	}
	if !h.releaseUpdates.mu.TryLock() {
		return connect.NewError(connect.CodeFailedPrecondition, errors.New("a hub update is already in progress"))
	}
	defer h.releaseUpdates.mu.Unlock()
	r, err := h.Releases.Check(ctx, req.Msg.Channel, req.Msg.Version)
	if err != nil {
		return connect.NewError(connect.CodeUnavailable, fmt.Errorf("resolve release: %w", err))
	}
	auth.Annotate(ctx, "hub", fmt.Sprintf("channel=%s version=%s", r.Channel, r.Version))
	if r.Version == h.Version && h.Releases.Current().Version == r.Version {
		return stream.Send(&pb.UpdateHubProgress{Phase: pb.UpdatePhase_UPDATE_PHASE_SKIPPED, Version: r.Version, PreviousVersion: h.Version, Message: "hub already runs this release"})
	}
	dir, err := h.Releases.Fetch(ctx, r, func(name string, sent, total uint64) error {
		return stream.Send(&pb.UpdateHubProgress{Phase: pb.UpdatePhase_UPDATE_PHASE_SENDING, Message: name, Version: r.Version, PreviousVersion: h.Version, SentBytes: sent, TotalBytes: total})
	})
	if err != nil {
		return connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("fetch verified release: %w", err))
	}
	if err := stream.Send(&pb.UpdateHubProgress{Phase: pb.UpdatePhase_UPDATE_PHASE_VERIFYING, Version: r.Version, PreviousVersion: h.Version, Message: "all fleet assets verified"}); err != nil {
		return err
	}
	if r.Version == h.Version {
		previous := h.Releases.Current().Version
		if err := h.Releases.Activate(r.Version); err != nil {
			return err
		}
		if err := h.Releases.Prune(previous); err != nil {
			return err
		}
		return stream.Send(&pb.UpdateHubProgress{Phase: pb.UpdatePhase_UPDATE_PHASE_CONFIRMED, Version: r.Version, PreviousVersion: h.Version, Message: "fleet assets now match the running hub"})
	}
	restart, err := h.ReplaceHub(ctx, filepath.Join(dir, BinaryName(runtime.GOOS, runtime.GOARCH)), r)
	if err != nil {
		return connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("replace hub: %w", err))
	}
	// Once committed, a disconnected caller must not prevent the restart.
	defer restart()
	return stream.Send(&pb.UpdateHubProgress{Phase: pb.UpdatePhase_UPDATE_PHASE_RESTARTING, Version: r.Version, PreviousVersion: h.Version, Message: "restarting; reconnect to confirm the result"})
}
