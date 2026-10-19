package hub

import (
	"context"
	"errors"
	"net/http"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/quota"
)

func win(name string, used float64, resetsIn time.Duration, est float64) *pb.QuotaWindow {
	w := &pb.QuotaWindow{Name: name, UsedPercent: used}
	if resetsIn != 0 {
		w.ResetsAt = timestamppb.New(t0.Add(resetsIn))
	}
	if est >= 0 {
		w.EstimatedUnusedPercentAtReset = &est
	}
	return w
}

func pq(provider string, f pb.Freshness, ws ...*pb.QuotaWindow) *pb.ProviderQuota {
	q := &pb.ProviderQuota{Provider: provider, Freshness: f, Windows: ws}
	if f == pb.Freshness_FRESHNESS_STALE {
		q.Age = durationpb.New(3 * time.Hour)
	}
	return q
}

const fresh = pb.Freshness_FRESHNESS_FRESH

func order(got []*pb.HarnessSuggestion) string {
	var parts []string
	for _, s := range got {
		parts = append(parts, strings.TrimPrefix(s.Harness.String(), "HARNESS_"))
	}
	return strings.Join(parts, ",")
}

func TestRankHarnesses(t *testing.T) {
	const h = time.Hour
	tests := []struct {
		name   string
		quotas []*pb.ProviderQuota
		size   pb.TaskSize
		want   string
		top    string
	}{
		{
			name: "more expiring capacity before a sooner reset wins",
			quotas: []*pb.ProviderQuota{
				pq("claude", fresh, win("7d", 80, 100*h, 10)),
				pq("codex", fresh, win("7d", 30, 20*h, 60)),
				pq("grok", fresh, win("weekly", 90, 10*h, 5)),
			},
			want: "CODEX,CLAUDE,PI,GROK",
		},
		{
			name: "stale and unknown are never spare",
			quotas: []*pb.ProviderQuota{
				pq("claude", pb.Freshness_FRESHNESS_STALE, win("7d", 0, 10*h, 100)),
				pq("codex", pb.Freshness_FRESHNESS_UNKNOWN),
				pq("grok", fresh, win("weekly", 50, 10*h, 40)),
			},
			want: "GROK,PI,CLAUDE,CODEX",
		},
		{
			name: "tightest window caps usable capacity",
			quotas: []*pb.ProviderQuota{
				pq("claude", fresh, win("5h", 97, 2*h, -1), win("7d", 20, 30*h, 70)),
				pq("codex", fresh, win("7d", 60, 30*h, 20)),
			},
			size: pb.TaskSize_TASK_SIZE_SMALL,
			want: "CODEX,PI,CLAUDE,GROK",
		},
		{
			name: "large task needs more headroom",
			quotas: []*pb.ProviderQuota{
				pq("claude", fresh, win("7d", 80, 10*h, 30)),
				pq("codex", fresh, win("7d", 50, 10*h, 30)),
			},
			size: pb.TaskSize_TASK_SIZE_LARGE,
			want: "CODEX,PI,CLAUDE,GROK",
		},
		{
			name: "window about to reset is worth more than the same waste later",
			quotas: []*pb.ProviderQuota{
				pq("claude", fresh, win("7d", 50, 150*h, 50)),
				pq("codex", fresh, win("7d", 50, 2*h, 50)),
			},
			want: "CODEX,CLAUDE,PI,GROK",
		},
		{
			name: "already reset windows do not count",
			quotas: []*pb.ProviderQuota{
				pq("claude", fresh, win("5h", 10, -time.Minute, 90)),
				pq("codex", fresh, win("7d", 50, 10*h, 30)),
			},
			want: "CODEX,PI,CLAUDE,GROK",
		},
		{
			name: "no waste expected falls below the local fallback",
			quotas: []*pb.ProviderQuota{
				pq("claude", fresh, win("7d", 40, 100*h, 0)),
				pq("grok", fresh, win("weekly", 10, 100*h, 0.5)),
			},
			want: "PI,GROK,CLAUDE,CODEX",
		},
		{
			name:   "nothing monitored leaves the local fallback first",
			quotas: nil,
			want:   "PI,CLAUDE,CODEX,GROK",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := RankHarnesses(tt.quotas, tt.size, t0)
			if o := order(got); o != tt.want {
				for _, s := range got {
					t.Logf("%d %s rec=%v score=%.2f %s", s.Rank, s.Harness, s.Recommended, s.Score, s.Reason)
				}
				t.Fatalf("order = %s, want %s", o, tt.want)
			}
			for i, s := range got {
				if int(s.Rank) != i+1 {
					t.Errorf("rank %d at position %d", s.Rank, i)
				}
				if s.Reason == "" {
					t.Errorf("%s has no reason", s.Harness)
				}
				if !s.Recommended && s.Score != 0 {
					t.Errorf("%s not recommended but score %v", s.Harness, s.Score)
				}
			}
		})
	}
}

