package quota

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"path/filepath"
	"time"
)

const claudeDefaultBase = "https://api.anthropic.com"

type Claude struct {
	Home    string
	BaseURL string
	Client  *http.Client
	Now     func() time.Time
}

func NewClaude() *Claude { return &Claude{BaseURL: claudeDefaultBase} }

func (c *Claude) Provider() string { return "claude" }

type claudeCreds struct {
	OAuth struct {
		AccessToken      string `json:"accessToken"`
		ExpiresAt        int64  `json:"expiresAt"`
		SubscriptionType string `json:"subscriptionType"`
	} `json:"claudeAiOauth"`
}

type claudeWindow struct {
	Utilization *float64 `json:"utilization"`
	ResetsAt    string   `json:"resets_at"`
}

type claudeUsage struct {
	FiveHour       *claudeWindow `json:"five_hour"`
	SevenDay       *claudeWindow `json:"seven_day"`
	SevenDaySonnet *claudeWindow `json:"seven_day_sonnet"`
	SevenDayOpus   *claudeWindow `json:"seven_day_opus"`
	ExtraUsage     *struct {
		IsEnabled   bool     `json:"is_enabled"`
		Utilization *float64 `json:"utilization"`
	} `json:"extra_usage"`
	Limits []struct {
		Kind     string   `json:"kind"`
		Percent  *float64 `json:"percent"`
		ResetsAt string   `json:"resets_at"`
		Scope    *struct {
			Model *struct {
				DisplayName string `json:"display_name"`
			} `json:"model"`
		} `json:"scope"`
	} `json:"limits"`
	CedarEmber *struct {
		Eligible bool `json:"eligible"`
		Grants   []struct {
			ResetsLeft int    `json:"resets_left"`
			StartsAt   string `json:"starts_at"`
			EndsAt     string `json:"ends_at"`
			Paused     bool   `json:"paused"`
		} `json:"grants"`
	} `json:"cedar_ember"`
}

func (c *Claude) Read(ctx context.Context) (Snapshot, error) {
	snap := Snapshot{Provider: "claude", Source: "oauth-usage"}
	home, err := homeDir(c.Home)
	if err != nil {
		return snap, err
	}
	raw, err := readCredFile(filepath.Join(home, ".claude", ".credentials.json"))
	if err != nil {
		return snap, err
	}
	var cr claudeCreds
	if err := json.Unmarshal(raw, &cr); err != nil {
		return snap, fmt.Errorf("decode credentials: %w", err)
	}
	now := nowFn(c.Now)
	if cr.OAuth.AccessToken == "" {
		return snap, fmt.Errorf("%w: no OAuth access token", ErrUnavailable)
	}
	if cr.OAuth.ExpiresAt > 0 && !now.Before(time.UnixMilli(cr.OAuth.ExpiresAt)) {
		return snap, ErrExpired
	}
	snap.Plan = cr.OAuth.SubscriptionType

	base := c.BaseURL
	if base == "" {
		base = claudeDefaultBase
	}
	var u claudeUsage
	err = getJSON(ctx, c.Client, base+"/api/oauth/usage?cedar_ember=1", map[string]string{
		"Authorization":  "Bearer " + cr.OAuth.AccessToken,
		"anthropic-beta": "oauth-2025-04-20",
		"Accept":         "application/json",
	}, &u)
	if err != nil {
		return snap, err
	}
	snap.FetchedAt = now

	seen := map[string]bool{}
	add := func(name string, w *claudeWindow) {
		if w == nil || w.Utilization == nil || seen[name] {
			return
		}
		seen[name] = true
		snap.Windows = append(snap.Windows, Window{Name: name, UsedPercent: clampPercent(*w.Utilization), ResetsAt: parseTime(w.ResetsAt)})
	}
	add("5h", u.FiveHour)
	add("7d", u.SevenDay)
	add("7d sonnet", u.SevenDaySonnet)
	add("7d opus", u.SevenDayOpus)
	for _, l := range u.Limits {
		if l.Kind != "weekly_scoped" || l.Scope == nil || l.Scope.Model == nil || l.Scope.Model.DisplayName == "" {
			continue
		}
		add("7d "+l.Scope.Model.DisplayName, &claudeWindow{Utilization: l.Percent, ResetsAt: l.ResetsAt})
	}
	if x := u.ExtraUsage; x != nil && x.IsEnabled {
		add("extra", &claudeWindow{Utilization: x.Utilization})
	}
	if ce := u.CedarEmber; ce != nil && ce.Eligible {
		for _, g := range ce.Grants {
			if g.Paused || g.ResetsLeft <= 0 {
				continue
			}
			if s := parseTime(g.StartsAt); !s.IsZero() && s.After(now) {
				continue
			}
			end := parseTime(g.EndsAt)
			if !end.IsZero() && !end.After(now) {
				continue
			}
			snap.Resets = append(snap.Resets, Reset{Kind: "usage-limit", Count: g.ResetsLeft, ExpiresAt: end})
		}
	}
	return snap, nil
}
