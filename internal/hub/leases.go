package hub

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"log"
	"math"
	"os"
	"path/filepath"
	"slices"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/encoding/protojson"
	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/protobuf/types/known/structpb"
	"google.golang.org/protobuf/types/known/timestamppb"
	"gopkg.in/yaml.v3"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/fsutil"
	"github.com/hotschmoe/protectorate/internal/mailbox"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

const (
	maxLeaseTTL   = 24 * time.Hour
	leaseSweepGap = time.Second
	// defaultClaimWindow is how long a granted ticket holds its lease for the
	// waiter before it passes to the next one.
	defaultClaimWindow = 2 * time.Minute
	// maxQueueWait drops tickets nobody could grant for this long.
	maxQueueWait = 24 * time.Hour
	ticketPrefix = "ticket-"
)

// errLeaseBusy marks a failure to acquire because a live lease holds the
// resource, as opposed to a node that cannot be used at all. Only this
// failure queues a caller that asked to wait.
var errLeaseBusy = errors.New("resource is leased")

type busyError struct{ msg string }

func (e *busyError) Error() string        { return e.msg }
func (e *busyError) Is(target error) bool { return target == errLeaseBusy }

func busyErr(format string, a ...any) error {
	return connect.NewError(connect.CodeFailedPrecondition, &busyError{msg: fmt.Sprintf(format, a...)})
}

type chooseFunc = func(blocked func(node string, gpu *int) *pb.Lease) (string, *int, error)

// ticketRecord is a queued request for a lease. Request is the proto-JSON of
// the AcquireLeaseRequest, from which the hub rebuilds the chooser after a
// restart.
type ticketRecord struct {
	ID       string        `yaml:"id"`
	Resource string        `yaml:"resource"`
	Holder   string        `yaml:"holder"`
	Notify   string        `yaml:"notify,omitempty"`
	Priority int           `yaml:"priority,omitempty"`
	Seq      uint64        `yaml:"seq"`
	Enqueued time.Time     `yaml:"enqueued"`
	TTL      time.Duration `yaml:"ttl"`
	Request  string        `yaml:"request"`
}

type queueEntry struct {
	ticketRecord
	choose chooseFunc
}

type leaseRecord struct {
	ID       string        `yaml:"id"`
	Node     string        `yaml:"node"`
	GPU      *int          `yaml:"gpu,omitempty"`
	Holder   string        `yaml:"holder"`
	TTL      time.Duration `yaml:"ttl"`
	Acquired time.Time     `yaml:"acquired"`
	Expires  time.Time     `yaml:"expires"`
	// Want is the ttl a queued waiter asked for. The grant holds the lease
	// only for the claim window; the first renew without a ttl switches to Want.
	Want time.Duration `yaml:"want,omitempty"`
}

type leaseFile struct {
	Leases []leaseRecord  `yaml:"leases"`
	Queue  []ticketRecord `yaml:"queue,omitempty"`
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

	// claimWindow, chooser and onGrant serve the queue. chooser rebuilds the
	// chooser of a ticket loaded from disk; onGrant runs under the table lock
	// and must not call back into the table.
	claimWindow time.Duration
	chooser     func(request string) (chooseFunc, error)
	onGrant     func(t ticketRecord, l *pb.Lease)

	mu      sync.Mutex
	leases  map[string]leaseRecord
	queue   []*queueEntry
	nextSeq uint64
}

// OpenLeases loads path, which may not exist yet. A corrupt file is an error
// and is never overwritten.
func OpenLeases(path string, emit func(kind, subject string, payload *structpb.Struct)) (*LeaseTable, error) {
	t := &LeaseTable{path: path, now: time.Now, emit: emit, leases: map[string]leaseRecord{}, claimWindow: defaultClaimWindow}
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
	for _, q := range f.Queue {
		if q.ID == "" || q.Holder == "" || q.TTL <= 0 {
			return nil, fmt.Errorf("parse %s: invalid ticket %q", path, q.ID)
		}
		t.queue = append(t.queue, &queueEntry{ticketRecord: q})
		t.nextSeq = max(t.nextSeq, q.Seq)
	}
	t.sortQueue()
	return t, nil
}