func TestRankHarnessesNeverCountsMissingDataAsSpare(t *testing.T) {
	got := RankHarnesses([]*pb.ProviderQuota{
		pq("claude", pb.Freshness_FRESHNESS_STALE, win("7d", 0, 10*time.Hour, 100)),
		pq("codex", pb.Freshness_FRESHNESS_UNKNOWN),
	}, pb.TaskSize_TASK_SIZE_SMALL, t0)
	for _, s := range got {
		if s.Harness == pb.Harness_HARNESS_PI {
			continue
		}
		if s.Recommended || s.UsablePercent != nil || s.ExpiringPercent != nil {
			t.Errorf("%s: stale/unknown data produced capacity: %+v", s.Harness, s)
		}
	}
}

func TestRankHarnessesLocalReason(t *testing.T) {
	local := &pb.ProviderQuota{Provider: "local", Freshness: fresh, Models: []string{"qwen3"}}
	got := RankHarnesses([]*pb.ProviderQuota{local}, 0, t0)
	if got[0].Harness != pb.Harness_HARNESS_PI || !strings.Contains(got[0].Reason, "qwen3") {
		t.Fatalf("local suggestion = %+v", got[0])
	}
	down := &pb.ProviderQuota{Provider: "local", Freshness: pb.Freshness_FRESHNESS_UNKNOWN, Error: "refused"}
	got = RankHarnesses([]*pb.ProviderQuota{down}, 0, t0)
	if !got[0].Recommended || got[0].Harness != pb.Harness_HARNESS_PI {
		t.Fatalf("local fallback must stay recommended: %+v", got[0])
	}
}

func TestRankHarnessesReasonText(t *testing.T) {
	got := RankHarnesses([]*pb.ProviderQuota{pq("codex", fresh, win("5h", 20, 90*time.Minute, 55), win("7d", 40, 100*time.Hour, 30))}, 0, t0)
	s := got[0]
	if s.Harness != pb.Harness_HARNESS_CODEX || s.Window != "5h" {
		t.Fatalf("top = %+v", s)
	}
	want := "5h window resets in 1h30m, ~55% expiring unused (projected); 60% usable now (7d)"
	if s.Reason != want {
		t.Errorf("reason = %q, want %q", s.Reason, want)
	}
}

func TestSuggestService(t *testing.T) {
	f := &fakeReader{name: "claude"}
	g := &fakeReader{name: "grok"}
	c, clk := newCache(t, f, g)
	f.set(good(clk, 30, 150*time.Minute), nil)
	g.set(quota.Snapshot{}, errors.New("credentials expired"))
	c.refresh(context.Background(), false)

	h := New("test", testToken)
	h.SetQuota(c)
	client := protectoratev1connect.NewQuotaServiceClient(&http.Client{Transport: bearer{testToken}}, newServer(t, h))
	res, err := client.Suggest(context.Background(), connect.NewRequest(&pb.SuggestRequest{TaskSize: pb.TaskSize_TASK_SIZE_SMALL}))
	if err != nil {
		t.Fatal(err)
	}
	if got := order(res.Msg.Suggestions); got != "CLAUDE,PI,CODEX,GROK" {
		t.Fatalf("order = %s", got)
	}
	if s := res.Msg.Suggestions[3]; s.Freshness != pb.Freshness_FRESHNESS_UNKNOWN || s.Recommended {
		t.Fatalf("grok = %v", s)
	}
}
