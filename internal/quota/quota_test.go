package quota

import (
	"context"
	"encoding/base64"
	"encoding/binary"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
	"time"
)

var testNow = time.Date(2026, 10, 19, 8, 0, 0, 0, time.UTC)

func writeHomeFile(t *testing.T, home, rel, content string) {
	t.Helper()
	p := filepath.Join(home, rel)
	if err := os.MkdirAll(filepath.Dir(p), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(p, []byte(content), 0o600); err != nil {
		t.Fatal(err)
	}
}

func fakeJWT(exp time.Time) string {
	enc := base64.RawURLEncoding.EncodeToString
	return enc([]byte(`{"alg":"none"}`)) + "." + enc([]byte(fmt.Sprintf(`{"exp":%d}`, exp.Unix()))) + ".sig"
}

const claudeUsageFixture = `{
 "five_hour":{"utilization":11.0,"resets_at":"2026-10-19T10:00:00.267071+00:00"},
 "seven_day":{"utilization":34.0,"resets_at":"2026-10-23T11:00:00.267095+00:00"},
 "seven_day_sonnet":null,
 "extra_usage":{"is_enabled":false,"utilization":0.0},
 "limits":[
  {"kind":"session","percent":11,"resets_at":"2026-10-19T10:00:00+00:00","scope":null},
  {"kind":"weekly_scoped","percent":15,"resets_at":"2026-10-23T11:00:00+00:00","scope":{"model":{"display_name":"Fable"}}}
 ],
 "cedar_ember":{"eligible":true,"grants":[
  {"resets_left":2,"starts_at":"2026-10-01T00:00:00Z","ends_at":"2026-11-01T00:00:00Z","paused":false},
  {"resets_left":1,"ends_at":"2026-12-01T00:00:00Z","paused":true},
  {"resets_left":1,"ends_at":"2026-10-01T00:00:00Z","paused":false}
 ]}}`

func claudeCredJSON(expMs int64) string {
	return fmt.Sprintf(`{"claudeAiOauth":{"accessToken":"tok","expiresAt":%d,"scopes":["user:profile"],"subscriptionType":"max"}}`, expMs)
}

func TestClaude(t *testing.T) {
	tests := []struct {
		name    string
		creds   string
		status  int
		body    string
		wantErr error
		windows int
		resets  int
	}{
		{"ok", claudeCredJSON(testNow.Add(time.Hour).UnixMilli()), 200, claudeUsageFixture, nil, 3, 1},
		{"expired", claudeCredJSON(testNow.Add(-time.Hour).UnixMilli()), 200, claudeUsageFixture, ErrExpired, 0, 0},
		{"unauthorized", claudeCredJSON(testNow.Add(time.Hour).UnixMilli()), 401, `{}`, ErrUnauthorized, 0, 0},
		{"scope", claudeCredJSON(testNow.Add(time.Hour).UnixMilli()), 403, `{"error":{"message":"missing scope user:profile"}}`, ErrScope, 0, 0},
		{"missing file", "", 0, "", ErrUnavailable, 0, 0},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var gotAuth, gotBeta string
			srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				gotAuth, gotBeta = r.Header.Get("Authorization"), r.Header.Get("anthropic-beta")
				w.WriteHeader(tt.status)
				_, _ = w.Write([]byte(tt.body))
			}))
			defer srv.Close()
			home := t.TempDir()
			if tt.creds != "" {
				writeHomeFile(t, home, ".claude/.credentials.json", tt.creds)
			}
			c := &Claude{Home: home, BaseURL: srv.URL, Now: func() time.Time { return testNow }}
			snap, err := c.Read(context.Background())
			if !errors.Is(err, tt.wantErr) {
				t.Fatalf("err = %v, want %v", err, tt.wantErr)
			}
			if len(snap.Windows) != tt.windows || len(snap.Resets) != tt.resets {
				t.Fatalf("windows=%d resets=%d, want %d/%d: %+v", len(snap.Windows), len(snap.Resets), tt.windows, tt.resets, snap)
			}
			if tt.name == "ok" {
				if gotAuth != "Bearer tok" || gotBeta != "oauth-2025-04-20" {
					t.Fatalf("bad headers %q %q", gotAuth, gotBeta)
				}
				if snap.Plan != "max" || snap.Windows[0].Name != "5h" || snap.Windows[0].UsedPercent != 11 ||
					snap.Windows[2].Name != "7d Fable" || snap.Resets[0].Count != 2 {
					t.Fatalf("unexpected snapshot %+v", snap)
				}
			}
		})
	}
}

