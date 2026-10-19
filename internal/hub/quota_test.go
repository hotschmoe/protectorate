package hub

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"math"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/quota"
)

type fakeReader struct {
	name  string
	calls atomic.Int32
	mu    sync.Mutex
	snap  quota.Snapshot
	err   error
	block chan struct{}
}

func (f *fakeReader) Provider() string { return f.name }

func (f *fakeReader) set(s quota.Snapshot, err error) {
	f.mu.Lock()
	f.snap, f.err = s, err
	f.mu.Unlock()
}

func (f *fakeReader) Read(ctx context.Context) (quota.Snapshot, error) {
	f.calls.Add(1)
	if f.block != nil {
		<-f.block
	}
	f.mu.Lock()
	defer f.mu.Unlock()
	return f.snap, f.err
}

type clock struct {
	mu sync.Mutex
	t  time.Time
}

func (c *clock) now() time.Time {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.t
}

func (c *clock) advance(d time.Duration) {
	c.mu.Lock()
	c.t = c.t.Add(d)
	c.mu.Unlock()
}

var t0 = time.Date(2026, 10, 19, 12, 0, 0, 0, time.UTC)

func newCache(t *testing.T, readers ...quota.Reader) (*QuotaCache, *clock) {
	t.Helper()
	clk := &clock{t: t0}
	return NewQuotaCache(readers, QuotaOptions{
		Interval:   5 * time.Minute,
		StaleAfter: 10 * time.Minute,
		MinRefresh: 30 * time.Second,
		Now:        clk.now,
	}), clk
}

func good(clk *clock, used float64, resetsIn time.Duration) quota.Snapshot {
	return quota.Snapshot{
		Plan:      "max",
		FetchedAt: clk.now(),
		Windows:   []quota.Window{{Name: "5h", UsedPercent: used, ResetsAt: clk.now().Add(resetsIn)}},
		Resets:    []quota.Reset{{Kind: "credit", Count: 2}},
	}
}

func TestFreshness(t *testing.T) {
	tests := []struct {
		name      string
		setup     func(f *fakeReader, clk *clock)
		advance   time.Duration
		second    func(f *fakeReader, clk *clock)
		want      pb.Freshness
		wantErr   string
		wantWins  int
		wantFirst float64
	}{
		{
			name:     "fresh",
			setup:    func(f *fakeReader, clk *clock) { f.set(good(clk, 40, 3*time.Hour), nil) },
			want:     pb.Freshness_FRESHNESS_FRESH,
			wantWins: 1, wantFirst: 40,
		},
		{
			name:  "never read successfully is unknown with no windows",
			setup: func(f *fakeReader, clk *clock) { f.set(quota.Snapshot{}, errors.New("credentials expired")) },
			want:  pb.Freshness_FRESHNESS_UNKNOWN, wantErr: "credentials expired",
		},
		{
			name:     "failure after good within threshold stays fresh and reports error",
			setup:    func(f *fakeReader, clk *clock) { f.set(good(clk, 40, 3*time.Hour), nil) },
			advance:  5 * time.Minute,
			second:   func(f *fakeReader, clk *clock) { f.set(quota.Snapshot{}, errors.New("HTTP 500")) },
			want:     pb.Freshness_FRESHNESS_FRESH,
			wantErr:  "HTTP 500",
			wantWins: 1, wantFirst: 40,
		},
		{
			name:     "failure after good beyond threshold is stale and keeps old data",
			setup:    func(f *fakeReader, clk *clock) { f.set(good(clk, 40, 3*time.Hour), nil) },
			advance:  11 * time.Minute,
			second:   func(f *fakeReader, clk *clock) { f.set(quota.Snapshot{}, errors.New("HTTP 500")) },
			want:     pb.Freshness_FRESHNESS_STALE,
			wantErr:  "HTTP 500",
			wantWins: 1, wantFirst: 40,
		},
		{
			name:     "recovery replaces stale data",
			setup:    func(f *fakeReader, clk *clock) { f.set(good(clk, 40, 3*time.Hour), nil) },
			advance:  11 * time.Minute,
			second:   func(f *fakeReader, clk *clock) { f.set(good(clk, 55, 2*time.Hour), nil) },
			want:     pb.Freshness_FRESHNESS_FRESH,
			wantWins: 1, wantFirst: 55,
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			f := &fakeReader{name: "claude"}
			c, clk := newCache(t, f)
			tt.setup(f, clk)
			c.refresh(context.Background(), false)
			if tt.second != nil {
				clk.advance(tt.advance)
				tt.second(f, clk)
				c.refresh(context.Background(), false)
			}
			got := c.Snapshot()[0]
			if got.Freshness != tt.want || got.Error != tt.wantErr || len(got.Windows) != tt.wantWins {
				t.Fatalf("got freshness=%v err=%q windows=%d", got.Freshness, got.Error, len(got.Windows))
			}
			if tt.wantWins > 0 && got.Windows[0].UsedPercent != tt.wantFirst {
				t.Fatalf("used %v, want %v", got.Windows[0].UsedPercent, tt.wantFirst)
			}
			if tt.want == pb.Freshness_FRESHNESS_UNKNOWN && (got.FetchedAt != nil || got.Age != nil || len(got.Resets) != 0) {
				t.Fatalf("unknown provider carries data: %v", got)
			}
		})
	}
}

