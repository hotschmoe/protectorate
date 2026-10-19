package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

func TestWriteTailnetStatus(t *testing.T) {
	now := time.Date(2026, 1, 1, 12, 0, 0, 0, time.UTC)
	res := &pb.TailnetServiceGetStatusResponse{
		BackendState: "Running",
		Self:         &pb.TailnetPeer{Name: "hub", Ips: []string{"100.64.0.1"}},
		Peers: []*pb.TailnetPeer{
			{Name: "alpha", Online: true, DirectAddr: "10.0.0.2:41641", LastHandshake: timestamppb.New(now.Add(-time.Minute))},
			{Name: "beta", Online: true, Relay: "protectorate"},
			{Name: "gamma", LastSeen: timestamppb.New(now.Add(-time.Hour))},
		},
	}
	tests := []struct{ peer, path string }{
		{"alpha", "direct 10.0.0.2:41641"},
		{"beta", "DERP protectorate"},
		{"gamma", "-"},
	}
	var b strings.Builder
	writeTailnetStatus(&b, res, now)
	for _, tt := range tests {
		var line string
		for _, l := range strings.Split(b.String(), "\n") {
			if strings.HasPrefix(l, tt.peer+"\t") {
				line = l
			}
		}
		if got := strings.Split(line, "\t")[4]; got != tt.path {
			t.Errorf("%s path %q, want %q\n%s", tt.peer, got, tt.path, b.String())
		}
	}
}

func TestNodeJoinKeyInputs(t *testing.T) {
	empty := filepath.Join(t.TempDir(), "empty.key")
	if err := os.WriteFile(empty, nil, 0o600); err != nil {
		t.Fatal(err)
	}
	tests := []struct {
		name string
		args []string
		want string
	}{
		{"no key", nil, "enrollment key is required"},
		{"both", []string{"--key", "k", "--key-file", empty}, "exclusive"},
		{"empty file", []string{"--key-file", empty}, "is empty"},
		{"missing file", []string{"--key-file", empty + ".nope"}, "read key file"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Setenv("PROTECTORATE_HOME", t.TempDir())
			t.Setenv(enrollKeyEnv, "")
			_, err := runCLI("http://127.0.0.1:1", append([]string{"node", "join", "--control", "http://127.0.0.1:1"}, tt.args...)...)
			if err == nil || !strings.Contains(err.Error(), tt.want) {
				t.Fatalf("err %v, want %q", err, tt.want)
			}
		})
	}
}