const rpcScript = `#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *'"initialize"'*) echo '{"id":1,"result":{"userAgent":"fake"}}';;
    *'rateLimits/read'*) echo '{"method":"noise","params":{}}'; echo '{"id":2,"result":{"rateLimits":{"limitId":"codex","primary":{"usedPercent":42,"windowDurationMins":300,"resetsAt":1792000000},"secondary":{"usedPercent":7,"windowDurationMins":10080,"resetsAt":1792500000},"planType":"pro"},"rateLimitsByLimitId":{"codex":{"limitId":"codex"},"spark":{"limitId":"spark","limitName":"Spark","primary":{"usedPercent":1,"windowDurationMins":300}}}}}';;
  esac
done
`

const resetCreditsFixture = `{"credits":[
 {"reset_type":"weekly","status":"available","expires_at":"2026-11-06T23:24:22Z"},
 {"reset_type":"weekly","status":"available","expires_at":"2026-10-29T18:52:40Z"},
 {"reset_type":"weekly","status":"redeemed","expires_at":"2026-11-06T23:24:22Z"},
 {"reset_type":"weekly","status":"available","expires_at":"2026-10-01T00:00:00Z"}],"available_count":2}`

func TestCodexAppServer(t *testing.T) {
	home := t.TempDir()
	writeHomeFile(t, home, ".codex/auth.json", fmt.Sprintf(`{"tokens":{"access_token":%q,"account_id":"acct"}}`, fakeJWT(testNow.Add(time.Hour))))
	bin := filepath.Join(t.TempDir(), "codex")
	if err := os.WriteFile(bin, []byte(rpcScript), 0o700); err != nil {
		t.Fatal(err)
	}
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") == "" || r.Header.Get("ChatGPT-Account-Id") != "acct" {
			w.WriteHeader(401)
			return
		}
		if r.URL.Path == "/wham/rate-limit-reset-credits" {
			_, _ = w.Write([]byte(resetCreditsFixture))
			return
		}
		w.WriteHeader(404)
	}))
	defer srv.Close()
	c := &Codex{Home: home, Bin: bin, BaseURL: srv.URL, Now: func() time.Time { return testNow }}
	snap, err := c.Read(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if snap.Source != "codex-app-server" || snap.Plan != "pro" || len(snap.Windows) != 3 {
		t.Fatalf("unexpected %+v", snap)
	}
	if snap.Windows[0].Name != "5h" || snap.Windows[0].UsedPercent != 42 || snap.Windows[1].Name != "7d" || snap.Windows[2].Name != "Spark 5h" {
		t.Fatalf("windows %+v", snap.Windows)
	}
	if len(snap.Resets) != 2 || !snap.Resets[0].ExpiresAt.Before(snap.Resets[1].ExpiresAt) {
		t.Fatalf("resets %+v", snap.Resets)
	}
}

func TestCodexWhamFallbackAndExpiry(t *testing.T) {
	const wham = `{"plan_type":"plus","rate_limit":{"primary_window":{"used_percent":12.5,"limit_window_seconds":604800,"reset_at":1793000992},"secondary_window":null}}`
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/wham/usage" {
			_, _ = w.Write([]byte(wham))
			return
		}
		w.WriteHeader(404)
	}))
	defer srv.Close()

	home := t.TempDir()
	writeHomeFile(t, home, ".codex/auth.json", fmt.Sprintf(`{"tokens":{"access_token":%q}}`, fakeJWT(testNow.Add(time.Hour))))
	c := &Codex{Home: home, Bin: filepath.Join(t.TempDir(), "missing"), BaseURL: srv.URL, Now: func() time.Time { return testNow }}
	snap, err := c.Read(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if snap.Source != "wham-usage" || snap.Plan != "plus" || len(snap.Windows) != 1 || snap.Windows[0].Name != "7d" || snap.Windows[0].UsedPercent != 12.5 {
		t.Fatalf("unexpected %+v", snap)
	}

	exp := t.TempDir()
	writeHomeFile(t, exp, ".codex/auth.json", fmt.Sprintf(`{"tokens":{"access_token":%q}}`, fakeJWT(testNow.Add(-time.Hour))))
	c = &Codex{Home: exp, Bin: "never-launched", BaseURL: srv.URL, Now: func() time.Time { return testNow }}
	if _, err := c.Read(context.Background()); !errors.Is(err, ErrExpired) {
		t.Fatalf("err = %v, want ErrExpired", err)
	}
}

