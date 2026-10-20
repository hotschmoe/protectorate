package e2e

import (
	"net/http"
	"testing"
)

func TestBinaryMCPSessionsBoundedAndReusable(t *testing.T) {
	c := newCluster(t)
	post, deleteSession := startMCPSessionClient(t, c)
	var ids []string
	for range 1000 {
		status, id, reply := post("", "initialize", map[string]any{"protocolVersion": "2025-11-25"})
		if status == http.StatusServiceUnavailable {
			break
		}
		if status != http.StatusOK || id == "" || reply.Error != nil {
			t.Fatalf("initialize: status=%d id=%q reply=%+v", status, id, reply)
		}
		ids = append(ids, id)
	}
	if len(ids) == 0 || len(ids) == 1000 {
		t.Fatalf("bridge did not bound retained sessions: %d accepted", len(ids))
	}
	status, _, reply := post(ids[0], "tools/call", map[string]any{"name": "hub_get_status", "arguments": map[string]any{}})
	if status != http.StatusOK {
		t.Fatalf("session limit disrupted an existing client: status=%d", status)
	}
	assertCLIMCPStatus(t, reply)
	if status := deleteSession(ids[0]); status != http.StatusNoContent {
		t.Fatalf("delete session: status=%d", status)
	}
	status, id, reply := post("", "initialize", map[string]any{"protocolVersion": "2025-11-25"})
	if status != http.StatusOK || id == "" || reply.Error != nil {
		t.Fatalf("session capacity was not released: status=%d id=%q reply=%+v", status, id, reply)
	}
	status, _, reply = post(id, "tools/call", map[string]any{"name": "hub_get_status", "arguments": map[string]any{}})
	if status != http.StatusOK {
		t.Fatalf("replacement session tool call: status=%d", status)
	}
	assertCLIMCPStatus(t, reply)
}
