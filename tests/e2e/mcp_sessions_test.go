package e2e

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"testing"
)

func TestBinaryMCPRejectedInitializeKeepsSession(t *testing.T) {
	c := newCluster(t)
	post, _ := startMCPSessionClient(t, c)
	status, id, reply := post("", "initialize", map[string]any{"protocolVersion": "2025-11-25"})
	if status != http.StatusOK || id == "" || reply.Error != nil {
		t.Fatalf("initialize: status=%d id=%q reply=%+v", status, id, reply)
	}
	status, _, reply = post(id, "initialize", map[string]any{})
	if status != http.StatusOK || reply.Error == nil || reply.Error.Code != -32602 {
		t.Fatalf("invalid initialize: status=%d reply=%+v", status, reply)
	}
	status, _, reply = post(id, "tools/call", map[string]any{"name": "hub_get_status", "arguments": map[string]any{}})
	if status != http.StatusOK {
		t.Fatalf("tool call after rejected initialize: status=%d", status)
	}
	assertCLIMCPStatus(t, reply)
}

func startMCPSessionClient(t *testing.T, c *cluster) (func(string, string, any) (int, string, cliMCPReply), func(string) int) {
	t.Helper()
	addr := loopback(t)
	c.start("mcp-sessions", c.env, "mcp", "--hub", c.url, "--http", addr)
	post := func(session, method string, params any) (int, string, cliMCPReply) {
		t.Helper()
		body, err := json.Marshal(map[string]any{"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
		if err != nil {
			t.Fatal(err)
		}
		req, err := http.NewRequestWithContext(c.ctx, http.MethodPost, "http://"+addr, bytes.NewReader(body))
		if err != nil {
			t.Fatal(err)
		}
		req.Header.Set("Authorization", "Bearer "+c.token)
		req.Header.Set("Content-Type", "application/json")
		req.Header.Set("Mcp-Session-Id", session)
		res, err := c.client.Do(req)
		if err != nil {
			return 0, "", cliMCPReply{}
		}
		defer res.Body.Close()
		var reply cliMCPReply
		if res.StatusCode == http.StatusOK {
			if err := json.NewDecoder(res.Body).Decode(&reply); err != nil {
				t.Fatal(err)
			}
		}
		return res.StatusCode, res.Header.Get("Mcp-Session-Id"), reply
	}
	c.eventually(func() bool {
		status, _, _ := post("unknown", "ping", map[string]any{})
		return status == http.StatusNotFound
	})
	deleteSession := func(id string) int {
		t.Helper()
		req, err := http.NewRequestWithContext(c.ctx, http.MethodDelete, "http://"+addr, nil)
		if err != nil {
			t.Fatal(err)
		}
		req.Header.Set("Authorization", "Bearer "+c.token)
		req.Header.Set("Mcp-Session-Id", id)
		res, err := c.client.Do(req)
		if err != nil {
			t.Fatal(fmt.Errorf("delete MCP session: %w", err))
		}
		res.Body.Close()
		return res.StatusCode
	}
	return post, deleteSession
}