func grokAuth(exp time.Time) string {
	return fmt.Sprintf(`{"https://auth.x.ai::client":{"key":"tok","expires_at":%q}}`, exp.Format(time.RFC3339Nano))
}

func pbMsg(num int, payload []byte) []byte {
	out := binary.AppendUvarint(nil, uint64(num<<3|2))
	out = binary.AppendUvarint(out, uint64(len(payload)))
	return append(out, payload...)
}

func grokResetFrame(expires ...time.Time) []byte {
	var body []byte
	for _, e := range expires {
		ts := append(binary.AppendUvarint(nil, 1<<3), binary.AppendUvarint(nil, uint64(e.Unix()))...)
		rec := append(pbMsg(10, []byte("id")), pbMsg(30, ts)...)
		body = append(body, pbMsg(10, rec)...)
	}
	frame := []byte{0, 0, 0, 0, 0}
	binary.BigEndian.PutUint32(frame[1:], uint32(len(body)))
	return append(frame, body...)
}

func TestGrok(t *testing.T) {
	const billing = `{"subscriptionTier":"SuperGrok","config":{"creditUsagePercent":23.5,"currentPeriod":{"start":"2026-10-12T00:00:00Z","end":"2026-10-19T00:00:00Z"}}}`
	const onDemand = `{"config":{"onDemandUsed":{"val":2500},"onDemandCap":{"val":10000},"billingPeriodStart":"2026-10-01T00:00:00Z","billingPeriodEnd":"2026-11-01T00:00:00Z"}}`
	tests := []struct {
		name    string
		auth    string
		status  int
		body    string
		wantErr error
		pct     float64
		window  string
	}{
		{"percent", grokAuth(testNow.Add(time.Hour)), 200, billing, nil, 23.5, "weekly"},
		{"on-demand ratio", grokAuth(testNow.Add(time.Hour)), 200, onDemand, nil, 25, "monthly"},
		{"expired", grokAuth(testNow.Add(-time.Hour)), 200, billing, ErrExpired, 0, ""},
		{"unauthorized", grokAuth(testNow.Add(time.Hour)), 401, `{}`, ErrUnauthorized, 0, ""},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var resetCalls int
			srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				switch r.URL.Path {
				case "/v1/billing":
					if r.Header.Get("x-xai-token-auth") != "xai-grok-cli" || r.URL.Query().Get("format") != "credits" {
						w.WriteHeader(400)
						return
					}
					w.WriteHeader(tt.status)
					_, _ = w.Write([]byte(tt.body))
				case "/prod_mc_billing.ConsumerUiSvc/GetRemainingResets":
					resetCalls++
					_, _ = w.Write(grokResetFrame(testNow.Add(48*time.Hour), testNow.Add(-time.Hour)))
				default:
					w.WriteHeader(404)
				}
			}))
			defer srv.Close()
			home := t.TempDir()
			writeHomeFile(t, home, ".grok/auth.json", tt.auth)
			g := &Grok{Home: home, BaseURL: srv.URL, WebURL: srv.URL, Now: func() time.Time { return testNow }}
			snap, err := g.Read(context.Background())
			if !errors.Is(err, tt.wantErr) {
				t.Fatalf("err = %v, want %v", err, tt.wantErr)
			}
			if tt.wantErr != nil {
				return
			}
			if len(snap.Windows) != 1 || snap.Windows[0].UsedPercent != tt.pct || snap.Windows[0].Name != tt.window {
				t.Fatalf("windows %+v", snap.Windows)
			}
			if resetCalls != 1 || len(snap.Resets) != 1 || !snap.Resets[0].ExpiresAt.Equal(testNow.Add(48*time.Hour)) {
				t.Fatalf("resets %+v calls=%d", snap.Resets, resetCalls)
			}
		})
	}
}