func TestStaleByAgeAlone(t *testing.T) {
	f := &fakeReader{name: "codex"}
	c, clk := newCache(t, f)
	f.set(good(clk, 10, 3*time.Hour), nil)
	c.refresh(context.Background(), false)
	clk.advance(20 * time.Minute)
	got := c.Snapshot()[0]
	if got.Freshness != pb.Freshness_FRESHNESS_STALE || got.Age.AsDuration() != 20*time.Minute {
		t.Fatalf("got %v age %v", got.Freshness, got.Age.AsDuration())
	}
}

func TestRefreshRateLimit(t *testing.T) {
	a, b := &fakeReader{name: "a"}, &fakeReader{name: "b"}
	c, clk := newCache(t, a, b)
	a.set(good(clk, 1, time.Hour), nil)
	b.set(good(clk, 1, time.Hour), nil)
	ctx := context.Background()

	c.refresh(ctx, false)
	c.Refresh(ctx)
	if a.calls.Load() != 1 || b.calls.Load() != 1 {
		t.Fatalf("forced refresh right after poll read again: %d %d", a.calls.Load(), b.calls.Load())
	}
	clk.advance(31 * time.Second)
	c.Refresh(ctx)
	c.Refresh(ctx)
	if a.calls.Load() != 2 || b.calls.Load() != 2 {
		t.Fatalf("calls %d %d, want 2 2", a.calls.Load(), b.calls.Load())
	}
	clk.advance(time.Second)
	c.refresh(ctx, false)
	if a.calls.Load() != 3 {
		t.Fatalf("poll must not be rate limited, calls %d", a.calls.Load())
	}
}

func TestConcurrentRefreshCollapses(t *testing.T) {
	f := &fakeReader{name: "a", block: make(chan struct{})}
	c, clk := newCache(t, f)
	f.set(good(clk, 1, time.Hour), nil)
	var wg sync.WaitGroup
	for range 8 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			c.Refresh(context.Background())
			_ = c.Snapshot()
		}()
	}
	time.Sleep(50 * time.Millisecond)
	close(f.block)
	wg.Wait()
	if n := f.calls.Load(); n != 1 {
		t.Fatalf("calls %d, want 1", n)
	}
}

func TestCancelledRefreshNotRecorded(t *testing.T) {
	f := &fakeReader{name: "a"}
	c, clk := newCache(t, f)
	f.set(good(clk, 1, time.Hour), nil)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	c.Refresh(ctx)
	if got := c.Snapshot()[0]; got.Freshness != pb.Freshness_FRESHNESS_UNKNOWN || got.Error != "" || got.CheckedAt != nil {
		t.Fatalf("cancelled read was recorded: %v", got)
	}
}

