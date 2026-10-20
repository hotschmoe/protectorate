package e2e

import (
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/hotschmoe/protectorate/internal/sleeve"
)

func TestBinaryNestedRuntime(t *testing.T) {
	c := newCluster(t)
	c.requireRuntime(t)
	goBinary, err := exec.LookPath("go")
	if err != nil {
		t.Fatal(err)
	}
	goBinary, err = filepath.EvalSymlinks(goBinary)
	if err != nil {
		t.Fatal(err)
	}
	c.hubArgs = []string{"--sleeve-tool-dir", filepath.Dir(c.binary), "--sleeve-tool-dir", filepath.Dir(filepath.Dir(goBinary))}
	c.stop(c.hub)
	c.boot()
	if os.Getenv("PROT_E2E_NESTED_CHILD") == "1" {
		if _, err := os.Stat(filepath.Join(sleeve.DelegatedCgroupRoot, "cgroup.controllers")); err != nil {
			t.Fatalf("nested sleeve has no delegated subtree: %v", err)
		}
		c.runtimeScenario(t)
		c.nestedLimitsScenario(t)
		return
	}
	workspace := filepath.Join(c.root, "nested-project")
	selectedVersion, err := exec.CommandContext(c.ctx, "bwrap", "--version").Output()
	if err != nil {
		t.Fatalf("read selected bwrap version: %v", err)
	}
	if err := os.Mkdir(workspace, 0700); err != nil {
		t.Fatal(err)
	}
	runner, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	for _, file := range []struct{ source, target string }{
		{runner, "runtime.test"}, {c.binary, "protectorate"},
	} {
		b, err := os.ReadFile(file.source)
		if err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(workspace, file.target), b, 0700); err != nil {
			t.Fatal(err)
		}
	}
	for _, limits := range []map[string]any{nil, {"cpuPercent": 100}, {"memoryBytes": "536870912"}} {
		if status := c.rpc("SleeveService", "SpawnSleeve", map[string]any{
			"name": "nested-invalid", "harness": "HARNESS_SHELL", "workspace": workspace, "nested": true, "limits": limits,
		}, nil); status != 400 {
			t.Fatalf("nested spawn without finite limits: status=%d", status)
		}
	}
	script := `
set +e
mkdir -p "$HOME/.cache/protectorate-tests"
export TMPDIR="$HOME/.cache/protectorate-tests"
bwrap --version > nested.bwrap-version
env -u PROTECTORATE_TOKEN_FILE -u PROTECTORATE_HUB -u PROTECTORATE_TOKEN PROT_E2E=1 PROT_E2E_NESTED_CHILD=1 PROT_E2E_BINARY="$PWD/protectorate" ./runtime.test -test.run '^TestBinaryNestedRuntime$' -test.v > nested.log 2>&1
status=$?
printf '%s\n' "$status" > nested.status
exec sleep 600
`
	c.cli("sleeve", "spawn", "nested", "--workspace", workspace, "--harness", "shell", "--nested", "--cpu", "100", "--mem", "1G", "--", "sh", "-c", script)
	t.Cleanup(func() { c.rpc("SleeveService", "KillSleeve", map[string]any{"name": "nested"}, nil) })
	// The running delegated scope and its opt-in must survive a hub restart.
	c.stop(c.hub)
	c.boot()
	if got := c.call("SleeveService", "GetSleeve", map[string]any{"name": "nested"})["sleeve"].(map[string]any)["nested"]; got != true {
		t.Fatalf("nested opt-in lost across restart: %v", got)
	}
	deadline := time.NewTimer(55 * time.Second)
	defer deadline.Stop()
	for {
		status, err := os.ReadFile(filepath.Join(workspace, "nested.status"))
		if err != nil {
			select {
			case <-c.ctx.Done():
				t.Fatal(c.ctx.Err())
			case <-deadline.C:
				log, _ := os.ReadFile(filepath.Join(workspace, "nested.log"))
				var pane map[string]any
				c.rpc("SessionService", "CapturePane", map[string]any{"name": "sleeve-nested", "lines": 80}, &pane)
				t.Fatalf("nested runtime timed out:\n%s\nparent pane: %v", log, pane)
			case <-time.After(100 * time.Millisecond):
				continue
			}
		}
		log, _ := os.ReadFile(filepath.Join(workspace, "nested.log"))
		if strings.TrimSpace(string(status)) != "0" || strings.Contains(string(log), "SKIP") {
			t.Fatalf("runtime suite inside nested sleeve: status=%s\n%s", status, log)
		}
		t.Logf("runtime suite inside nested sleeve:\n%s", log)
		break
	}
	if actual, err := os.ReadFile(filepath.Join(workspace, "nested.bwrap-version")); err != nil || strings.TrimSpace(string(actual)) != strings.TrimSpace(string(selectedVersion)) {
		t.Fatalf("nested bwrap changed: selected=%s actual=%s error=%v", selectedVersion, actual, err)
	}
	cfg, err := sleeve.DefaultConfig(c.home, []string{})
	if err != nil {
		t.Fatal(err)
	}
	root := filepath.Join(cfg.CgroupRoot, cfg.UnitName("nested")+".scope")
	for file, want := range map[string]string{"cpu.max": "100000 100000", "memory.max": "1073741824", "pids.max": "4096"} {
		b, err := os.ReadFile(filepath.Join(root, file))
		if err != nil || strings.TrimSpace(string(b)) != want {
			t.Fatalf("outer %s changed: %q, %v", file, b, err)
		}
	}
	c.call("SleeveService", "KillSleeve", map[string]any{"name": "nested"})
	c.eventually(func() bool { _, err := os.Stat(root); return os.IsNotExist(err) })
}

