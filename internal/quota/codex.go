package quota

import (
	"bufio"
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

const codexDefaultBase = "https://chatgpt.com/backend-api"

type Codex struct {
	Home    string
	Bin     string
	BaseURL string
	Client  *http.Client
	Now     func() time.Time
}

func NewCodex() *Codex { return &Codex{Bin: "codex", BaseURL: codexDefaultBase} }

func (c *Codex) Provider() string { return "codex" }

type codexAuth struct {
	Tokens struct {
		AccessToken string `json:"access_token"`
		AccountID   string `json:"account_id"`
	} `json:"tokens"`
}

type rpcWindow struct {
	UsedPercent        float64 `json:"usedPercent"`
	WindowDurationMins *int    `json:"windowDurationMins"`
	ResetsAt           *int64  `json:"resetsAt"`
}

type rpcSnapshot struct {
	LimitID   string     `json:"limitId"`
	LimitName string     `json:"limitName"`
	Primary   *rpcWindow `json:"primary"`
	Secondary *rpcWindow `json:"secondary"`
	PlanType  string     `json:"planType"`
}

type rpcLimits struct {
	RateLimits         rpcSnapshot            `json:"rateLimits"`
	RateLimitsByLimit  map[string]rpcSnapshot `json:"rateLimitsByLimitId"`
	RateLimitsByLimit2 map[string]rpcSnapshot `json:"rate_limits_by_limit_id"`
}

type whamWindow struct {
	UsedPercent        float64 `json:"used_percent"`
	LimitWindowSeconds int     `json:"limit_window_seconds"`
	ResetAt            int64   `json:"reset_at"`
}

type whamUsage struct {
	PlanType  string `json:"plan_type"`
	RateLimit *struct {
		Primary   *whamWindow `json:"primary_window"`
		Secondary *whamWindow `json:"secondary_window"`
	} `json:"rate_limit"`
}

type whamResetCredits struct {
	Credits []struct {
		ResetType string `json:"reset_type"`
		Status    string `json:"status"`
		ExpiresAt string `json:"expires_at"`
	} `json:"credits"`
}

func (c *Codex) Read(ctx context.Context) (Snapshot, error) {
	snap := Snapshot{Provider: "codex"}
	home, err := homeDir(c.Home)
	if err != nil {
		return snap, err
	}
	raw, err := readCredFile(filepath.Join(home, ".codex", "auth.json"))
	if err != nil {
		return snap, err
	}
	var auth codexAuth
	if err := json.Unmarshal(raw, &auth); err != nil {
		return snap, fmt.Errorf("decode credentials: %w", err)
	}
	if auth.Tokens.AccessToken == "" {
		return snap, fmt.Errorf("%w: no OAuth access token", ErrUnavailable)
	}
	now := nowFn(c.Now)
	// The codex CLI refreshes tokens on its own; refuse to launch it with a
	// stale token so a quota read can never trigger or rewrite a login.
	if exp, ok := jwtExpiry(auth.Tokens.AccessToken); ok && !now.Before(exp) {
		return snap, ErrExpired
	}

	rl, rpcErr := c.rpcRateLimits(ctx)
	if rpcErr == nil {
		applyRPCLimits(&snap, rl)
	} else {
		var u whamUsage
		if err := getJSON(ctx, c.Client, c.base()+"/wham/usage", c.headers(auth), &u); err != nil {
			if errors.Is(err, ErrUnauthorized) {
				return snap, err
			}
			return snap, fmt.Errorf("app-server: %w; wham fallback: %w", rpcErr, err)
		}
		snap.Source = "wham-usage"
		snap.Plan = u.PlanType
		addWhamWindows(&snap, u)
	}
	snap.FetchedAt = now

	var rc whamResetCredits
	if err := getJSON(ctx, c.Client, c.base()+"/wham/rate-limit-reset-credits", c.headers(auth), &rc); err == nil {
		for _, cr := range rc.Credits {
			exp := parseTime(cr.ExpiresAt)
			if cr.Status != "available" || (!exp.IsZero() && !exp.After(now)) {
				continue
			}
			snap.Resets = append(snap.Resets, Reset{Kind: cr.ResetType, Count: 1, ExpiresAt: exp})
		}
		sort.SliceStable(snap.Resets, func(i, j int) bool { return snap.Resets[i].ExpiresAt.Before(snap.Resets[j].ExpiresAt) })
	}
	return snap, nil
}

func applyRPCLimits(snap *Snapshot, rl rpcLimits) {
	snap.Source, snap.Plan = "codex-app-server", rl.RateLimits.PlanType
	addRPCWindows(snap, "", rl.RateLimits)
	by := rl.RateLimitsByLimit
	if by == nil {
		by = rl.RateLimitsByLimit2
	}
	keys := make([]string, 0, len(by))
	for k := range by {
		if k != "codex" && k != rl.RateLimits.LimitID {
			keys = append(keys, k)
		}
	}
	sort.Strings(keys)
	for _, k := range keys {
		s := by[k]
		label := s.LimitName
		if label == "" {
			label = k
		}
		addRPCWindows(snap, label+" ", s)
	}
}

func addWhamWindows(snap *Snapshot, u whamUsage) {
	if u.RateLimit == nil {
		return
	}
	for _, w := range []*whamWindow{u.RateLimit.Primary, u.RateLimit.Secondary} {
		if w == nil {
			continue
		}
		snap.Windows = append(snap.Windows, Window{
			Name: windowLabel(w.LimitWindowSeconds / 60), UsedPercent: clampPercent(w.UsedPercent),
			ResetsAt: time.Unix(w.ResetAt, 0).UTC(),
		})
	}
}

func (c *Codex) base() string {
	if c.BaseURL == "" {
		return codexDefaultBase
	}
	return strings.TrimRight(c.BaseURL, "/")
}

func (c *Codex) headers(a codexAuth) map[string]string {
	h := map[string]string{
		"Authorization": "Bearer " + a.Tokens.AccessToken,
		"Accept":        "application/json",
		"User-Agent":    "protectorate",
	}
	if a.Tokens.AccountID != "" {
		h["ChatGPT-Account-Id"] = a.Tokens.AccountID
	}
	return h
}

func addRPCWindows(snap *Snapshot, prefix string, s rpcSnapshot) {
	for _, w := range []*rpcWindow{s.Primary, s.Secondary} {
		if w == nil {
			continue
		}
		mins := 0
		if w.WindowDurationMins != nil {
			mins = *w.WindowDurationMins
		}
		win := Window{Name: prefix + windowLabel(mins), UsedPercent: clampPercent(w.UsedPercent)}
		if w.ResetsAt != nil {
			win.ResetsAt = time.Unix(*w.ResetsAt, 0).UTC()
		}
		snap.Windows = append(snap.Windows, win)
	}
}

func windowLabel(mins int) string {
	switch {
	case mins <= 0:
		return "window"
	case mins == 300:
		return "5h"
	case mins == 10080:
		return "7d"
	case mins%1440 == 0:
		return fmt.Sprintf("%dd", mins/1440)
	case mins%60 == 0:
		return fmt.Sprintf("%dh", mins/60)
	}
	return fmt.Sprintf("%dm", mins)
}

func jwtExpiry(tok string) (time.Time, bool) {
	parts := strings.Split(tok, ".")
	if len(parts) != 3 {
		return time.Time{}, false
	}
	b, err := base64.RawURLEncoding.DecodeString(strings.TrimRight(parts[1], "="))
	if err != nil {
		return time.Time{}, false
	}
	var claims struct {
		Exp int64 `json:"exp"`
	}
	if json.Unmarshal(b, &claims) != nil || claims.Exp == 0 {
		return time.Time{}, false
	}
	return time.Unix(claims.Exp, 0), true
}

type rpcMessage struct {
	ID     *int            `json:"id"`
	Result json.RawMessage `json:"result"`
	Error  *struct {
		Code int `json:"code"`
	} `json:"error"`
}

// rpcRateLimits drives `codex app-server` over newline-delimited JSON-RPC and
// always closes stdin and reaps the process before returning.
func (c *Codex) rpcRateLimits(ctx context.Context) (rpcLimits, error) {
	var out rpcLimits
	bin := c.Bin
	if bin == "" {
		bin = "codex"
	}
	p, err := startQuotaRPC(ctx, bin)
	if err != nil {
		return out, err
	}
	defer p.close()
	if _, err := p.call(ctx, 1, "initialize", map[string]any{"clientInfo": map[string]string{"name": "protectorate", "version": "0"}}); err != nil {
		return out, err
	}
	if err := writeLine(p.stdin, map[string]any{"method": "initialized", "params": map[string]any{}}); err != nil {
		return out, err
	}
	res, err := p.call(ctx, 2, "account/rateLimits/read", nil)
	if err != nil {
		return out, err
	}
	if err := json.Unmarshal(res, &out); err != nil {
		return out, fmt.Errorf("decode rate limits: %w", err)
	}
	return out, nil
}

type quotaRPC struct {
	cmd   *exec.Cmd
	stdin io.WriteCloser
	lines <-chan []byte
	stop  chan struct{}
}

func startQuotaRPC(ctx context.Context, bin string) (*quotaRPC, error) {
	cmd := exec.CommandContext(ctx, bin, "-s", "read-only", "-a", "never", "app-server")
	configureProc(cmd)
	stdin, err := cmd.StdinPipe()
	if err != nil {
		return nil, err
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return nil, err
	}
	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("%w: cannot start codex", ErrUnavailable)
	}

	lines := make(chan []byte)
	stop := make(chan struct{})
	go func() {
		defer close(lines)
		sc := bufio.NewScanner(stdout)
		sc.Buffer(make([]byte, 0, 64<<10), 4<<20)
		for sc.Scan() {
			b := append([]byte(nil), sc.Bytes()...)
			select {
			case lines <- b:
			case <-stop:
				return
			}
		}
	}()
	return &quotaRPC{cmd: cmd, stdin: stdin, lines: lines, stop: stop}, nil
}