func (t *LeaseTable) sortQueue() {
	sort.SliceStable(t.queue, func(i, j int) bool {
		a, b := t.queue[i], t.queue[j]
		if a.Priority != b.Priority {
			return a.Priority > b.Priority
		}
		return a.Seq < b.Seq
	})
}

func (q ticketRecord) proto(position int) *pb.Lease {
	return &pb.Lease{
		Id: q.ID, Resource: q.Resource, Holder: q.Holder, Ttl: durationpb.New(q.TTL),
		AcquiredAt: timestamppb.New(q.Enqueued), ExpiresAt: timestamppb.New(q.Enqueued.Add(maxQueueWait)),
		State: pb.LeaseState_LEASE_STATE_QUEUED, QueuePosition: int32(position),
	}
}

func (t *LeaseTable) ticketEvent(kind string, q ticketRecord, extra map[string]any) {
	if t.emit == nil {
		return
	}
	m := map[string]any{"resource": q.Resource, "holder": q.Holder}
	for k, v := range extra {
		m[k] = v
	}
	p, _ := structpb.NewStruct(m)
	t.emit(kind, q.ID, p)
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
	for _, q := range t.queue {
		f.Queue = append(f.Queue, q.ticketRecord)
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

// sweepLocked drops expired leases and then hands freed resources to queued
// tickets.
func (t *LeaseTable) sweepLocked(now time.Time) {
	t.expireLocked(now)
	t.grantLocked(now)
}

// Run sweeps expired leases and grants queued tickets until ctx is done.
func (t *LeaseTable) Run(ctx context.Context) {
	tick := time.NewTicker(leaseSweepGap)
	defer tick.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-tick.C:
			t.mu.Lock()
			t.sweepLocked(t.now())
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
func (t *LeaseTable) Acquire(holder string, ttl time.Duration, choose chooseFunc) (*pb.Lease, error) {
	if err := checkAcquire(holder, ttl); err != nil {
		return nil, err
	}
	t.mu.Lock()
	defer t.mu.Unlock()
	t.sweepLocked(t.now())
	return t.acquireLocked(holder, ttl, 0, choose)
}

func checkAcquire(holder string, ttl time.Duration) error {
	if strings.TrimSpace(holder) == "" {
		return connect.NewError(connect.CodeInvalidArgument, errors.New("holder is required"))
	}
	return checkTTL(ttl)
}

// acquireLocked is Acquire for a caller that holds the lock and has swept.
// want, when set, is the ttl a queued waiter asked for; ttl is then only the
// claim window.
func (t *LeaseTable) acquireLocked(holder string, ttl, want time.Duration, choose chooseFunc) (*pb.Lease, error) {
	now := t.now()
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
	r := leaseRecord{ID: "lease-" + hex.EncodeToString(b[:]), Node: node, GPU: gpu, Holder: holder, TTL: ttl, Acquired: now, Expires: now.Add(ttl), Want: want}
	t.leases[r.ID] = r
	if err := t.persistLocked(); err != nil {
		delete(t.leases, r.ID)
		return nil, connect.NewError(connect.CodeInternal, fmt.Errorf("persist lease: %w", err))
	}
	t.event("lease.acquired", r)
	return r.proto(), nil
}

// Wait describes a caller that queues when the resource is held. Request is
// the proto-JSON of the original request, kept so the queue survives a
// restart; Notify names the sleeve to tell when the ticket is granted.
type Wait struct {
	Holder, Notify, Resource, Request string
	TTL                               time.Duration
	Priority                          int
}

// AcquireWait is Acquire that queues instead of failing when a live lease holds
// the resource. It returns the lease, or a ticket in state QUEUED. Any other
// failure is returned as is.
func (t *LeaseTable) AcquireWait(w Wait, choose chooseFunc) (*pb.Lease, error) {
	if err := checkAcquire(w.Holder, w.TTL); err != nil {
		return nil, err
	}
	t.mu.Lock()
	defer t.mu.Unlock()
	now := t.now()
	t.sweepLocked(now)
	l, err := t.acquireLocked(w.Holder, w.TTL, 0, choose)
	if err == nil || !errors.Is(err, errLeaseBusy) {
		return l, err
	}
	var b [4]byte
	if _, err := rand.Read(b[:]); err != nil {
		return nil, connect.NewError(connect.CodeInternal, err)
	}
	t.nextSeq++
	q := &queueEntry{choose: choose, ticketRecord: ticketRecord{
		ID: ticketPrefix + hex.EncodeToString(b[:]), Resource: w.Resource, Holder: w.Holder, Notify: w.Notify,
		Priority: w.Priority, Seq: t.nextSeq, Enqueued: now, TTL: w.TTL, Request: w.Request,
	}}
	prev := t.queue
	t.queue = append(append([]*queueEntry(nil), prev...), q)
	t.sortQueue()
	if err := t.persistLocked(); err != nil {
		t.queue = prev
		return nil, connect.NewError(connect.CodeInternal, fmt.Errorf("persist ticket: %w", err))
	}
	pos := t.positionLocked(q.ID)
	t.ticketEvent("lease.queued", q.ticketRecord, map[string]any{"position": float64(pos)})
	return q.proto(pos), nil
}

func (t *LeaseTable) positionLocked(id string) int {
	for i, q := range t.queue {
		if q.ID == id {
			return i + 1
		}
	}
	return 0
}

// grantLocked gives each queued ticket, in queue order, the resource it asked
// for if it is free now. The lease is held for the claim window; the waiter
// claims it by renewing. A ticket whose resource is still busy stays queued;
// one older than maxQueueWait is dropped.
func (t *LeaseTable) grantLocked(now time.Time) {
	for i := 0; i < len(t.queue); {
		q := t.queue[i]
		if now.Sub(q.Enqueued) > maxQueueWait {
			t.removeLocked(i)
			t.ticketEvent("lease.dropped", q.ticketRecord, map[string]any{"reason": "waited too long"})
			continue
		}
		if q.choose == nil && t.chooser != nil {
			c, err := t.chooser(q.Request)
			if err != nil {
				t.removeLocked(i)
				t.ticketEvent("lease.dropped", q.ticketRecord, map[string]any{"reason": err.Error()})
				continue
			}
			q.choose = c
		}
		if q.choose == nil {
			i++
			continue
		}
		window := min(t.claimWindow, q.TTL)
		prev := append([]*queueEntry(nil), t.queue...)
		t.queue = append(t.queue[:i:i], t.queue[i+1:]...)
		l, err := t.acquireLocked(q.Holder, window, q.TTL, q.choose)
		if err != nil {
			t.queue = prev
			i++
			continue
		}
		t.ticketEvent("lease.granted", q.ticketRecord, map[string]any{"lease": l.Id, "claim_by": l.ExpiresAt.AsTime().UTC().Format(time.RFC3339)})
		if t.onGrant != nil {
			t.onGrant(q.ticketRecord, l)
		}
	}
}

func (t *LeaseTable) removeLocked(i int) {
	t.queue = append(t.queue[:i:i], t.queue[i+1:]...)
	if err := t.persistLocked(); err != nil {
		fmt.Fprintf(os.Stderr, "error: persist leases: %v\n", err)
	}
}

// Renew moves a live lease's expiry to now plus ttl (or its own TTL if zero).
// The first renew without a ttl of a lease granted from the queue claims it
// for the ttl the waiter asked for.
func (t *LeaseTable) Renew(id string, ttl time.Duration) (*pb.Lease, error) {
	t.mu.Lock()
	defer t.mu.Unlock()
	now := t.now()
	t.sweepLocked(now)
	r, ok := t.leases[id]
	if !ok {
		return nil, connect.NewError(connect.CodeNotFound, fmt.Errorf("lease %q not found or expired", id))
	}
	if ttl == 0 {
		ttl = r.TTL
		if r.Want > 0 {
			ttl = r.Want
		}
	}
	if err := checkTTL(ttl); err != nil {
		return nil, err
	}
	old := r
	r.TTL, r.Expires, r.Want = ttl, now.Add(ttl), 0
	t.leases[id] = r
	if err := t.persistLocked(); err != nil {
		t.leases[id] = old
		return nil, connect.NewError(connect.CodeInternal, fmt.Errorf("persist lease: %w", err))
	}
	t.event("lease.renewed", r)
	return r.proto(), nil
}

// Release frees a live lease, or takes a ticket out of the queue.
func (t *LeaseTable) Release(id string) error {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.sweepLocked(t.now())
	for i, q := range t.queue {
		if q.ID == id {
			t.removeLocked(i)
			t.ticketEvent("lease.dropped", q.ticketRecord, map[string]any{"reason": "cancelled"})
			return nil
		}
	}
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
	t.grantLocked(t.now())
	return nil
}

// Get returns a live lease.
func (t *LeaseTable) Get(id string) (*pb.Lease, error) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.sweepLocked(t.now())
	r, ok := t.leases[id]
	if !ok {
		return nil, connect.NewError(connect.CodeNotFound, fmt.Errorf("lease %q not found or expired", id))
	}
	return r.proto(), nil
}

// List returns the live leases ordered by expiry, then the queued tickets in
// the order they will be granted.
func (t *LeaseTable) List() []*pb.Lease {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.sweepLocked(t.now())
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
	out := make([]*pb.Lease, len(rs), len(rs)+len(t.queue))
	for i, r := range rs {
		out[i] = r.proto()
	}
	for i, q := range t.queue {
		out = append(out, q.proto(i+1))
	}
	return out
}

// Blocked returns a live lease overlapping (node, gpu), if any.
func (t *LeaseTable) Blocked(node string, gpu *int) *pb.Lease {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.sweepLocked(t.now())
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
	t.sweepLocked(t.now())
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
// matching, not leased, never tier care, least loaded first. The
// error lists why each node was rejected. busy counts jobs already placed on
// a node; blocked reports an overlapping lease; warm marks nodes that hold a
// cache for the job's repository.
func pickNode(statuses []*pb.NodeStatus, sel *pb.NodeSelector, busy func(string) int, blocked func(string, *int) *pb.Lease, warm map[string]bool) (string, *int, error) {
	type choice struct {
		node  string
		gpu   *int
		score float64
	}
	var best *choice
	var why []string
	sawLease := false
	seen := func(node string, gpu *int) *pb.Lease {
		l := blocked(node, gpu)
		sawLease = sawLease || l != nil
		return l
	}
	for _, st := range statuses {
		name := st.Node.Name
		gpu, reason := nodeFits(st, sel, seen)
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
		if sawLease {
			return "", nil, busyErr("%s", msg)
		}
		return "", nil, connect.NewError(connect.CodeFailedPrecondition, errors.New(msg))
	}
	return best.node, best.gpu, nil
}

// nodeFits returns the GPU to reserve (nil for the whole node) or the reason
// the node does not fit.
func nodeFits(st *pb.NodeStatus, sel *pb.NodeSelector, blocked func(string, *int) *pb.Lease) (*int, string) {
	name := st.Node.Name
	switch {
	case st.Freshness != pb.NodeFreshness_NODE_FRESHNESS_FRESH || st.Metrics == nil:
		return nil, "not fresh"
	case st.Draining:
		return nil, "being removed"
	case st.Node.Tier == TierCare:
		return nil, "tier care"
	case sel.GetOs() != "" && !strings.EqualFold(st.Metrics.Os, sel.Os):
		return nil, "os " + st.Metrics.Os
	case sel.GetArch() != "" && !strings.EqualFold(st.Metrics.Arch, sel.Arch):
		return nil, "arch " + st.Metrics.Arch
	case sel.GetMinFreeMemoryBytes() > 0 && st.Metrics.GetMemory().GetAvailableBytes() < sel.MinFreeMemoryBytes:
		return nil, "not enough free memory"
	case sel.GetGpu() != "" && !slices.ContainsFunc(st.Metrics.Gpus, func(g *pb.GpuMetrics) bool { return accelMatches(g.Vendor, g.Name, sel.Gpu) }):
		return nil, "no gpu matching " + sel.Gpu
	case sel.GetNpu() != "" && !slices.ContainsFunc(st.Metrics.Npus, func(n *pb.AcceleratorMetrics) bool { return accelMatches(n.Vendor, n.Name, sel.Npu) }):
		return nil, "no npu matching " + sel.Npu
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
		if sel.GetGpu() != "" && !accelMatches(g.Vendor, g.Name, sel.Gpu) {
			continue
		}
		// Unknown use counts as free: a lease, not the figure, is what
		// keeps two holders off one GPU.
		free := g.MemoryTotalBytes
		if used := g.GetMemoryUsedBytes(); used < free {
			free -= used
		} else {
			free = 0
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

// accelMatches reports whether a GPU or NPU selector text appears in the
// device's vendor or name, ignoring case.
func accelMatches(vendor, name, want string) bool {
	return strings.Contains(strings.ToLower(vendor+" "+name), strings.ToLower(strings.TrimSpace(want)))
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
// Its tier is the caller's to check.
func (m *NodeMonitor) namedNode(name string, gpu *int) error {
	st, err := m.Status(name)
	if err != nil {
		return nodeErr(err)
	}
	switch {
	case st.Freshness == pb.NodeFreshness_NODE_FRESHNESS_UNREACHABLE:
		return connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("node %s is unreachable", name))
	case st.Freshness == pb.NodeFreshness_NODE_FRESHNESS_INVITED:
		return connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("node %s is invited but has not joined yet", name))
	case st.Draining:
		return connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("node %s is being removed", name))
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

// careGate admits work on a named node: any node of tier shared, a care node
// only for the owner or a token holding node_admin:<glob> matching it.
func (m *NodeMonitor) careGate(ctx context.Context, name string) error {
	info, err := m.info(name)
	if err != nil || info.Tier != TierCare {
		return nil
	}
	p := auth.PrincipalFrom(ctx)
	if p.CareAdmin(info.Name) {
		return nil
	}
	who := "this caller"
	if p != nil {
		who = "token " + p.Name
	}
	return connect.NewError(connect.CodePermissionDenied, fmt.Errorf("node %s is tier care: it takes work only under an admin grant for it, and %s has none (the owner mints one per task: protectorate grant admin --node %s --task \"...\")", info.Name, who, info.Name))
}

// leaseChooser builds the function that picks what a request leases.
func (h *Hub) leaseChooser(m *pb.AcquireLeaseRequest) chooseFunc {
	return func(blocked func(string, *int) *pb.Lease) (string, *int, error) {
		if sel := m.GetSelector(); sel != nil {
			return pickNode(h.nodes.Statuses(), sel, h.jobs.busy, blocked, nil)
		}
		if desk, ok := strings.CutPrefix(strings.ToLower(strings.TrimSpace(m.GetResource())), "desk:"); ok {
			return h.acquireDesk(desk, blocked)
		}
		node, gpu, err := parseResource(m.GetResource())
		if err != nil {
			return "", nil, err
		}
		if err := h.nodes.namedNode(node, gpu); err != nil {
			return "", nil, err
		}
		if info, err := h.nodes.info(node); err == nil && info.Tier == TierCare {
			return "", nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("node %s is tier care, which is never leased: name it in a job with an admin grant for it instead", node))
		}
		if l := blocked(node, gpu); l != nil {
			return "", nil, busyErr("%s is leased by %s until %s", l.Resource, l.Holder, l.ExpiresAt.AsTime().Local().Format(time.RFC3339))
		}
		return node, gpu, nil
	}
}

// leaseCaller returns who a lease call is for. A sleeve's lease is always
// held by that sleeve and its queue notice goes to it, whatever holder it
// names; the orchestrator is notified under its own name.
func leaseCaller(ctx context.Context, holder string) (holderOut, notify string) {
	p := auth.PrincipalFrom(ctx)
	switch {
	case p != nil && strings.HasPrefix(p.Name, sleeveTokenPrefix):
		name := strings.TrimPrefix(p.Name, sleeveTokenPrefix)
		return name, name
	case p != nil && p.Name == sleeve.OrchestratorTokenName:
		return holder, sleeve.OrchestratorName
	}
	return holder, ""
}

func (h *Hub) AcquireLease(ctx context.Context, req *connect.Request[pb.AcquireLeaseRequest]) (*connect.Response[pb.Lease], error) {
	m := req.Msg
	if m.GetResource() == "" && m.GetSelector() == nil {
		return nil, connect.NewError(connect.CodeInvalidArgument, errors.New("set resource or selector"))
	}
	holder, notify := leaseCaller(ctx, m.Holder)
	if desk, ok := strings.CutPrefix(strings.ToLower(strings.TrimSpace(m.GetResource())), "desk:"); ok && m.GetSelector() == nil {
		if err := h.nodes.careGate(ctx, desk); err != nil {
			return nil, err
		}
		if err := h.gateDesk(ctx, desk, true); err != nil {
			return nil, err
		}
	}
	var lease *pb.Lease
	var err error
	if m.Wait {
		raw, merr := protojson.Marshal(m)
		if merr != nil {
			return nil, connect.NewError(connect.CodeInternal, merr)
		}
		resource := m.GetResource()
		if resource == "" {
			resource = "(selector)"
		}
		lease, err = h.leases.AcquireWait(Wait{
			Holder: holder, Notify: notify, Resource: resource, Request: string(raw),
			TTL: m.Ttl.AsDuration(), Priority: int(m.Priority),
		}, h.leaseChooser(m))
	} else {
		lease, err = h.leases.Acquire(holder, m.Ttl.AsDuration(), h.leaseChooser(m))
	}
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(lease), nil
}

// leaseChooserFromJSON rebuilds the chooser of a ticket kept across a restart.
func (h *Hub) leaseChooserFromJSON(raw string) (chooseFunc, error) {
	var m pb.AcquireLeaseRequest
	if err := (protojson.UnmarshalOptions{DiscardUnknown: true}).Unmarshal([]byte(raw), &m); err != nil {
		return nil, fmt.Errorf("read queued request: %w", err)
	}
	return h.leaseChooser(&m), nil
}

// leaseGranted tells a waiting sleeve that its lease is ready to claim.
func (h *Hub) leaseGranted(t ticketRecord, l *pb.Lease) {
	if t.Notify == "" || h.coord == nil {
		return
	}
	window := time.Until(l.ExpiresAt.AsTime()).Round(time.Second)
	body := fmt.Sprintf("Your queued lease on %s is granted as %s. It is held for you for %s; unclaimed, it passes to the next waiter.\n\nReply: claim it with `protectorate lease renew %s` (keeps it for %s), or give it up with `protectorate lease release %s`.",
		l.Resource, l.Id, window, l.Id, t.TTL, l.Id)
	if _, err := h.coord.send(hubName, sendArgs{
		To: t.Notify, Direct: true, Body: body, Kind: mailbox.Notice, RequestID: "lease-grant-" + l.Id,
	}); err != nil {
		log.Printf("lease %s: notify %s: %v", l.Id, t.Notify, err)
	}
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

// acquireDesk picks the "desk:<node>" resource. The lease record keeps the
// whole resource name as its node, so it overlaps only other leases on the
// same desktop and never blocks jobs or GPU leases on that node.
// The caller has passed careGate for the node.
func (h *Hub) acquireDesk(node string, blocked func(string, *int) *pb.Lease) (string, *int, error) {
	if !nodeNameRe.MatchString(node) {
		return "", nil, connect.NewError(connect.CodeInvalidArgument, fmt.Errorf("invalid resource desk:%s", node))
	}
	if err := h.nodes.namedNode(node, nil); err != nil {
		return "", nil, err
	}
	if err := h.gateDesk(context.Background(), node, false); err != nil {
		return "", nil, err
	}
	res := deskResource(node)
	if l := blocked(res, nil); l != nil {
		return "", nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("%s is leased by %s until %s", l.Resource, l.Holder, l.ExpiresAt.AsTime().Local().Format(time.RFC3339)))
	}
	return res, nil, nil
}
