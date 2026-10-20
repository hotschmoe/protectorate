package hub

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"net/http"
	"os"
	"sort"
	"strings"
	"sync"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/fsutil"
)

// deskResource is the lease resource that guards a node's desktop.
func deskResource(node string) string { return "desk:" + strings.ToLower(node) }

const (
	// autoDeskTTL is how long a lease the hub takes for a caller lasts; each
	// further desk command from that caller renews it.
	autoDeskTTL = 2 * time.Minute
	// autoHolder prefixes the holder of a lease the hub took on its own.
	autoHolder = "auto:"
	// DeskLeaseHeader and DeskLeaseExpiresHeader carry the auto lease's id and
	// expiry back on the response of a desk command.
	DeskLeaseHeader        = "Protectorate-Desk-Lease"
	DeskLeaseExpiresHeader = "Protectorate-Desk-Lease-Expires"
)

// deskUse counts desk input commands per node and caller for the current
// local day. With a path the counts are kept on disk and survive a restart.
type deskUse struct {
	mu     sync.Mutex
	path   string
	day    string
	counts map[string]map[string]int32
}

type deskUseFile struct {
	Day    string                      `json:"day"`
	Counts map[string]map[string]int32 `json:"counts"`
}

// open adopts the counts saved at path if they are from today, and keeps
// saving there.
func (u *deskUse) open(path string) error {
	u.mu.Lock()
	defer u.mu.Unlock()
	u.path = path
	raw, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return nil
	}
	if err != nil {
		return fmt.Errorf("read desk use: %w", err)
	}
	var f deskUseFile
	if err := json.Unmarshal(raw, &f); err != nil {
		return fmt.Errorf("parse %s: %w", path, err)
	}
	if f.Day == time.Now().Format(time.DateOnly) && f.Counts != nil {
		u.day, u.counts = f.Day, f.Counts
	}
	return nil
}

func (u *deskUse) add(node, caller string) {
	u.mu.Lock()
	defer u.mu.Unlock()
	u.rollLocked()
	if u.counts[node] == nil {
		u.counts[node] = map[string]int32{}
	}
	u.counts[node][caller]++
	if u.path == "" {
		return
	}
	raw, err := json.Marshal(deskUseFile{Day: u.day, Counts: u.counts})
	if err == nil {
		err = fsutil.WriteFile(u.path, raw, 0o600)
	}
	if err != nil {
		log.Printf("desk: save input counts: %v", err)
	}
}

func (u *deskUse) rollLocked() {
	if day := time.Now().Format(time.DateOnly); day != u.day || u.counts == nil {
		u.day, u.counts = day, map[string]map[string]int32{}
	}
}

func (u *deskUse) get(node string) []*pb.DeskUse {
	u.mu.Lock()
	defer u.mu.Unlock()
	u.rollLocked()
	var out []*pb.DeskUse
	for c, n := range u.counts[node] {
		out = append(out, &pb.DeskUse{Caller: c, Count: n})
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Caller < out[j].Caller })
	return out
}

// deskActor names the caller for the usage count.
func deskActor(ctx context.Context) string {
	if p := auth.PrincipalFrom(ctx); p != nil {
		return p.Actor()
	}
	return "anonymous"
}

// deskCaller names the authenticated caller for auto leases.
func deskCaller(ctx context.Context) string {
	if p := auth.PrincipalFrom(ctx); p != nil {
		return autoHolder + p.Actor()
	}
	return autoHolder + "anonymous"
}

// deskLease is the lease a mutating desktop command ran under when the hub
// took or renewed it; fresh marks a lease created by this very command.
type deskLease struct {
	lease *pb.Lease
	fresh bool
}