func (p *quotaRPC) close() {
	close(p.stop)
	_ = p.stdin.Close()
	done := make(chan struct{})
	go func() {
		for range p.lines {
		}
		_ = p.cmd.Wait()
		close(done)
	}()
	timer := time.NewTimer(3 * time.Second)
	defer timer.Stop()
	select {
	case <-done:
	case <-timer.C:
		killProc(p.cmd)
		<-done
	}
}

func (p *quotaRPC) call(ctx context.Context, id int, method string, params any) (json.RawMessage, error) {
	msg := map[string]any{"id": id, "method": method}
	if params != nil {
		msg["params"] = params
	}
	if err := writeLine(p.stdin, msg); err != nil {
		return nil, err
	}
	for {
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case line, ok := <-p.lines:
			if !ok {
				return nil, errors.New("codex app-server exited early")
			}
			var m rpcMessage
			if json.Unmarshal(line, &m) != nil || m.ID == nil || *m.ID != id {
				continue
			}
			if m.Error != nil {
				return nil, fmt.Errorf("codex app-server error code %d", m.Error.Code)
			}
			return m.Result, nil
		}
	}
}

func writeLine(w io.Writer, v any) error {
	b, err := json.Marshal(v)
	if err != nil {
		return err
	}
	_, err = w.Write(append(b, '\n'))
	return err
}
