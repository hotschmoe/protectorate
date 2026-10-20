package e2e

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestBinaryTerminalLauncher(t *testing.T) {
	c := newCluster(t)
	c.requireRuntime(t)
	workspace := filepath.Join(c.root, "terminal-project")
	if err := os.Mkdir(workspace, 0700); err != nil {
		t.Fatalf("create terminal workspace: %v", err)
	}
	t.Cleanup(func() {
		if !t.Failed() {
			return
		}
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		previous := c.ctx
		c.ctx = ctx
		defer func() { c.ctx = previous }()
		c.rpc("SleeveService", "KillSleeve", map[string]any{"name": "long-command"}, nil)
	})
	payload := strings.Repeat("' $(touch injected) `touch injected` ; \\\n", 1024)
	secret := "terminal-environment-secret"
	script := `set -eu
printf '%s' "$1" > payload
printf '%s' "$LAUNCH_SECRET" > secret
if test ! -f once; then
  touch once
  sleep 1
  exit 7
fi
touch respawned
exec sleep 600
`
	request := map[string]any{
		"name": "long-command", "harness": "HARNESS_SHELL", "workspace": workspace,
		"env": map[string]string{"LAUNCH_SECRET": secret}, "args": []string{"sh", "-c", script, "sh", payload},
	}
	c.call("SleeveService", "SpawnSleeve", request)
	c.eventually(func() bool {
		info := c.call("SleeveService", "GetSleeve", map[string]any{"name": "long-command"})["sleeve"].(map[string]any)
		return info["state"] == "SLEEVE_STATE_EXITED" && info["exitCode"] == float64(7)
	})
	for _, tc := range []struct{ file, want string }{{"payload", payload}, {"secret", secret}} {
		b, err := os.ReadFile(filepath.Join(workspace, tc.file))
		if err != nil || string(b) != tc.want {
			t.Fatalf("terminal argument/environment %s changed: bytes=%d error=%v", tc.file, len(b), err)
		}
	}
	if _, err := os.Stat(filepath.Join(workspace, "injected")); !os.IsNotExist(err) {
		t.Fatalf("terminal argument executed shell syntax: %v", err)
	}
	launcher := filepath.Join(c.home, "sleeves", "long-command", "launch.sh")
	info, err := os.Stat(launcher)
	if err != nil || info.Mode().Perm() != 0700 {
		t.Fatalf("terminal launcher permissions: info=%v error=%v", info, err)
	}
	b, err := os.ReadFile(launcher)
	if err != nil || strings.Contains(string(b), secret) {
		t.Fatalf("terminal launcher exposed caller environment: %v", err)
	}
	listed := c.cli("session", "ls", "--json")
	if strings.Contains(listed, secret) || strings.Contains(listed, "touch injected") {
		t.Fatal("session metadata exposed environment or expanded terminal argv")
	}
	c.stop(c.hub)
	c.boot()
	if persisted, err := os.ReadFile(launcher); err != nil || string(persisted) != string(b) {
		t.Fatalf("terminal launcher changed across restart: %v", err)
	}
	c.call("SleeveService", "SpawnSleeve", request)
	c.eventually(func() bool {
		_, err := os.Stat(filepath.Join(workspace, "respawned"))
		return err == nil
	})
	c.call("SleeveService", "KillSleeve", map[string]any{"name": "long-command"})
}
