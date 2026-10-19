package hub

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"math"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/protobuf/types/known/structpb"
	"google.golang.org/protobuf/types/known/timestamppb"
	"gopkg.in/yaml.v3"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/fsutil"
)

const (
	maxLeaseTTL   = 24 * time.Hour
	leaseSweepGap = time.Second
)

type leaseRecord struct {
	ID       string        `yaml:"id"`
	Node     string        `yaml:"node"`
	GPU      *int          `yaml:"gpu,omitempty"`
	Holder   string        `yaml:"holder"`
	TTL      time.Duration `yaml:"ttl"`
	Acquired time.Time     `yaml:"acquired"`
	Expires  time.Time     `yaml:"expires"`
}

type leaseFile struct {
	Leases []leaseRecord `yaml:"leases"`
}

func (r leaseRecord) resource() string {
	if r.GPU == nil {
		return r.Node
	}
	return fmt.Sprintf("%s:gpu%d", r.Node, *r.GPU)
}

func (r leaseRecord) proto() *pb.Lease {
	l := &pb.Lease{
		Id: r.ID, Resource: r.resource(), Node: r.Node, Holder: r.Holder,
		Ttl: durationpb.New(r.TTL), AcquiredAt: timestamppb.New(r.Acquired), ExpiresAt: timestamppb.New(r.Expires),
	}
	if r.GPU != nil {
		g := int32(*r.GPU)
		l.GpuIndex = &g
	}
	return l
}

// conflicts reports whether a lease on (node, gpu) overlaps r: a whole-node
// lease overlaps everything on the node, a GPU lease only the same GPU.
func (r leaseRecord) conflicts(node string, gpu *int) bool {
	if r.Node != node {
		return false
	}
	return r.GPU == nil || gpu == nil || *r.GPU == *gpu
}

// LeaseTable holds the live leases, persisted to a YAML file so they survive
// a hub restart. Expired leases are dropped lazily and by Run.
type LeaseTable struct {
	path string
	now  func() time.Time
	emit func(kind, subject string, payload *structpb.Struct)

	mu     sync.Mutex
	leases map[string]leaseRecord
}

// OpenLeases loads path, which may not exist yet. A corrupt file is an error
// and is never overwritten.
func OpenLeases(path string, emit func(kind, subject string, payload *structpb.Struct)) (*LeaseTable, error) {
	t := &LeaseTable{path: path, now: time.Now, emit: emit, leases: map[string]leaseRecord{}}
	raw, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return t, nil
	}
	if err != nil {
		return nil, fmt.Errorf("read leases: %w", err)
	}
	var f leaseFile
	dec := yaml.NewDecoder(bytes.NewReader(raw))
	dec.KnownFields(true)
	if err := dec.Decode(&f); err != nil && !errors.Is(err, io.EOF) {
		return nil, fmt.Errorf("parse %s: %w", path, err)
	}
	for _, r := range f.Leases {
		if r.ID == "" || r.Node == "" || r.TTL <= 0 {
			return nil, fmt.Errorf("parse %s: invalid lease %q", path, r.ID)
		}
		if _, dup := t.leases[r.ID]; dup {
			return nil, fmt.Errorf("parse %s: duplicate lease %q", path, r.ID)
		}
		t.leases[r.ID] = r
	}
	return t, nil
}

func (t *LeaseTable) event(kind string, r leaseRecord) {
	if t.emit == nil {
		return
	}
	p, _ := structpb.NewStruct(map[string]any{
		"resource": r.resource(), "node": r.Node, "holder": r.Holder, "expires_at": r.Expires.UTC().Format(time.RFC3339),
	})
	t.emit(kind, r.ID, p)
}

// persistLocked rewrites the file. Callers roll back their change on error.
func (t *LeaseTable) persistLocked() error {
	f := leaseFile{Leases: make([]leaseRecord, 0, len(t.leases))}
	for _, r := range t.leases {
		f.Leases = append(f.Leases, r)
	}
	sort.Slice(f.Leases, func(i, j int) bool { return f.Leases[i].ID < f.Leases[j].ID })
	raw, err := yaml.Marshal(f)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(t.path), 0o700); err != nil {
		return err
	}
	return fsutil.WriteFile(t.path, raw, 0o600)
}

