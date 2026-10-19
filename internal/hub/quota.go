package hub

import (
	"context"
	"strconv"
	"strings"
	"sync"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/quota"
)

type QuotaOptions struct {
	Interval    time.Duration // poll period, default 5m
	StaleAfter  time.Duration // good data older than this is STALE, default 2*Interval
	MinRefresh  time.Duration // per-provider floor between forced reads, default 30s
	ReadTimeout time.Duration // per-read bound, default 45s
	Now         func() time.Time
}

func (o QuotaOptions) withDefaults() QuotaOptions {
	if o.Interval <= 0 {
		o.Interval = 5 * time.Minute
	}
	if o.StaleAfter <= 0 {
		o.StaleAfter = 2 * o.Interval
	}
	if o.MinRefresh <= 0 {
		o.MinRefresh = 30 * time.Second
	}
	if o.ReadTimeout <= 0 {
		o.ReadTimeout = 45 * time.Second
	}
	if o.Now == nil {
		o.Now = time.Now
	}
	return o
}

type provState struct {
	good        *quota.Snapshot
	lastAttempt time.Time
	lastErr     string
}

// QuotaCache polls readers and keeps the last good snapshot per provider.
type QuotaCache struct {
	opts    QuotaOptions
	readers []quota.Reader
	readMu  []sync.Mutex

	mu       sync.RWMutex
	state    []provState
	onUpdate func(*pb.ProviderQuota)
}

func NewQuotaCache(readers []quota.Reader, opts QuotaOptions) *QuotaCache {
	return &QuotaCache{
		opts:    opts.withDefaults(),
		readers: readers,
		readMu:  make([]sync.Mutex, len(readers)),
		state:   make([]provState, len(readers)),
	}
}

// Run polls immediately and then every Interval until ctx is done.
func (c *QuotaCache) Run(ctx context.Context) {
	c.refresh(ctx, false)
	tick := time.NewTicker(c.opts.Interval)
	defer tick.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-tick.C:
			c.refresh(ctx, false)
		}
	}
}

// Refresh reads every provider not read within MinRefresh.
func (c *QuotaCache) Refresh(ctx context.Context) { c.refresh(ctx, true) }

func (c *QuotaCache) refresh(ctx context.Context, forced bool) {
	var wg sync.WaitGroup
	for i := range c.readers {
		wg.Add(1)
		go func() {
			defer wg.Done()
			c.readOne(ctx, i, forced)
		}()
	}
	wg.Wait()
}

// readOne serializes reads per provider, so concurrent forced refreshes
// collapse into one read.
func (c *QuotaCache) readOne(ctx context.Context, i int, forced bool) {
	c.readMu[i].Lock()
	defer c.readMu[i].Unlock()

	c.mu.RLock()
	last := c.state[i].lastAttempt
	c.mu.RUnlock()
	if forced && !last.IsZero() && c.opts.Now().Sub(last) < c.opts.MinRefresh {
		return
	}

	snap := quota.ReadAll(ctx, c.readers[i:i+1], c.opts.ReadTimeout)[0]
	if ctx.Err() != nil {
		return
	}

	c.mu.Lock()
	st := &c.state[i]
	st.lastAttempt = c.opts.Now()
	st.lastErr = snap.Err
	if snap.Err == "" {
		st.good = &snap
	}
	cb := c.onUpdate
	pq := c.providerQuota(i, c.opts.Now())
	c.mu.Unlock()
	if cb != nil {
		cb(pq)
	}
}

// Snapshot returns every provider's quota with freshness computed at now.
func (c *QuotaCache) Snapshot() []*pb.ProviderQuota {
	c.mu.RLock()
	defer c.mu.RUnlock()
	now := c.opts.Now()
	out := make([]*pb.ProviderQuota, len(c.readers))
	for i := range c.readers {
		out[i] = c.providerQuota(i, now)
	}
	return out
}

