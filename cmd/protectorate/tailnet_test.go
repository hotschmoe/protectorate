package main

import (
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