func TestProjection(t *testing.T) {
	tests := []struct {
		name    string
		win     quota.Window
		fetched time.Time
		now     time.Time
		want    float64
		wantOK  bool
	}{
		{"5h half elapsed 30 used", quota.Window{Name: "5h", UsedPercent: 30, ResetsAt: t0.Add(150 * time.Minute)}, t0, t0, 40, true},
		{"7d quarter elapsed 10 used", quota.Window{Name: "7d", UsedPercent: 10, ResetsAt: t0.Add(126 * time.Hour)}, t0, t0, 60, true},
		{"overburn clamps to zero", quota.Window{Name: "5h", UsedPercent: 80, ResetsAt: t0.Add(150 * time.Minute)}, t0, t0, 0, true},
		{"prefixed name", quota.Window{Name: "secondary 7d", UsedPercent: 0, ResetsAt: t0.Add(84 * time.Hour)}, t0, t0, 100, true},
		{"weekly alias", quota.Window{Name: "weekly", UsedPercent: 25, ResetsAt: t0.Add(84 * time.Hour)}, t0, t0, 50, true},
		{"window too young", quota.Window{Name: "5h", UsedPercent: 1, ResetsAt: t0.Add(299 * time.Minute)}, t0, t0, 0, false},
		{"unknown length", quota.Window{Name: "credits", UsedPercent: 5, ResetsAt: t0.Add(time.Hour)}, t0, t0, 0, false},
		{"no reset time", quota.Window{Name: "5h", UsedPercent: 5}, t0, t0, 0, false},
		{"already reset", quota.Window{Name: "5h", UsedPercent: 5, ResetsAt: t0.Add(time.Hour)}, t0, t0.Add(2 * time.Hour), 0, false},
		{"stale uses fetch-time pace", quota.Window{Name: "5h", UsedPercent: 30, ResetsAt: t0.Add(150 * time.Minute)}, t0, t0.Add(30 * time.Minute), 40, true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, ok := projectedUnused(tt.win, windowLength(tt.win.Name), tt.fetched, tt.now)
			if ok != tt.wantOK || (ok && math.Abs(got-tt.want) > 1e-9) {
				t.Fatalf("got %v %v, want %v %v", got, ok, tt.want, tt.wantOK)
			}
		})
	}
}

func TestWindowLength(t *testing.T) {
	tests := map[string]time.Duration{
		"5h": 5 * time.Hour, "7d": 7 * 24 * time.Hour, "30d": 30 * 24 * time.Hour, "weekly": 7 * 24 * time.Hour,
		"primary 5h": 5 * time.Hour, "credits": 0, "": 0, "h": 0, "xd": 0, "0h": 0, "monthly": 0, "90m": 0,
	}
	for name, want := range tests {
		if got := windowLength(name); got != want {
			t.Errorf("windowLength(%q) = %v, want %v", name, got, want)
		}
	}
}

func TestGetQuotaService(t *testing.T) {
	f := &fakeReader{name: "claude"}
	g := &fakeReader{name: "grok"}
	c, clk := newCache(t, f, g)
	f.set(good(clk, 30, 150*time.Minute), nil)
	g.set(quota.Snapshot{}, errors.New("credentials expired"))
	c.refresh(context.Background(), false)

	h := New("test", testToken)
	h.SetQuota(c)
	client := protectoratev1connect.NewQuotaServiceClient(&http.Client{Transport: bearer{testToken}}, newServer(t, h))

	clk.advance(40 * time.Second)
	f.set(good(clk, 35, 149*time.Minute), nil)
	res, err := client.GetQuota(context.Background(), connect.NewRequest(&pb.GetQuotaRequest{Refresh: true}))
	if err != nil {
		t.Fatal(err)
	}
	qs := res.Msg.Quotas
	if len(qs) != 2 || qs[0].Provider != "claude" || qs[1].Provider != "grok" {
		t.Fatalf("unexpected quotas: %v", qs)
	}
	if qs[0].Freshness != pb.Freshness_FRESHNESS_FRESH || qs[0].Windows[0].UsedPercent != 35 ||
		qs[0].Windows[0].WindowSeconds != 5*3600 || qs[0].Windows[0].EstimatedUnusedPercentAtReset == nil ||
		qs[0].Resets[0].Count != 2 {
		t.Fatalf("claude: %v", qs[0])
	}
	if qs[1].Freshness != pb.Freshness_FRESHNESS_UNKNOWN || len(qs[1].Windows) != 0 || qs[1].Error == "" {
		t.Fatalf("grok: %v", qs[1])
	}
}

