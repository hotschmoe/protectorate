package main

import (
	"bytes"
	"context"
	"net"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"text/tabwriter"
	"time"

	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/hub"
)

func TestCLI(t *testing.T) {
	t.Setenv("PROTECTORATE_HOME", t.TempDir())
	tok, err := hub.EnsureToken()
	if err != nil {
		t.Fatal(err)
	}
	h := hub.New("test", tok)
	h.Heartbeat = 10 * time.Millisecond
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- h.Serve(ctx, l) }()
	t.Cleanup(func() { cancel(); <-done })
	url := "http://" + l.Addr().String()

	tests := []struct {
		name string
		args []string
		want []string
	}{
		{"status", []string{"status"}, []string{"version   test", "platform"}},
		{"status json", []string{"status", "--json"}, []string{`{"version":"test"`, `"startedAt":"`}},
		{"api call", []string{"api", "call", "HubService/GetStatus"}, []string{"\"version\": \"test\""}},
		{"api call json", []string{"api", "call", "--json", "protectorate.v1.HubService/GetStatus", "{}"}, []string{`{"version":"test"`}},
		{"version", []string{"version"}, []string{"protectorate dev"}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var buf bytes.Buffer
			cmd := newRoot()
			cmd.SetOut(&buf)
			cmd.SetArgs(append(tt.args, "--hub", url))
			if err := cmd.Execute(); err != nil {
				t.Fatal(err)
			}
			for _, w := range tt.want {
				if !strings.Contains(buf.String(), w) {
					t.Fatalf("output %q missing %q", buf.String(), w)
				}
			}
		})
	}

	t.Run("bad method", func(t *testing.T) {
		cmd := newRoot()
		cmd.SetOut(&bytes.Buffer{})
		cmd.SetArgs([]string{"api", "call", "nope", "--hub", url})
		if cmd.Execute() == nil {
			t.Fatal("expected error")
		}
	})
}

func TestWriteQuota(t *testing.T) {
	now := time.Date(2026, 10, 19, 12, 0, 0, 0, time.UTC)
	proj := 41.0
	resp := &pb.GetQuotaResponse{Quotas: []*pb.ProviderQuota{
		{
			Provider: "claude", Plan: "max", Freshness: pb.Freshness_FRESHNESS_FRESH,
			Windows: []*pb.QuotaWindow{{Name: "5h", UsedPercent: 30, ResetsAt: timestamppb.New(now.Add(150 * time.Minute)), EstimatedUnusedPercentAtReset: &proj}},
			Resets:  []*pb.ResetCredit{{Kind: "credit", Count: 2, ExpiresAt: timestamppb.New(now.Add(72 * time.Hour))}},
		},
		{
			Provider: "codex", Plan: "plus", Freshness: pb.Freshness_FRESHNESS_STALE, Age: durationpb.New(25 * time.Minute),
			Windows: []*pb.QuotaWindow{{Name: "7d", UsedPercent: 10, ResetsAt: timestamppb.New(now.Add(48 * time.Hour))}},
			Error:   "HTTP 500",
		},
		{Provider: "grok", Freshness: pb.Freshness_FRESHNESS_UNKNOWN, Error: "credentials expired"},
	}}
	var buf bytes.Buffer
	tw := tabwriter.NewWriter(&buf, 0, 0, 2, ' ', 0)
	writeQuota(tw, resp, now)
	tw.Flush()
	out := buf.String()
	for _, want := range []string{
		"claude", "30.0%", "2h30m", "~41%", "fresh",
		"STALE (25m old)", "2d00h",
		"UNKNOWN",
		"claude: 2 reset credit(s) [credit], expires in 3d00h",
		"codex: last read failed: HTTP 500",
		"grok: last read failed: credentials expired",
	} {
		if !strings.Contains(out, want) {
			t.Errorf("output missing %q:\n%s", want, out)
		}
	}
	for _, line := range strings.Split(out, "\n") {
		if strings.HasPrefix(line, "grok ") && (!strings.Contains(line, "?") || strings.Contains(line, "%")) {
			t.Errorf("unknown provider row must show ? and no percentages: %q", line)
		}
	}
}

func TestWriteSuggest(t *testing.T) {
	usable, exp := 62.0, 48.0
	resp := &pb.SuggestResponse{Suggestions: []*pb.HarnessSuggestion{
		{Rank: 1, Harness: pb.Harness_HARNESS_CODEX, Recommended: true, Score: 31.4, UsablePercent: &usable, ExpiringPercent: &exp,
			ResetsIn: durationpb.New(26 * time.Hour), Reason: "7d window resets in 1d02h"},
		{Rank: 2, Harness: pb.Harness_HARNESS_PI, Recommended: true, Score: 1, Reason: "local LLM"},
		{Rank: 3, Harness: pb.Harness_HARNESS_GROK, Reason: "no quota data yet"},
	}}
	var buf bytes.Buffer
	tw := tabwriter.NewWriter(&buf, 0, 0, 2, ' ', 0)
	writeSuggest(tw, resp)
	tw.Flush()
	lines := strings.Split(strings.TrimSpace(buf.String()), "\n")
	if len(lines) != 4 {
		t.Fatalf("want header and 3 rows:\n%s", buf.String())
	}
	for i, want := range [][]string{
		{"RANK", "HARNESS", "REASON"},
		{"1", "codex", "yes", "31.4", "62%", "48%", "1d02h", "7d window"},
		{"2", "pi", "yes", "1.0", "local LLM"},
		{"3", "grok", "no", "0.0", "no quota data yet"},
	} {
		for _, w := range want {
			if !strings.Contains(lines[i], w) {
				t.Errorf("line %d missing %q: %q", i, w, lines[i])
			}
		}
	}
	if strings.Contains(lines[3], "%") {
		t.Errorf("a provider without data must show no percentages: %q", lines[3])
	}
}

func TestLoginStatus(t *testing.T) {
	home := t.TempDir()
	if err := os.MkdirAll(filepath.Join(home, "auth", "codex"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(home, "auth", "codex", "auth.json"), []byte("{}"), 0o600); err != nil {
		t.Fatal(err)
	}
	tests := []struct {
		name string
		args []string
		want string
	}{
		{"status", []string{"login", "--status", "--home", home}, "claude  missing\ncodex   logged in\ngrok    missing\npi      missing\n"},
		{"status with harness", []string{"login", "--status", "codex", "--home", home}, ""},
		{"unknown harness", []string{"login", "nope", "--home", home}, ""},
		{"no harness", []string{"login", "--home", home}, ""},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var buf bytes.Buffer
			cmd := newRoot()
			cmd.SetOut(&buf)
			cmd.SetErr(&buf)
			cmd.SetArgs(tt.args)
			err := cmd.Execute()
			if (err != nil) != (tt.want == "") {
				t.Fatalf("err = %v, output %q", err, buf.String())
			}
			if tt.want != "" && buf.String() != tt.want {
				t.Fatalf("output %q, want %q", buf.String(), tt.want)
			}
		})
	}
}
