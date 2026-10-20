package e2e

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"google.golang.org/protobuf/encoding/protojson"
	"google.golang.org/protobuf/proto"
)

func TestBinaryNoteLifecycle(t *testing.T) {
	c := newCluster(t)
	c.stop(c.hub)
	const existing = `{"notes":[{"id":"0a94","title":"Existing task","created_at":"2026-10-20T00:00:00Z","updated_at":"2026-10-20T00:00:00Z"}]}`
	if err := os.WriteFile(filepath.Join(c.home, "notes.json"), []byte(existing), 0600); err != nil {
		t.Fatal(err)
	}
	c.boot()
	note := func(args ...string) *pb.Note {
		t.Helper()
		var n pb.Note
		out := c.cli(append([]string{"backlog"}, append(args, "--json")...)...)
		if err := protojson.Unmarshal([]byte(out), &n); err != nil {
			t.Fatalf("decode note: %v: %s", err, out)
		}
		return &n
	}
	if n := note("show", "0a94"); n.State != "open" || n.Completion != nil {
		t.Fatalf("existing note did not load as open: %v", n)
	}
	const title = "Finish note lifecycle"
	n := note("add", title, "--project", "protectorate", "--pin", "--body", "A real task")
	id := n.Id
	if n.State != "open" || n.Completion != nil {
		t.Fatalf("new note: %v", n)
	}
	closed := note("done", id, "--resolution", "implemented in 8b98d58")
	if closed.State != "done" || closed.Completion == nil || closed.Completion.DoneBy != "owner" || closed.Completion.Resolution != "implemented in 8b98d58" || closed.Completion.DoneAt == nil {
		t.Fatalf("completion: %v", closed)
	}
	if repeated := note("done", id); !repeated.UpdatedAt.AsTime().Equal(closed.UpdatedAt.AsTime()) || !proto.Equal(repeated.Completion, closed.Completion) {
		t.Fatalf("retry changed completion: %v", repeated)
	}
	for _, args := range [][]string{{"backlog", "ls", "--json"}, {"backlog", "ls"}} {
		if out := c.cli(args...); strings.Contains(out, title) || !strings.Contains(out, "0a94") {
			t.Fatalf("default list %v: %s", args, out)
		}
	}
	for _, args := range [][]string{{"--all"}, {"--state", "done"}, {"--all", "--project", "protectorate", "--query", "real task"}} {
		if out := c.cli(append([]string{"backlog", "ls", "--json"}, args...)...); !strings.Contains(out, title) || !strings.Contains(out, `"state":"done"`) {
			t.Fatalf("filtered list %v: %s", args, out)
		}
	}
	if out := c.cli("backlog", "ls", "--state", "done", "--json"); strings.Contains(out, "Existing task") {
		t.Fatalf("done filter includes open note: %s", out)
	}
	if out := c.cli("backlog", "ls", "--all", "--json"); !strings.Contains(out, "Existing task") {
		t.Fatalf("all filter excludes open note: %s", out)
	}
	if listed := c.call("BacklogService", "ListNotes", map[string]any{}); strings.Contains(string(mustJSON(t, listed)), title) {
		t.Fatalf("RPC default includes done: %v", listed)
	}
	c.rejectedCLI("one line", "backlog", "done", id, "--resolution", "two\nlines")
	c.rejectedCLI("longer than 200", "backlog", "done", id, "--resolution", strings.Repeat("x", 201))
	c.rejectedCLI("state must", "backlog", "ls", "--state", "closed")
	c.rejectedCLI("not both", "backlog", "ls", "--all", "--state", "done")
	for _, req := range []map[string]any{{"id": id, "state": "closed"}, {"id": "0a94", "resolution": "not done"}} {
		if status := c.rpc("BacklogService", "UpdateNote", req, nil); status != 400 {
			t.Fatalf("invalid update status=%d: %v", status, req)
		}
	}
	if status := c.rpc("BacklogService", "ListNotes", map[string]any{"state": "closed"}, nil); status != 400 {
		t.Fatalf("invalid list state status=%d", status)
	}
	if edited := note("edit", id, "--body", "The completed task was reviewed"); edited.State != "done" || !proto.Equal(edited.Completion, closed.Completion) {
		t.Fatalf("body edit changed completion: %v", edited)
	}
	c.stop(c.hub)
	c.boot()
	if persisted := note("show", id); persisted.State != "done" || !proto.Equal(persisted.Completion, closed.Completion) {
		t.Fatalf("completion lost on restart: %v", persisted)
	}
	if reopened := note("reopen", id); reopened.State != "open" || reopened.Completion != nil {
		t.Fatalf("reopen retained completion: %v", reopened)
	}
	c.stop(c.hub)
	c.boot()
	if out := c.cli("backlog", "ls", "--json"); !strings.Contains(out, title) {
		t.Fatalf("reopened note missing after restart: %s", out)
	}
	if reopened := note("show", id); reopened.Completion != nil || reopened.State != "open" {
		t.Fatalf("reopen did not persist: %v", reopened)
	}
	owner := c.token
	reader := c.call("TokenService", "CreateToken", map[string]any{"name": "note-reader", "scopes": []string{"read"}})["secret"].(string)
	sleeve := c.call("TokenService", "CreateToken", map[string]any{"name": "sleeve:note-worker", "scopes": []string{"read", "backlog_*", "directory_*", "lease_*", "message_*"}})["secret"].(string)
	c.token = reader
	if status := c.rpc("BacklogService", "UpdateNote", map[string]any{"id": id, "state": "done"}, nil); status != 403 {
		t.Fatalf("read token closed note: %d", status)
	}
	c.token = sleeve
	updated := c.call("BacklogService", "UpdateNote", map[string]any{"id": id, "state": "done", "resolution": "worker completed task"})
	completion := updated["note"].(map[string]any)["completion"].(map[string]any)
	if completion["doneBy"] != "token:sleeve:note-worker" || completion["resolution"] != "worker completed task" {
		t.Fatalf("sleeve completion actor: %v", updated)
	}
	c.token = owner
	audits := c.call("AuditService", "ListAuditEntries", map[string]any{"subject": id, "actor": "token:sleeve:note-worker"})
	if !strings.Contains(string(mustJSON(t, audits)), "BacklogService/UpdateNote") {
		t.Fatalf("completion actor does not match audit: %v", audits)
	}
	t.Run("MCP note transitions", func(t *testing.T) {
		peer := startCLIMCP(t, c, "--allow", "backlog_*")
		peer.send(1, "initialize", map[string]any{"protocolVersion": "2025-11-25", "clientInfo": map[string]any{"name": "notes-e2e", "version": "1"}, "capabilities": map[string]any{}})
		if res := peer.read(); res.Error != nil {
			t.Fatalf("initialize: %+v", res)
		}
		peer.notify("notifications/initialized", map[string]any{})
		peer.send(2, "tools/call", map[string]any{"name": "backlog_list_notes", "arguments": map[string]any{}})
		if res := peer.read(); res.Error != nil || bytes.Contains(res.Result, []byte(title)) || !bytes.Contains(res.Result, []byte("0a94")) {
			t.Fatalf("MCP open list: %+v", res)
		}
		peer.send(3, "tools/call", map[string]any{"name": "backlog_update_note", "arguments": map[string]any{"id": "0a94", "state": "done", "resolution": "completed through MCP"}})
		if res := peer.read(); res.Error != nil || !bytes.Contains(res.Result, []byte("completed through MCP")) || bytes.Contains(res.Result, []byte(`"isError":true`)) {
			t.Fatalf("MCP close: %+v", res)
		}
		if n := note("show", "0a94"); n.State != "done" || n.Completion.Resolution != "completed through MCP" {
			t.Fatalf("MCP close did not persist: %v", n)
		}
	})
}

func mustJSON(t *testing.T, value any) []byte {
	t.Helper()
	b, err := json.Marshal(value)
	if err != nil {
		t.Fatal(err)
	}
	return b
}
