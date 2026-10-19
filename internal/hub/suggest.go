package hub

import (
	"context"
	"fmt"
	"sort"
	"strings"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

const (
	// localScore ranks the local LLM below any subscription with at least this
	// much weighted capacity expiring, and above everything not recommended.
	localScore = 1.0
	// unknownResetHours stands in for a window whose reset time is unknown.
	unknownResetHours = 168.0
	// urgencyHalfHours is the time to reset at which a window's weight halves.
	urgencyHalfHours = 48.0
)

type providerHarness struct {
	provider string
	harness  pb.Harness
}

var suggestable = []providerHarness{
	{"claude", pb.Harness_HARNESS_CLAUDE},
	{"codex", pb.Harness_HARNESS_CODEX},
	{"grok", pb.Harness_HARNESS_GROK},
}

func minUsable(s pb.TaskSize) float64 {
	switch s {
	case pb.TaskSize_TASK_SIZE_SMALL:
		return 5
	case pb.TaskSize_TASK_SIZE_LARGE:
		return 30
	}
	return 15
}

func sizeName(s pb.TaskSize) string {
	switch s {
	case pb.TaskSize_TASK_SIZE_SMALL:
		return "small"
	case pb.TaskSize_TASK_SIZE_LARGE:
		return "large"
	}
	return "medium"
}

func (h *Hub) Suggest(ctx context.Context, req *connect.Request[pb.SuggestRequest]) (*connect.Response[pb.SuggestResponse], error) {
	if req.Msg.Refresh {
		h.quota.Refresh(ctx)
	}
	now := h.quota.opts.Now()
	return connect.NewResponse(&pb.SuggestResponse{
		Suggestions: RankHarnesses(h.quota.Snapshot(), req.Msg.TaskSize, now),
		GeneratedAt: timestamppb.New(now),
	}), nil
}

// RankHarnesses scores each sleeve harness from the provider quotas.
func RankHarnesses(quotas []*pb.ProviderQuota, size pb.TaskSize, now time.Time) []*pb.HarnessSuggestion {
	byProvider := map[string]*pb.ProviderQuota{}
	for _, q := range quotas {
		if prev := byProvider[q.Provider]; prev == nil || (prev.Freshness != pb.Freshness_FRESHNESS_FRESH && q.Freshness == pb.Freshness_FRESHNESS_FRESH) {
			byProvider[q.Provider] = q
		}
	}
	var out []*pb.HarnessSuggestion
	for _, ph := range suggestable {
		s := &pb.HarnessSuggestion{Harness: ph.harness, Provider: ph.provider, Freshness: pb.Freshness_FRESHNESS_UNKNOWN}
		if q := byProvider[ph.provider]; q != nil {
			scoreProvider(s, q, size, now)
		} else {
			s.Reason = "not monitored: no quota reader for this provider"
		}
		out = append(out, s)
	}
	out = append(out, localSuggestion(byProvider["local"]))

	sort.SliceStable(out, func(i, j int) bool {
		a, b := out[i], out[j]
		if a.Recommended != b.Recommended {
			return a.Recommended
		}
		if a.Score != b.Score {
			return a.Score > b.Score
		}
		return a.Harness < b.Harness
	})
	for i, s := range out {
		s.Rank = int32(i + 1)
	}
	return out
}

func localSuggestion(q *pb.ProviderQuota) *pb.HarnessSuggestion {
	s := &pb.HarnessSuggestion{
		Harness: pb.Harness_HARNESS_PI, Provider: "local", Recommended: true, Score: localScore,
		Freshness: pb.Freshness_FRESHNESS_UNKNOWN,
	}
	switch {
	case q == nil:
		s.Reason = "local LLM via pi: no quota, always available fallback (no local endpoint monitored)"
	case q.Freshness == pb.Freshness_FRESHNESS_FRESH:
		s.Freshness = q.Freshness
		s.Reason = "local LLM via pi: no quota, reachable"
		if len(q.Models) > 0 {
			s.Reason += " (" + strings.Join(q.Models, ", ") + ")"
		}
	default:
		s.Freshness = q.Freshness
		s.Reason = "local LLM via pi: no quota, always available fallback (endpoint not confirmed reachable)"
	}
	return s
}

type windowScore struct {
	name      string
	expiring  float64
	hours     float64
	known     bool
	estimated bool
	score     float64
}

func scoreProvider(s *pb.HarnessSuggestion, q *pb.ProviderQuota, size pb.TaskSize, now time.Time) {
	s.Freshness = q.Freshness
	switch q.Freshness {
	case pb.Freshness_FRESHNESS_FRESH:
	case pb.Freshness_FRESHNESS_STALE:
		s.Reason = fmt.Sprintf("quota data is stale (%s old); not counted as spare", fmtAge(q.Age.AsDuration()))
		return
	default:
		s.Reason = "no quota data yet; not counted as spare"
		if q.Error != "" {
			s.Reason += " (" + q.Error + ")"
		}
		return
	}

	var cands []windowScore
	usable, tight := 100.0, ""
	for _, w := range q.Windows {
		if w.ResetsAt != nil && !w.ResetsAt.AsTime().After(now) {
			continue
		}
		remaining := min(max(100-w.UsedPercent, 0), 100)
		if tight == "" || remaining < usable {
			usable, tight = remaining, w.Name
		}
		ws := windowScore{name: w.Name, expiring: remaining, hours: unknownResetHours}
		if w.EstimatedUnusedPercentAtReset != nil {
			ws.expiring, ws.estimated = min(*w.EstimatedUnusedPercentAtReset, remaining), true
		}
		if w.ResetsAt != nil {
			ws.hours, ws.known = w.ResetsAt.AsTime().Sub(now).Hours(), true
		}
		cands = append(cands, ws)
	}
	if len(cands) == 0 {
		s.Reason = "no usable windows reported (none, or all already reset); wait for the next quota read"
		return
	}
	var best windowScore
	for i, c := range cands {
		c.expiring = min(c.expiring, usable)
		c.score = c.expiring / (1 + c.hours/urgencyHalfHours)
		if i == 0 || c.score > best.score {
			best = c
		}
	}
	s.UsablePercent, s.ExpiringPercent, s.Window = &usable, &best.expiring, best.name
	if best.known {
		s.ResetsIn = durationpb.New(time.Duration(best.hours * float64(time.Hour)))
	}

	if need := minUsable(size); usable < need {
		s.Reason = fmt.Sprintf("only %.0f%% usable now (%s window), below the %.0f%% a %s task needs", usable, tight, need, sizeName(size))
		return
	}
	s.Recommended, s.Score = true, best.score
	basis := "remaining, no pace estimate"
	if best.estimated {
		basis = "projected"
	}
	reset := "reset time unknown"
	if best.known {
		reset = "resets in " + fmtAge(time.Duration(best.hours*float64(time.Hour)))
	}
	s.Reason = fmt.Sprintf("%s window %s, ~%.0f%% expiring unused (%s); %.0f%% usable now (%s)", best.name, reset, best.expiring, basis, usable, tight)
}

func fmtAge(d time.Duration) string {
	d = d.Round(time.Minute)
	switch {
	case d < time.Hour:
		return fmt.Sprintf("%dm", int(d.Minutes()))
	case d < 48*time.Hour:
		return fmt.Sprintf("%dh%02dm", int(d.Hours()), int(d.Minutes())%60)
	}
	return fmt.Sprintf("%dd%02dh", int(d.Hours())/24, int(d.Hours())%24)
}
