package e2e

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"
)

func (c *cluster) runtimeScenario(t *testing.T) {
	t.Helper()
	previous := c.t
	defer func() { c.t = previous }()
	c.t = t
	for _, tool := range []string{"bwrap", "systemd-run", "tmux", "git"} {
		if _, err := exec.LookPath(tool); err != nil {
			t.Skipf("sleeve isolation scenario requires %s: %v", tool, err)
		}
	}
	ctx, cancel := context.WithTimeout(c.ctx, 3*time.Second)
	defer cancel()
	if out, err := exec.CommandContext(ctx, "systemd-run", "--user", "--scope", "--quiet", "true").CombinedOutput(); err != nil {
		t.Skipf("sleeve isolation requires a working systemd user manager: %v: %s", err, out)
	}
	if out, err := exec.CommandContext(ctx, "bwrap", "--ro-bind", "/", "/", "--unshare-user", "--", "true").CombinedOutput(); err != nil {
		t.Skipf("sleeve isolation requires nested user namespaces: %v: %s", err, out)
	}
	workspace := filepath.Join(c.root, "project")
	if err := os.MkdirAll(workspace, 0700); err != nil {
		t.Fatal(err)
	}
	c.call("SleeveService", "SpawnSleeve", map[string]any{"name": "isolation", "harness": "HARNESS_SHELL", "workspace": workspace, "args": []string{"sh", "-c", "printf isolated > proof; sleep 20"}})
	t.Cleanup(func() { c.rpc("SleeveService", "KillSleeve", map[string]any{"name": "isolation"}, nil) })
	c.eventually(func() bool {
		b, e := os.ReadFile(filepath.Join(workspace, "proof"))
		return e == nil && string(b) == "isolated"
	})
	for _, forbidden := range []string{c.home, filepath.Join(c.home, "workspaces", "other"), filepath.Join(c.root, ".ssh")} {
		if err := os.MkdirAll(forbidden, 0700); err != nil {
			t.Fatal(err)
		}
		if status := c.rpc("SleeveService", "SpawnSleeve", map[string]any{"name": "escape", "harness": "HARNESS_SHELL", "workspace": forbidden}, nil); status != 400 && status != 403 {
			t.Fatalf("unexpected rejection status=%d for forbidden workspace %s", status, forbidden)
		}
	}
	c.call("SleeveService", "KillSleeve", map[string]any{"name": "isolation"})
	t.Run("ownership across restart", c.ownershipScenario)
	t.Run("official protocol chat", c.chatScenario)
}