func (c *cluster) nestedLimitsScenario(t *testing.T) {
	cfg, err := sleeve.DefaultConfig(c.home, []string{})
	if err != nil {
		t.Fatal(err)
	}
	workspace := filepath.Join(c.root, "limits-project")
	if err := os.Mkdir(workspace, 0700); err != nil {
		t.Fatal(err)
	}
	secret := filepath.Join(c.root, "parent-only")
	if err := os.WriteFile(secret, []byte("parent state"), 0600); err != nil {
		t.Fatal(err)
	}
	script := `set -eu
test ! -e "$1"
test ! -e "$2"
test ! -e "$3"
test "$(cat /proc/self/cgroup)" = '0::/'
unshare --user --map-root-user --mount sh -ec '
  mkdir /tmp/cgroup-view
  if mount -t cgroup2 none /tmp/cgroup-view; then
    test -e /tmp/cgroup-view/cgroup.procs
    test ! -e /tmp/cgroup-view/cpu.max
    test ! -e /tmp/cgroup-view/memory.max
    test ! -e /tmp/cgroup-view/pids.max
    printf confined > remount.proof
  else
    printf refused > remount.proof
  fi
'
printf confined > proof
set +e
tail /dev/zero
printf '%s\n' "$?" > memory.status
exec sh -c 'while :; do :; done'
`
	c.call("SleeveService", "SpawnSleeve", map[string]any{
		"name": "limited", "harness": "HARNESS_SHELL", "workspace": workspace,
		"limits": map[string]any{"cpuPercent": 50, "memoryBytes": "134217728"},
		"args":   []string{"sh", "-c", script, "sh", filepath.Join(sleeve.DelegatedCgroupRoot, "cgroup.controllers"), c.home, secret},
	})
	t.Cleanup(func() { c.rpc("SleeveService", "KillSleeve", map[string]any{"name": "limited"}, nil) })
	c.eventually(func() bool {
		b, err := os.ReadFile(filepath.Join(workspace, "proof"))
		return err == nil && string(b) == "confined"
	})
	if b, err := os.ReadFile(filepath.Join(workspace, "remount.proof")); err != nil || (string(b) != "confined" && string(b) != "refused") {
		t.Fatalf("child cgroup filesystem remount: %s, %v", b, err)
	}
	root := filepath.Join(cfg.CgroupRoot, cfg.UnitName("limited")+".scope")
	for file, want := range map[string]string{"cpu.max": "50000 100000", "memory.max": "134217728", "pids.max": "4096"} {
		b, err := os.ReadFile(filepath.Join(root, file))
		if err != nil || strings.TrimSpace(string(b)) != want {
			t.Fatalf("child %s: %q, %v", file, b, err)
		}
	}
	c.eventually(func() bool {
		b, err := os.ReadFile(filepath.Join(workspace, "memory.status"))
		if err != nil {
			return false
		}
		if strings.TrimSpace(string(b)) != "137" {
			t.Fatalf("memory limit did not kill allocator: %s", b)
		}
		b, err = os.ReadFile(filepath.Join(root, "memory.events"))
		if err != nil || !strings.Contains(string(b), "oom_kill 1\n") {
			t.Fatalf("missing cgroup OOM evidence: %s, %v", b, err)
		}
		b, err = os.ReadFile(filepath.Join(root, "cpu.stat"))
		if err != nil {
			t.Fatal(err)
		}
		for _, line := range strings.Split(string(b), "\n") {
			if value, ok := strings.CutPrefix(line, "nr_throttled "); ok {
				count, err := strconv.ParseUint(value, 10, 64)
				return err == nil && count > 0
			}
		}
		t.Fatalf("CPU throttling counter missing: %s", b)
		return false
	})
	// Limits on the visible delegation root can be raised without changing
	// the hidden ancestor's hard ceiling, which the outer test checks.
	for file, value := range map[string]string{"cpu.max": "max 100000", "memory.max": "max", "pids.max": "max"} {
		if err := os.WriteFile(filepath.Join(cfg.CgroupRoot, file), []byte(value), 0); err != nil {
			t.Logf("delegation root protects %s: %v", file, err)
		}
	}
	c.call("SleeveService", "KillSleeve", map[string]any{"name": "limited"})
	c.eventually(func() bool { _, err := os.Stat(root); return os.IsNotExist(err) })
}
