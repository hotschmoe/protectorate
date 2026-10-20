package e2e

import (
	"os"
	"path/filepath"
	"testing"
)

func (c *cluster) ownershipScenario(t *testing.T) {
	previous := c.t
	c.t = t
	defer func() { c.t = previous }()
	owner := c.token
	defer func() {
		c.token = owner
		c.rpc("SleeveService", "KillSleeve", map[string]any{"name": "owned"}, nil)
		c.rpc("SleeveService", "KillSleeve", map[string]any{"name": "other-child"}, nil)
	}()
	issued := c.call("TokenService", "CreateToken", map[string]any{"name": "scope-parent", "scopes": []string{"sleeve_spawn_sleeve"}})
	secret := issued["secret"].(string)
	other := c.call("TokenService", "CreateToken", map[string]any{"name": "sleeve:other", "scopes": []string{"read", "sleeve_spawn_sleeve"}})["secret"].(string)
	workspace := filepath.Join(c.root, "ownership")
	if err := os.MkdirAll(workspace, 0700); err != nil {
		t.Fatal(err)
	}
	request := map[string]any{"name": "owned", "harness": "HARNESS_SHELL", "workspace": workspace, "args": []string{"sh", "-c", "sleep 600"}}
	c.token = secret
	spawned := c.call("SleeveService", "SpawnSleeve", request)["sleeve"].(map[string]any)
	if spawned["parent"] != "token:scope-parent" {
		t.Fatalf("ownership: %v", spawned)
	}
	c.token = owner
	c.token = other
	if status := c.rpc("SleeveService", "KillSleeve", map[string]any{"name": "owned"}, nil); status != 403 {
		t.Fatalf("cross-sleeve kill: %d", status)
	}
	request["name"] = "other-child"
	child := c.call("SleeveService", "SpawnSleeve", request)["sleeve"].(map[string]any)
	if child["parent"] != "sleeve:other" {
		t.Fatalf("child parent: %v", child)
	}
	c.token = owner
	c.stop(c.hub)
	c.boot()
	if got := c.call("SleeveService", "GetSleeve", map[string]any{"name": "owned"})["sleeve"].(map[string]any)["parent"]; got != "token:scope-parent" {
		t.Fatalf("persisted parent: %v", got)
	}
	c.token = secret
	c.call("SleeveService", "KillSleeve", map[string]any{"name": "owned"})
	c.token = other
	c.call("SleeveService", "KillSleeve", map[string]any{"name": "other-child"})
	c.token = owner
}