// expireLocked drops leases past their expiry and reports whether any went.
func (t *LeaseTable) expireLocked(now time.Time) {
	var gone []leaseRecord
	for id, r := range t.leases {
		if !r.Expires.After(now) {
			gone = append(gone, r)
			delete(t.leases, id)
		}
	}
	if len(gone) == 0 {
		return
	}
	if err := t.persistLocked(); err != nil {
		fmt.Fprintf(os.Stderr, "error: persist leases: %v\n", err)
	}
	sort.Slice(gone, func(i, j int) bool { return gone[i].ID < gone[j].ID })
	for _, r := range gone {
		t.event("lease.expired", r)
	}
}

// Run sweeps expired leases until ctx is done.
func (t *LeaseTable) Run(ctx context.Context) {
	tick := time.NewTicker(leaseSweepGap)
	defer tick.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-tick.C:
			t.mu.Lock()
			t.expireLocked(t.now())
			t.mu.Unlock()
		}
	}
}

func checkTTL(ttl time.Duration) error {
	if ttl <= 0 || ttl > maxLeaseTTL {
		return connect.NewError(connect.CodeInvalidArgument, fmt.Errorf("ttl must be positive and at most %s", maxLeaseTTL))
	}
	return nil
}

// Acquire records a lease on whatever choose picks. choose runs under the
// table lock, so two selections never pick the same resource; blocked tells it
// which (node, gpu) overlap a live lease.
func (t *LeaseTable) Acquire(holder string, ttl time.Duration, choose func(blocked func(node string, gpu *int) *pb.Lease) (string, *int, error)) (*pb.Lease, error) {
	if strings.TrimSpace(holder) == "" {
		return nil, connect.NewError(connect.CodeInvalidArgument, errors.New("holder is required"))
	}
	if err := checkTTL(ttl); err != nil {
		return nil, err
	}
	t.mu.Lock()
	defer t.mu.Unlock()
	now := t.now()
	t.expireLocked(now)
	node, gpu, err := choose(func(node string, gpu *int) *pb.Lease {
		for _, r := range t.leases {
			if r.conflicts(node, gpu) {
				return r.proto()
			}
		}
		return nil
	})
	if err != nil {
		return nil, err
	}
	var b [4]byte
	if _, err := rand.Read(b[:]); err != nil {
		return nil, connect.NewError(connect.CodeInternal, err)
	}
	r := leaseRecord{ID: "lease-" + hex.EncodeToString(b[:]), Node: node, GPU: gpu, Holder: holder, TTL: ttl, Acquired: now, Expires: now.Add(ttl)}
	t.leases[r.ID] = r
	if err := t.persistLocked(); err != nil {
		delete(t.leases, r.ID)
		return nil, connect.NewError(connect.CodeInternal, fmt.Errorf("persist lease: %w", err))
	}
	t.event("lease.acquired", r)
	return r.proto(), nil
}

// Renew moves a live lease's expiry to now plus ttl (or its own TTL if zero).
func (t *LeaseTable) Renew(id string, ttl time.Duration) (*pb.Lease, error) {
	t.mu.Lock()
	defer t.mu.Unlock()
	now := t.now()
	t.expireLocked(now)
	r, ok := t.leases[id]
	if !ok {
		return nil, connect.NewError(connect.CodeNotFound, fmt.Errorf("lease %q not found or expired", id))
	}
	if ttl == 0 {
		ttl = r.TTL
	}
	if err := checkTTL(ttl); err != nil {
		return nil, err
	}
	old := r
	r.TTL, r.Expires = ttl, now.Add(ttl)
	t.leases[id] = r
	if err := t.persistLocked(); err != nil {
		t.leases[id] = old
		return nil, connect.NewError(connect.CodeInternal, fmt.Errorf("persist lease: %w", err))
	}
	t.event("lease.renewed", r)
	return r.proto(), nil
}

// Release frees a live lease.
func (t *LeaseTable) Release(id string) error {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.expireLocked(t.now())
	r, ok := t.leases[id]
	if !ok {
		return connect.NewError(connect.CodeNotFound, fmt.Errorf("lease %q not found or expired", id))
	}
	delete(t.leases, id)
	if err := t.persistLocked(); err != nil {
		t.leases[id] = r
		return connect.NewError(connect.CodeInternal, fmt.Errorf("persist lease: %w", err))
	}
	t.event("lease.released", r)
	return nil
}

// Get returns a live lease.
func (t *LeaseTable) Get(id string) (*pb.Lease, error) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.expireLocked(t.now())
	r, ok := t.leases[id]
	if !ok {
		return nil, connect.NewError(connect.CodeNotFound, fmt.Errorf("lease %q not found or expired", id))
	}
	return r.proto(), nil
}

