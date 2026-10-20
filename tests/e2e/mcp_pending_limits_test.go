package e2e

import (
	"strconv"
	"testing"
)

func TestBinaryMCPPendingCallsBoundedAndCancelable(t *testing.T) {
	c := newCluster(t)
	mail := c.call("MessageService", "Send", map[string]any{"to": "owner", "body": "MCP capacity regression", "ackRequired": true})
	id := mail["message"].(map[string]any)["id"].(string)
	t.Run("capacity and cancellation", func(t *testing.T) {
		peer := startCLIMCP(t, c)
		peer.send(1, "initialize", map[string]any{"protocolVersion": "2025-11-25"})
		if reply := peer.read(); reply.Error != nil {
			t.Fatalf("initialize: %+v", reply)
		}
		for request := 2; request < 162; request++ {
			peer.send(request, "tools/call", map[string]any{"name": "message_wait", "arguments": map[string]any{"target": id, "timeout": "60s"}})
		}
		peer.send(1000, "ping", map[string]any{})
		rejected := map[int]bool{}
		for {
			reply := peer.read()
			if string(reply.ID) == "1000" {
				break
			}
			request, err := strconv.Atoi(string(reply.ID))
			if err != nil || reply.Error == nil || reply.Error.Code != -32000 {
				t.Fatalf("pending limit response: %+v", reply)
			}
			rejected[request] = true
		}
		if len(rejected) == 0 || len(rejected) == 160 {
			t.Fatalf("pending requests were not bounded while admitting useful work: %d rejected", len(rejected))
		}
		for request := 2; request < 162; request++ {
			if rejected[request] {
				continue
			}
			peer.notify("notifications/cancelled", map[string]any{"requestId": request})
			if reply := peer.read(); string(reply.ID) != strconv.Itoa(request) {
				t.Fatalf("cancel pending request %d: %+v", request, reply)
			}
		}
		peer.send(1001, "tools/call", map[string]any{"name": "hub_get_status", "arguments": map[string]any{}})
		assertCLIMCPStatus(t, peer.read())
	})
}
