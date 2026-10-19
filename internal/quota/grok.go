package quota

import (
	"context"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"net/http"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

const (
	grokDefaultProxy = "https://cli-chat-proxy.grok.com"
	grokDefaultWeb   = "https://grok.com"
)

type Grok struct {
	Home     string
	BaseURL  string
	WebURL   string
	Client   *http.Client
	Now      func() time.Time
	NoResets bool
}

func NewGrok() *Grok { return &Grok{BaseURL: grokDefaultProxy, WebURL: grokDefaultWeb} }

func (g *Grok) Provider() string { return "grok" }

type grokAuthEntry struct {
	Key       string `json:"key"`
	ExpiresAt string `json:"expires_at"`
}

type grokAmount struct {
	Val json.Number `json:"val"`
}

func (a *grokAmount) float() (float64, bool) {
	if a == nil || a.Val == "" {
		return 0, false
	}
	f, err := a.Val.Float64()
	return f, err == nil
}

type grokBilling struct {
	SubscriptionTier string `json:"subscriptionTier"`
	Config           *struct {
		CreditUsagePercent *float64 `json:"creditUsagePercent"`
		CurrentPeriod      *struct {
			Start string `json:"start"`
			End   string `json:"end"`
		} `json:"currentPeriod"`
		BillingPeriodStart string      `json:"billingPeriodStart"`
		BillingPeriodEnd   string      `json:"billingPeriodEnd"`
		OnDemandCap        *grokAmount `json:"onDemandCap"`
		OnDemandUsed       *grokAmount `json:"onDemandUsed"`
		SubscriptionTier   string      `json:"subscriptionTier"`
	} `json:"config"`
}

func pickGrokEntry(raw []byte) (grokAuthEntry, error) {
	var all map[string]json.RawMessage
	if err := json.Unmarshal(raw, &all); err != nil {
		return grokAuthEntry{}, fmt.Errorf("decode credentials: %w", err)
	}
	keys := make([]string, 0, len(all))
	for k := range all {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, prefix := range []string{"https://auth.x.ai::", "https://accounts.x.ai/sign-in"} {
		for _, k := range keys {
			if !strings.HasPrefix(k, prefix) {
				continue
			}
			var e grokAuthEntry
			if json.Unmarshal(all[k], &e) == nil && e.Key != "" {
				return e, nil
			}
		}
	}
	return grokAuthEntry{}, fmt.Errorf("%w: no Grok login entry", ErrUnavailable)
}

func (g *Grok) Read(ctx context.Context) (Snapshot, error) {
	snap := Snapshot{Provider: "grok", Source: "cli-proxy-billing"}
	home, err := homeDir(g.Home)
	if err != nil {
		return snap, err
	}
	raw, err := readCredFile(filepath.Join(home, ".grok", "auth.json"))
	if err != nil {
		return snap, err
	}
	entry, err := pickGrokEntry(raw)
	if err != nil {
		return snap, err
	}
	now := nowFn(g.Now)
	if exp := parseTime(entry.ExpiresAt); !exp.IsZero() && !now.Before(exp) {
		return snap, ErrExpired
	}

	hdr := map[string]string{
		"Authorization":    "Bearer " + entry.Key,
		"x-xai-token-auth": "xai-grok-cli",
		"Accept":           "application/json",
	}
	base := strings.TrimRight(g.BaseURL, "/")
	if base == "" {
		base = grokDefaultProxy
	}
	var b grokBilling
	if err := getJSON(ctx, g.Client, base+"/v1/billing?format=credits", hdr, &b); err != nil {
		return snap, err
	}
	snap.FetchedAt = now
	snap.Plan = b.SubscriptionTier

	if cfg := b.Config; cfg != nil {
		if snap.Plan == "" {
			snap.Plan = cfg.SubscriptionTier
		}
		start, end := parseTime(cfg.BillingPeriodStart), parseTime(cfg.BillingPeriodEnd)
		if cp := cfg.CurrentPeriod; cp != nil {
			if e := parseTime(cp.End); !e.IsZero() {
				start, end = parseTime(cp.Start), e
			}
		}
		var pct float64
		known := false
		if cfg.CreditUsagePercent != nil {
			pct, known = *cfg.CreditUsagePercent, true
		} else if used, ok := cfg.OnDemandUsed.float(); ok {
			if capv, ok := cfg.OnDemandCap.float(); ok && capv > 0 {
				pct, known = used/capv*100, true
			}
		}
		if known {
			snap.Windows = append(snap.Windows, Window{Name: grokWindowName(start, end), UsedPercent: clampPercent(pct), ResetsAt: end})
		}
	}

	if snap.Plan == "" {
		var s struct {
			Tier string `json:"subscription_tier_display"`
		}
		if getJSON(ctx, g.Client, base+"/v1/settings", hdr, &s) == nil {
			snap.Plan = s.Tier
		}
	}
	if !g.NoResets {
		snap.Resets = g.remainingResets(ctx, entry.Key, now)
	}
	return snap, nil
}

func grokWindowName(start, end time.Time) string {
	if start.IsZero() || end.IsZero() || !end.After(start) {
		return "credits"
	}
	days := end.Sub(start).Hours() / 24
	switch {
	case days >= 6 && days <= 8:
		return "weekly"
	case days >= 27 && days <= 32:
		return "monthly"
	}
	return "credits"
}

// remainingResets calls the read-only GetRemainingResets listing. Failures
// yield no resets rather than failing the usage read. Nothing is redeemed.
func (g *Grok) remainingResets(ctx context.Context, token string, now time.Time) []Reset {
	web := strings.TrimRight(g.WebURL, "/")
	if web == "" {
		web = grokDefaultWeb
	}
	data, err := doRaw(ctx, g.Client, http.MethodPost, web+"/prod_mc_billing.ConsumerUiSvc/GetRemainingResets", map[string]string{
		"Authorization": "Bearer " + token,
		"Origin":        grokDefaultWeb,
		"Accept":        "*/*",
		"Content-Type":  "application/grpc-web+proto",
		"x-grpc-web":    "1",
	}, []byte{0, 0, 0, 0, 0})
	if err != nil {
		return nil
	}
	var out []Reset
	for _, exp := range parseGrokResets(data) {
		if exp.After(now) {
			out = append(out, Reset{Kind: "usage-limit", Count: 1, ExpiresAt: exp})
		}
	}
	sort.SliceStable(out, func(i, j int) bool { return out[i].ExpiresAt.Before(out[j].ExpiresAt) })
	return out
}

type pbField struct {
	num    int
	varint uint64
	data   []byte
	isMsg  bool
}

func pbParse(b []byte) ([]pbField, bool) {
	var fs []pbField
	for len(b) > 0 {
		tag, n := binary.Uvarint(b)
		if n <= 0 {
			return nil, false
		}
		b = b[n:]
		f := pbField{num: int(tag >> 3)}
		switch tag & 7 {
		case 0:
			v, n := binary.Uvarint(b)
			if n <= 0 {
				return nil, false
			}
			f.varint, b = v, b[n:]
		case 1:
			if len(b) < 8 {
				return nil, false
			}
			b = b[8:]
		case 2:
			l, n := binary.Uvarint(b)
			if n <= 0 || l > uint64(len(b)-n) {
				return nil, false
			}
			f.data, f.isMsg = b[n:n+int(l)], true
			b = b[n+int(l):]
		case 5:
			if len(b) < 4 {
				return nil, false
			}
			b = b[4:]
		default:
			return nil, false
		}
		fs = append(fs, f)
	}
	return fs, true
}

func grpcWebDataFrames(b []byte) [][]byte {
	var frames [][]byte
	for len(b) >= 5 {
		flag, l := b[0], int(binary.BigEndian.Uint32(b[1:5]))
		if flag&0x7f != 0 || l < 0 || l > len(b)-5 {
			return nil
		}
		if flag == 0 && l > 0 {
			frames = append(frames, b[5:5+l])
		}
		b = b[5+l:]
	}
	return frames
}

// parseGrokResets extracts expiry times from the record layout CodexBar
// documents: field 10 = reset record { 10 = id, 30 = Timestamp{1 = seconds} }.
func parseGrokResets(data []byte) []time.Time {
	payloads := grpcWebDataFrames(data)
	if len(payloads) == 0 {
		payloads = [][]byte{data}
	}
	var out []time.Time
	for _, p := range payloads {
		top, ok := pbParse(p)
		if !ok {
			continue
		}
		for _, f := range top {
			if f.num != 10 || !f.isMsg {
				continue
			}
			rec, ok := pbParse(f.data)
			if !ok {
				continue
			}
			for _, rf := range rec {
				if rf.num != 30 || !rf.isMsg {
					continue
				}
				ts, ok := pbParse(rf.data)
				if !ok {
					continue
				}
				for _, tf := range ts {
					if tf.num == 1 && !tf.isMsg && tf.varint >= 1_700_000_000 && tf.varint <= 2_100_000_000 {
						out = append(out, time.Unix(int64(tf.varint), 0).UTC())
					}
				}
			}
		}
	}
	return out
}
