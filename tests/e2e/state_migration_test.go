package e2e

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestBinaryStateMigrationAndPolicyRestart(t *testing.T) {
	c := newCluster(t)
	if c.rust {
		t.Skip("fixture migrates Go nodes.json state, which is outside Rust user-facing parity")
	}
	c.cli("node", "add", "family-box", "--addr", "127.0.0.1:1", "--tag", "keep", "--tier", "care")
	c.stop(c.hub)
	path := filepath.Join(c.home, "nodes.json")
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var inventory map[string]any
	if err := json.Unmarshal(raw, &inventory); err != nil {
		t.Fatal(err)
	}
	for _, item := range inventory["nodes"].([]any) {
		node := item.(map[string]any)
		switch node["name"] {
		case "peer":
			node["tier"] = "server"
		case "family-box":
			node["tier"] = "family"
		}
	}
	retired, err := json.Marshal(inventory)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, retired, 0600); err != nil {
		t.Fatal(err)
	}
	c.rejectedCLI("migrate-tiers", "hub", "serve", "--listen", strings.TrimPrefix(c.url, "http://"))
	if b, err := os.ReadFile(path); err != nil || !bytes.Equal(b, retired) {
		t.Fatalf("refused startup changed the inventory: %q %v", b, err)
	}
	output := c.cli("node", "migrate-tiers")
	if !strings.Contains(output, "server -> shared") || !strings.Contains(output, "family -> care") {
		t.Fatalf("migration did not report both changes: %s", output)
	}
	if b, err := os.ReadFile(path + ".pre-tiers"); err != nil || !bytes.Equal(b, retired) {
		t.Fatalf("migration did not preserve the exact original inventory: %q %v", b, err)
	}
	migrated, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(c.cli("node", "migrate-tiers"), "nothing to do") {
		t.Fatal("migration is not idempotent")
	}
	if err := os.WriteFile(path, retired, 0600); err != nil {
		t.Fatal(err)
	}
	c.rejectedCLI("keep a copy", "node", "migrate-tiers")
	if b, err := os.ReadFile(path); err != nil || !bytes.Equal(b, retired) {
		t.Fatalf("failed migration overwrote the input: %q %v", b, err)
	}
	if err := os.WriteFile(path, migrated, 0600); err != nil {
		t.Fatal(err)
	}
	c.boot()
	for _, expected := range []struct{ name, tier string }{{"peer", "shared"}, {"family-box", "care"}} {
		got := c.call("NodeService", "GetNode", map[string]any{"name": expected.name})["node"].(map[string]any)
		if got["tier"] != expected.tier || (expected.name == "family-box" && !strings.Contains(fmt.Sprint(got["tags"]), "keep")) {
			t.Fatalf("migration lost node metadata: %v", got)
		}
	}
	if hosts, err := os.ReadFile(filepath.Join(c.home, "hosts.md")); err != nil || !strings.Contains(string(hosts), "family-box") || !strings.Contains(string(hosts), "care") {
		t.Fatalf("migration did not regenerate hosts.md: %q %v", hosts, err)
	}
	c.cli("desk", "policy", "peer", "--window", "23:00-05:00", "--idle", "5m", "--tz", "UTC", "--lock-after")
	owner := c.token
	c.token = c.call("TokenService", "CreateToken", map[string]any{"name": "policy-reader", "scopes": []string{"read"}})["secret"].(string)
	if status := c.rpc("NodeService", "SetDeskPolicy", map[string]any{"name": "peer"}, nil); status != 403 {
		t.Fatalf("reader cleared desktop policy: %d", status)
	}
	c.token = owner
	if status := c.rpc("NodeService", "SetDeskPolicy", map[string]any{"name": "peer", "policy": map[string]any{"window": "25:00-05:00", "idle": "300s"}}, nil); status != 400 {
		t.Fatalf("invalid policy accepted: %d", status)
	}
	short := c.call("LeaseService", "AcquireLease", map[string]any{"resource": "peer", "holder": "expire-during-restart", "ttl": "0.2s"})
	c.stop(c.hub)
	c.boot()
	policy := c.call("NodeService", "GetDeskPolicy", map[string]any{"name": "peer"})["policy"].(map[string]any)
	if policy["window"] != "23:00-05:00" || policy["idle"] != "300s" || policy["timezone"] != "UTC" || policy["lockAfter"] != true {
		t.Fatalf("restart lost the desktop safety policy: %v", policy)
	}
	c.eventually(func() bool {
		return !strings.Contains(fmt.Sprint(c.call("LeaseService", "ListLeases", map[string]any{})), short["id"].(string))
	})
	if status := c.rpc("LeaseService", "RenewLease", map[string]any{"id": short["id"]}, nil); status != 404 {
		t.Fatalf("renew resurrected an expired lease: %d", status)
	}
	c.cli("desk", "policy", "peer", "--off")
	c.stop(c.hub)
	c.boot()
	if policy := c.call("NodeService", "GetDeskPolicy", map[string]any{"name": "peer"}); policy["policy"] != nil {
		t.Fatalf("cleared policy returned after restart: %v", policy)
	}
}
