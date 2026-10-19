package main

import (
	"bytes"
	"context"
	"net"
	"strings"
	"testing"
	"time"

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
