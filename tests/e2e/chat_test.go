package e2e

import (
	"context"
	"net/http"
	"os"
	"path/filepath"
	"testing"
	"time"

	"connectrpc.com/connect"
	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

const protocolPeer = `#!/bin/sh
sid=e2e-session
previous=
for arg in "$@"; do
 case "$previous" in --session-id|--resume) sid="$arg";; esac
 previous="$arg"
done
printf '{"type":"system","subtype":"init","session_id":"%s"}\n' "$sid"
while IFS= read -r line; do
 printf '%s\n' '{"type":"assistant","message":{"id":"reply","model":"fixture","content":[{"type":"text","text":"po"}]}}'
 printf '%s\n' '{"type":"assistant","message":{"id":"reply","model":"fixture","content":[{"type":"text","text":"ng"}]}}'
 printf '{"type":"result","subtype":"success","session_id":"%s"}\n' "$sid"
done
`

func (c *cluster) chatScenario(t *testing.T) {
	previous := c.t
	defer func() { c.t = previous }()
	c.t = t
	workspace := filepath.Join(c.root, "chat-project")
	if err := os.MkdirAll(workspace, 0700); err != nil {
		t.Fatal(err)
	}
	c.call("SleeveService", "SpawnSleeve", map[string]any{"name": "chatfixture", "harness": "HARNESS_CLAUDE", "workspace": workspace})
	t.Cleanup(func() { c.rpc("SleeveService", "KillSleeve", map[string]any{"name": "chatfixture"}, nil) })
	client := protectoratev1connect.NewChatServiceClient(&http.Client{}, c.url)
	watch := func() string {
		ctx, cancel := context.WithTimeout(c.ctx, 5*time.Second)
		defer cancel()
		req := connect.NewRequest(&pb.WatchRequest{Sleeve: "chatfixture"})
		req.Header().Set("Authorization", "Bearer "+c.token)
		stream, err := client.Watch(ctx, req)
		if err != nil {
			t.Fatal(err)
		}
		defer stream.Close()
		text := ""
		for stream.Receive() {
			ev := stream.Msg()
			if chunk := ev.GetText(); chunk != nil {
				text += chunk.Delta
			}
			if ev.GetSettled() != nil {
				return text
			}
		}
		t.Fatalf("watch stopped before settlement: %v", stream.Err())
		return ""
	}
	request := map[string]any{"sleeve": "chatfixture", "text": "ping", "requestId": "chat-e2e-0001"}
	c.call("ChatService", "Send", request)
	if text := watch(); text != "pong" {
		t.Fatalf("live/replay text=%q", text)
	}
	c.stop(c.hub)
	c.boot()
	c.call("ChatService", "Send", request)
	if text := watch(); text != "pong" {
		t.Fatalf("duplicate send changed replay=%q", text)
	}
	c.call("SleeveService", "KillSleeve", map[string]any{"name": "chatfixture"})
}
