package e2e

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

func TestBinaryAppHubConnection(t *testing.T) {
	c := newCluster(t)
	dart, err := exec.LookPath("dart")
	if err != nil {
		t.Skipf("app connection scenario requires Dart: %v", err)
	}
	app, err := filepath.Abs("../../app")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(app, ".dart_tool", "package_config.json")); err != nil {
		t.Skip("app connection scenario requires resolved Flutter dependencies; run make app-check first")
	}
	cmd := exec.CommandContext(c.ctx, dart, "--packages=.dart_tool/package_config.json", "tool/hub_connection_e2e.dart", c.url, c.token)
	cmd.Dir = app
	cmd.Env = c.env
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("Dart app connection against real hub: %v\n%s", err, out)
	} else {
		t.Logf("%s", out)
	}
}