// providerQuota requires c.mu held.
func (c *QuotaCache) providerQuota(i int, now time.Time) *pb.ProviderQuota {
	st := c.state[i]
	pq := &pb.ProviderQuota{
		Provider:  c.readers[i].Provider(),
		Freshness: pb.Freshness_FRESHNESS_UNKNOWN,
		Error:     st.lastErr,
	}
	if !st.lastAttempt.IsZero() {
		pq.CheckedAt = timestamppb.New(st.lastAttempt)
	}
	g := st.good
	if g == nil {
		return pq
	}
	age := max(now.Sub(g.FetchedAt), 0)
	pq.Freshness = pb.Freshness_FRESHNESS_FRESH
	if age > c.opts.StaleAfter {
		pq.Freshness = pb.Freshness_FRESHNESS_STALE
	}
	pq.Plan = g.Plan
	pq.Source = g.Source
	pq.FetchedAt = timestamppb.New(g.FetchedAt)
	pq.Age = durationpb.New(age)
	pq.Models = g.Models
	if g.Latency > 0 {
		pq.Latency = durationpb.New(g.Latency)
	}
	for _, w := range g.Windows {
		pq.Windows = append(pq.Windows, convertWindow(w, g.FetchedAt, now))
	}
	for _, r := range g.Resets {
		rc := &pb.ResetCredit{Kind: r.Kind, Count: int32(r.Count)}
		if !r.ExpiresAt.IsZero() {
			rc.ExpiresAt = timestamppb.New(r.ExpiresAt)
		}
		pq.Resets = append(pq.Resets, rc)
	}
	return pq
}

func convertWindow(w quota.Window, fetched, now time.Time) *pb.QuotaWindow {
	out := &pb.QuotaWindow{Name: w.Name, UsedPercent: w.UsedPercent}
	if !w.ResetsAt.IsZero() {
		out.ResetsAt = timestamppb.New(w.ResetsAt)
	}
	length := windowLength(w.Name)
	out.WindowSeconds = int64(length / time.Second)
	if p, ok := projectedUnused(w, length, fetched, now); ok {
		out.EstimatedUnusedPercentAtReset = &p
	}
	return out
}

// windowLength parses the trailing token of a window name such as "5h",
// "7d" or "secondary 7d"; unknown names give 0.
func windowLength(name string) time.Duration {
	f := strings.Fields(name)
	if len(f) == 0 {
		return 0
	}
	tok := f[len(f)-1]
	if tok == "weekly" {
		return 7 * 24 * time.Hour
	}
	if len(tok) < 2 {
		return 0
	}
	n, err := strconv.Atoi(tok[:len(tok)-1])
	if err != nil || n <= 0 {
		return 0
	}
	switch tok[len(tok)-1] {
	case 'h':
		return time.Duration(n) * time.Hour
	case 'd':
		return time.Duration(n) * 24 * time.Hour
	}
	return 0
}

const minElapsedFraction = 0.05

// projectedUnused extrapolates the pace between window start and the fetch
// time to the reset instant.
func projectedUnused(w quota.Window, length time.Duration, fetched, now time.Time) (float64, bool) {
	if length <= 0 || w.ResetsAt.IsZero() || now.After(w.ResetsAt) {
		return 0, false
	}
	elapsed := fetched.Sub(w.ResetsAt.Add(-length))
	frac := float64(elapsed) / float64(length)
	if frac < minElapsedFraction || frac > 1 {
		return 0, false
	}
	return min(max(100-w.UsedPercent/frac, 0), 100), true
}

func (h *Hub) GetQuota(ctx context.Context, req *connect.Request[pb.GetQuotaRequest]) (*connect.Response[pb.GetQuotaResponse], error) {
	if req.Msg.Refresh {
		h.quota.Refresh(ctx)
	}
	return connect.NewResponse(&pb.GetQuotaResponse{Quotas: h.quota.Snapshot()}), nil
}