// admitDesk admits a mutating desktop command. With a lease id, that lease
// must be live and on exactly this desktop; holding the id is the proof.
// Without one, the hub takes a short lease for the caller when the desktop is
// free, renews the caller's own, and refuses when someone else holds it.
func (h *Hub) admitDesk(ctx context.Context, node, leaseID string, allowFamily bool) (*deskLease, error) {
	want := deskResource(node)
	if leaseID == "" {
		if err := h.gateDesk(ctx, node, true); err != nil {
			return nil, err
		}
	}
	if leaseID != "" {
		l, err := h.leases.Get(leaseID)
		if err != nil {
			return nil, err
		}
		if l.Resource != want {
			return nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("lease %s is on %s, not %s", leaseID, l.Resource, want))
		}
		if !strings.HasPrefix(l.Holder, autoHolder) {
			return nil, nil
		}
		if l, err = h.leases.Renew(leaseID, 0); err != nil {
			return nil, err
		}
		return &deskLease{lease: l}, nil
	}
	holder := deskCaller(ctx)
	var lastErr error
	for range 2 {
		if cur := h.leases.Blocked(want, nil); cur != nil {
			if cur.Holder != holder {
				return nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("%s is held by %s until %s (expires in %s): wait for it, or ask the holder to release it", want, cur.Holder, cur.ExpiresAt.AsTime().Local().Format(time.RFC3339), time.Until(cur.ExpiresAt.AsTime()).Round(time.Second)))
			}
			l, err := h.leases.Renew(cur.Id, 0)
			if err != nil {
				lastErr = err
				continue
			}
			return &deskLease{lease: l}, nil
		}
		l, err := h.leases.Acquire(holder, autoDeskTTL, func(blocked func(string, *int) *pb.Lease) (string, *int, error) {
			return h.acquireDesk(node, allowFamily, blocked)
		})
		if err == nil {
			return &deskLease{lease: l, fresh: true}, nil
		}
		if connect.CodeOf(err) != connect.CodeFailedPrecondition {
			return nil, err
		}
		lastErr = err
	}
	return nil, lastErr
}

func setDeskLeaseHeaders(h http.Header, dl *deskLease) {
	if dl == nil {
		return
	}
	h.Set(DeskLeaseHeader, dl.lease.Id)
	h.Set(DeskLeaseExpiresHeader, dl.lease.ExpiresAt.AsTime().UTC().Format(time.RFC3339))
}

func (h *Hub) deskDo(ctx context.Context, t *pb.DeskTarget, mutating bool, act *pb.DeskActionRequest) (*pb.DeskActionResponse, *deskLease, error) {
	if t == nil || t.Name == "" {
		return nil, nil, connect.NewError(connect.CodeInvalidArgument, fmt.Errorf("target.name is required"))
	}
	ops, err := h.opsFor(t.Name, t.AllowFamily)
	if err != nil {
		return nil, nil, err
	}
	var dl *deskLease
	if mutating {
		if dl, err = h.admitDesk(ctx, t.Name, t.LeaseId, t.AllowFamily); err != nil {
			return nil, nil, err
		}
	}
	res, err := ops.Desk(ctx, act)
	if err != nil && deskRefusal(err) {
		h.endDeskLeases(strings.ToLower(t.Name), err.Error())
	}
	if err == nil && mutating {
		h.deskUse.add(strings.ToLower(t.Name), deskActor(ctx))
	}
	if err != nil && dl != nil && dl.fresh {
		_ = h.leases.Release(dl.lease.Id)
		return nil, nil, err
	}
	return res, dl, err
}

func (h *Hub) Screenshot(ctx context.Context, req *connect.Request[pb.ScreenshotRequest]) (*connect.Response[pb.ScreenshotResponse], error) {
	res, _, err := h.deskDo(ctx, req.Msg.Target, false, &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Screenshot{Screenshot: req.Msg}})
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(res.GetScreenshot()), nil
}

func (h *Hub) ListWindows(ctx context.Context, req *connect.Request[pb.ListWindowsRequest]) (*connect.Response[pb.ListWindowsResponse], error) {
	res, _, err := h.deskDo(ctx, req.Msg.Target, false, &pb.DeskActionRequest{Action: &pb.DeskActionRequest_ListWindows{ListWindows: req.Msg}})
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(res.GetListWindows()), nil
}

func (h *Hub) Foreground(ctx context.Context, req *connect.Request[pb.ForegroundRequest]) (*connect.Response[pb.ForegroundResponse], error) {
	res, _, err := h.deskDo(ctx, req.Msg.Target, false, &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Foreground{Foreground: req.Msg}})
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(res.GetForeground()), nil
}

func (h *Hub) deskInput(ctx context.Context, t *pb.DeskTarget, act *pb.DeskActionRequest) (*connect.Response[pb.InputResponse], error) {
	_, dl, err := h.deskDo(ctx, t, true, act)
	if err != nil {
		return nil, err
	}
	res := connect.NewResponse(&pb.InputResponse{})
	setDeskLeaseHeaders(res.Header(), dl)
	return res, nil
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
	res, dl, err := h.deskDo(ctx, req.Msg.Target, true, &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Run{Run: req.Msg}})
	if err != nil {
		return nil, err
	}
	out := connect.NewResponse(res.GetRun())
	setDeskLeaseHeaders(out.Header(), dl)
	return out, nil
}