// List returns the live leases ordered by expiry.
func (t *LeaseTable) List() []*pb.Lease {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.expireLocked(t.now())
	rs := make([]leaseRecord, 0, len(t.leases))
	for _, r := range t.leases {
		rs = append(rs, r)
	}
	sort.Slice(rs, func(i, j int) bool {
		if !rs[i].Expires.Equal(rs[j].Expires) {
			return rs[i].Expires.Before(rs[j].Expires)
		}
		return rs[i].ID < rs[j].ID
	})
	out := make([]*pb.Lease, len(rs))
	for i, r := range rs {
		out[i] = r.proto()
	}
	return out
}

// Blocked returns a live lease overlapping (node, gpu), if any.
func (t *LeaseTable) Blocked(node string, gpu *int) *pb.Lease {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.expireLocked(t.now())
	for _, r := range t.leases {
		if r.conflicts(node, gpu) {
			return r.proto()
		}
	}
	return nil
}

// NodeLease returns a live lease that covers a whole node, if any.
func (t *LeaseTable) NodeLease(node string) *pb.Lease {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.expireLocked(t.now())
	for _, r := range t.leases {
		if r.Node == node && r.GPU == nil {
			return r.proto()
		}
	}
	return nil
}

// warmBonus is how much lighter a node with a warm cache for the job's repo
// counts: a warm target/ beats a somewhat faster cold node.
const warmBonus = 20

// pickNode returns the node (and GPU) for a selector among statuses: fresh,
// matching, not leased, not family unless allowed, least loaded first. The
// error lists why each node was rejected. busy counts jobs already placed on
// a node; blocked reports an overlapping lease; warm marks nodes that hold a
// cache for the job's repository.
func pickNode(statuses []*pb.NodeStatus, sel *pb.NodeSelector, allowFamily bool, busy func(string) int, blocked func(string, *int) *pb.Lease, warm map[string]bool) (string, *int, error) {
	type choice struct {
		node  string
		gpu   *int
		score float64
	}
	var best *choice
	var why []string
	for _, st := range statuses {
		name := st.Node.Name
		gpu, reason := nodeFits(st, sel, allowFamily, blocked)
		if reason != "" {
			why = append(why, name+": "+reason)
			continue
		}
		c := choice{node: name, gpu: gpu, score: loadScore(st, busy(name))}
		if warm[name] {
			c.score -= warmBonus
		}
		if best == nil || c.score < best.score || (c.score == best.score && c.node < best.node) {
			best = &c
		}
	}
	if best == nil {
		msg := "no node matches"
		if len(why) > 0 {
			msg += " (" + strings.Join(why, "; ") + ")"
		}
		return "", nil, connect.NewError(connect.CodeFailedPrecondition, errors.New(msg))
	}
	return best.node, best.gpu, nil
}

// nodeFits returns the GPU to reserve (nil for the whole node) or the reason
// the node does not fit.
func nodeFits(st *pb.NodeStatus, sel *pb.NodeSelector, allowFamily bool, blocked func(string, *int) *pb.Lease) (*int, string) {
	name := st.Node.Name
	switch {
	case st.Freshness != pb.NodeFreshness_NODE_FRESHNESS_FRESH || st.Metrics == nil:
		return nil, "not fresh"
	case st.Node.Tier == TierFamily && !allowFamily:
		return nil, "tier family"
	case sel.GetOs() != "" && !strings.EqualFold(st.Metrics.Os, sel.Os):
		return nil, "os " + st.Metrics.Os
	case sel.GetArch() != "" && !strings.EqualFold(st.Metrics.Arch, sel.Arch):
		return nil, "arch " + st.Metrics.Arch
	case sel.GetMinFreeMemoryBytes() > 0 && st.Metrics.GetMemory().GetAvailableBytes() < sel.MinFreeMemoryBytes:
		return nil, "not enough free memory"
	}
	if sel.GetGpuVramBytes() == 0 {
		if l := blocked(name, nil); l != nil {
			return nil, "leased by " + l.Holder
		}
		return nil, ""
	}
	var gpu *int
	var bestFree uint64
	for _, g := range st.Metrics.Gpus {
		free := uint64(0)
		if g.MemoryTotalBytes > g.MemoryUsedBytes {
			free = g.MemoryTotalBytes - g.MemoryUsedBytes
		}
		idx := int(g.Index)
		if free < sel.GpuVramBytes || blocked(name, &idx) != nil {
			continue
		}
		if gpu == nil || free > bestFree {
			gpu, bestFree = &idx, free
		}
	}
	if gpu == nil {
		return nil, "no free gpu with enough vram"
	}
	return gpu, ""
}