func TestQuotaUpdatedEvent(t *testing.T) {
	f := &fakeReader{name: "claude"}
	c, clk := newCache(t, f)
	f.set(good(clk, 30, 150*time.Minute), nil)
	h := New("test", testToken)
	h.Heartbeat = time.Hour
	h.SetQuota(c)
	srv := newServer(t, h)

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	s, err := protectoratev1connect.NewEventServiceClient(&http.Client{Transport: bearer{testToken}}, srv).
		Subscribe(ctx, connect.NewRequest(&pb.SubscribeRequest{}))
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	if !s.Receive() || s.Msg().Kind != "heartbeat" {
		t.Fatalf("want initial heartbeat, err %v", s.Err())
	}
	go c.refresh(ctx, false)
	if !s.Receive() {
		t.Fatalf("no event: %v", s.Err())
	}
	e := s.Msg()
	if e.Kind != "quota.updated" || e.Subject != "claude" || e.Payload.Fields["freshness"].GetStringValue() != "FRESHNESS_FRESH" {
		t.Fatalf("unexpected event: %v", e)
	}
}

func TestStrictJSONRejectsUnknownFields(t *testing.T) {
	_, srv := newTestHub(t)
	tests := []struct {
		name     string
		method   string
		body     string
		wantCode int
	}{
		{"valid", "QuotaService/GetQuota", `{"refresh":false}`, http.StatusOK},
		{"empty object", "QuotaService/GetQuota", `{}`, http.StatusOK},
		{"typo field", "QuotaService/GetQuota", `{"refesh":true}`, http.StatusBadRequest},
		{"unknown field on other service", "HubService/GetStatus", `{"verbose":true}`, http.StatusBadRequest},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			req, _ := http.NewRequest(http.MethodPost, srv.URL+"/protectorate.v1."+tt.method, strings.NewReader(tt.body))
			req.Header.Set("Content-Type", "application/json")
			req.Header.Set("Authorization", "Bearer "+testToken)
			res, err := http.DefaultClient.Do(req)
			if err != nil {
				t.Fatal(err)
			}
			defer res.Body.Close()
			if res.StatusCode != tt.wantCode {
				t.Fatalf("status %d, want %d", res.StatusCode, tt.wantCode)
			}
		})
	}
}

func TestStrictJSONTypedError(t *testing.T) {
	_, srv := newTestHub(t)
	hc := &http.Client{Transport: bearer{testToken}}
	req, _ := http.NewRequest(http.MethodPost, srv.URL+"/protectorate.v1.QuotaService/GetQuota", strings.NewReader(`{"refesh":true}`))
	req.Header.Set("Content-Type", "application/json")
	res, err := hc.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	var body struct {
		Code string `json:"code"`
	}
	if err := decodeJSON(res.Body, &body); err != nil || body.Code != "invalid_argument" {
		t.Fatalf("body code %q, err %v", body.Code, err)
	}
}

func decodeJSON(r io.Reader, v any) error { return json.NewDecoder(r).Decode(v) }

func newServer(t *testing.T, h *Hub) string {
	t.Helper()
	srv := httptest.NewServer(h.Handler())
	t.Cleanup(srv.Close)
	return srv.URL
}