func TestAntigravityParse(t *testing.T) {
	tests := []struct {
		name    string
		report  string
		wantErr bool
		windows int
	}{
		{"camel", `{"status":"SUCCESS","command":{"name":"usage","data":{"groups":[{"displayName":"Gemini","buckets":[{"bucketId":"s","displayName":"Session","remaining":{"remainingFraction":0.75},"resetTime":"2026-10-19T12:00:00Z"}]}]}}}`, false, 1},
		{"snake", `{"status":"SUCCESS","command":{"name":"usage","data":{"groups":[{"display_name":"Claude","buckets":[{"bucket_id":"w","display_name":"Weekly","remaining_fraction":0.5,"reset_time":1792000000},{"bucket_id":"x","disabled":true,"remaining_fraction":1}]}]}}}`, false, 1},
		{"failed", `{"status":"ERROR","command":{"name":"usage","data":{}}}`, true, 0},
		{"no quota", `{"status":"SUCCESS","command":{"name":"usage","data":{"groups":[{"buckets":[{"bucket_id":"w"}]}]}}}`, true, 0},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			snap, err := parseAgyReport([]byte(tt.report), Snapshot{})
			if (err != nil) != tt.wantErr || len(snap.Windows) != tt.windows {
				t.Fatalf("err=%v windows=%+v", err, snap.Windows)
			}
			if tt.name == "camel" && (snap.Windows[0].UsedPercent != 25 || snap.Windows[0].Name != "Gemini Session") {
				t.Fatalf("window %+v", snap.Windows[0])
			}
		})
	}
	a := &Antigravity{Bin: "definitely-not-installed-agy"}
	if _, err := a.Read(context.Background()); !errors.Is(err, ErrUnavailable) {
		t.Fatalf("err = %v", err)
	}
}

func TestLocal(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/v1/models" {
			w.WriteHeader(404)
			return
		}
		_, _ = w.Write([]byte(`{"data":[{"id":"m1"},{"id":"m2"}]}`))
	}))
	defer srv.Close()
	for _, base := range []string{srv.URL, srv.URL + "/v1", srv.URL + "/v1/"} {
		snap, err := NewLocal(base).Read(context.Background())
		if err != nil || len(snap.Models) != 2 || snap.Latency <= 0 {
			t.Fatalf("base %q: err=%v snap=%+v", base, err, snap)
		}
	}
	srv.Close()
	if _, err := NewLocal(srv.URL).Read(context.Background()); err == nil {
		t.Fatal("expected error for closed server")
	}
}

type stubReader struct {
	name  string
	delay time.Duration
	err   error
	panic bool
}

func (s stubReader) Provider() string { return s.name }
func (s stubReader) Read(ctx context.Context) (Snapshot, error) {
	if s.panic {
		panic("boom")
	}
	select {
	case <-time.After(s.delay):
		return Snapshot{Plan: "p"}, s.err
	case <-ctx.Done():
		return Snapshot{}, ctx.Err()
	}
}

func TestReadAll(t *testing.T) {
	out := ReadAll(context.Background(), []Reader{
		stubReader{name: "a"},
		stubReader{name: "b", err: ErrExpired},
		stubReader{name: "c", delay: time.Minute},
		stubReader{name: "d", panic: true},
	}, 100*time.Millisecond)
	if len(out) != 4 {
		t.Fatal(len(out))
	}
	if out[0].Provider != "a" || out[0].Err != "" || out[0].Plan != "p" {
		t.Fatalf("a: %+v", out[0])
	}
	if out[1].Err != ErrExpired.Error() {
		t.Fatalf("b: %+v", out[1])
	}
	if out[2].Err == "" || out[2].Provider != "c" {
		t.Fatalf("c: %+v", out[2])
	}
	if out[3].Err == "" {
		t.Fatalf("d: %+v", out[3])
	}
}
