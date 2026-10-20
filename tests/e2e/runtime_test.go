package e2e

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"

	"github.com/hotschmoe/protectorate/internal/sleeve"
)

func (c *cluster) requireRuntime(t *testing.T) {
	t.Helper()
	for _, tool := range []string{"bwrap", "tmux", "git"} {
		if _, err := exec.LookPath(tool); err != nil {
			t.Skipf("sleeve isolation scenario requires %s: %v", tool, err)
		}
	}
	ctx, cancel := context.WithTimeout(c.ctx, 3*time.Second)
	defer cancel()
	cfg, err := sleeve.DefaultConfig(c.home, nil)
	if err != nil {
		t.Fatal(err)
	}
	if err := cfg.Preflight(); err != nil {
		if os.Getenv("PROT_E2E_NESTED_CHILD") == "1" {
			t.Fatalf("nested runtime preflight: %v", err)
		}
		t.Skipf("sleeve isolation runtime unavailable: %v", err)
	}
	if cfg.CgroupRoot != sleeve.DelegatedCgroupRoot {
		if out, err := exec.CommandContext(ctx, "systemd-run", "--user", "--scope", "--quiet", "true").CombinedOutput(); err != nil {
			t.Skipf("sleeve isolation requires a working systemd user manager: %v: %s", err, out)
		}
	}
	if out, err := exec.CommandContext(ctx, "bwrap", "--ro-bind", "/", "/", "--unshare-user", "--", "true").CombinedOutput(); err != nil {
		if os.Getenv("PROT_E2E_NESTED_CHILD") == "1" {
			t.Fatalf("nested bwrap: %v: %s", err, out)
		}
		t.Skipf("sleeve isolation requires nested user namespaces: %v: %s", err, out)
	}
}

func (c *cluster) runtimeScenario(t *testing.T) {
	t.Helper()
	previous := c.t
	defer func() { c.t = previous }()
	c.t = t
	c.requireRuntime(t)
	workspace := filepath.Join(c.root, "project")
	if err := os.MkdirAll(workspace, 0700); err != nil {
		t.Fatal(err)
	}
	c.call("SleeveService", "SpawnSleeve", map[string]any{"name": "isolation", "harness": "HARNESS_SHELL", "workspace": workspace, "args": []string{"sh", "-c", "printf isolated > proof; sleep 20"}})
	t.Cleanup(func() { c.rpc("SleeveService", "KillSleeve", map[string]any{"name": "isolation"}, nil) })
	started := time.Now()
	diagnosed := false
	c.eventually(func() bool {
		b, e := os.ReadFile(filepath.Join(workspace, "proof"))
		if e != nil && !diagnosed && time.Since(started) > 13*time.Second {
			diagnosed = true
			t.Logf("isolation sleeve state: %v", c.call("SleeveService", "GetSleeve", map[string]any{"name": "isolation"}))
			t.Logf("isolation pane: %v", c.call("SessionService", "CapturePane", map[string]any{"name": "sleeve-isolation", "lines": 80}))
		}
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