// loadScore ranks nodes, lower is better: mean of CPU and memory use, plus a
// full core's worth of load per job already placed there.
func loadScore(st *pb.NodeStatus, jobs int) float64 {
	cores := math.Max(float64(st.Metrics.GetCpu().GetLogicalCores()), 1)
	return (st.Metrics.GetCpu().GetUsagePercent()+st.Metrics.GetMemory().GetUsedPercent())/2 + float64(jobs)*100/cores
}

// parseResource splits "node" or "node:gpu<N>".
func parseResource(s string) (node string, gpu *int, err error) {
	node, g, hasGPU := strings.Cut(strings.ToLower(strings.TrimSpace(s)), ":")
	if !nodeNameRe.MatchString(node) {
		return "", nil, connect.NewError(connect.CodeInvalidArgument, fmt.Errorf("invalid resource %q", s))
	}
	if !hasGPU {
		return node, nil, nil
	}
	n, convErr := strconv.Atoi(strings.TrimPrefix(g, "gpu"))
	if !strings.HasPrefix(g, "gpu") || convErr != nil || n < 0 {
		return "", nil, connect.NewError(connect.CodeInvalidArgument, fmt.Errorf("invalid resource %q: want <node> or <node>:gpu<index>", s))
	}
	return node, &n, nil
}

// namedNode checks that an explicitly named node (and GPU) can be used now.
func (m *NodeMonitor) namedNode(name string, gpu *int, allowFamily bool) error {
	st, err := m.Status(name)
	if err != nil {
		return nodeErr(err)
	}
	switch {
	case st.Freshness == pb.NodeFreshness_NODE_FRESHNESS_UNREACHABLE:
		return connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("node %s is unreachable", name))
	case st.Node.Tier == TierFamily && !allowFamily:
		return connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("node %s is tier family: set allow_family to use it", name))
	}
	if gpu != nil {
		found := false
		for _, g := range st.Metrics.GetGpus() {
			found = found || int(g.Index) == *gpu
		}
		if !found {
			return connect.NewError(connect.CodeNotFound, fmt.Errorf("node %s has no gpu %d", name, *gpu))
		}
	}
	return nil
}

func (h *Hub) AcquireLease(_ context.Context, req *connect.Request[pb.AcquireLeaseRequest]) (*connect.Response[pb.Lease], error) {
	m := req.Msg
	if m.GetResource() == "" && m.GetSelector() == nil {
		return nil, connect.NewError(connect.CodeInvalidArgument, errors.New("set resource or selector"))
	}
	lease, err := h.leases.Acquire(m.Holder, m.Ttl.AsDuration(), func(blocked func(string, *int) *pb.Lease) (string, *int, error) {
		if sel := m.GetSelector(); sel != nil {
			return pickNode(h.nodes.Statuses(), sel, m.AllowFamily, h.jobs.busy, blocked, nil)
		}
		node, gpu, err := parseResource(m.GetResource())
		if err != nil {
			return "", nil, err
		}
		if err := h.nodes.namedNode(node, gpu, m.AllowFamily); err != nil {
			return "", nil, err
		}
		if l := blocked(node, gpu); l != nil {
			return "", nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("%s is leased by %s until %s", l.Resource, l.Holder, l.ExpiresAt.AsTime().Local().Format(time.RFC3339)))
		}
		return node, gpu, nil
	})
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(lease), nil
}

func (h *Hub) RenewLease(_ context.Context, req *connect.Request[pb.RenewLeaseRequest]) (*connect.Response[pb.Lease], error) {
	l, err := h.leases.Renew(req.Msg.Id, req.Msg.Ttl.AsDuration())
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(l), nil
}

func (h *Hub) ReleaseLease(_ context.Context, req *connect.Request[pb.ReleaseLeaseRequest]) (*connect.Response[pb.ReleaseLeaseResponse], error) {
	if err := h.leases.Release(req.Msg.Id); err != nil {
		return nil, err
	}
	return connect.NewResponse(&pb.ReleaseLeaseResponse{}), nil
}

func (h *Hub) ListLeases(context.Context, *connect.Request[pb.ListLeasesRequest]) (*connect.Response[pb.ListLeasesResponse], error) {
	return connect.NewResponse(&pb.ListLeasesResponse{Leases: h.leases.List()}), nil
}
