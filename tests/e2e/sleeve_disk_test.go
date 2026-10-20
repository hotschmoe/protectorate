package e2e

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"google.golang.org/protobuf/encoding/protojson"
)

func TestBinarySleeveDiskPrune(t *testing.T) {
	c := newCluster(t)
	c.stop(c.hub)
	write := func(path, body string) {
		t.Helper()
		if err := os.MkdirAll(filepath.Dir(path), 0700); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, []byte(body), 0600); err != nil {
			t.Fatal(err)
		}
	}
	git := func(args ...string) string {
		t.Helper()
		cmd := exec.CommandContext(c.ctx, "git", args...)
		cmd.Env = append(append([]string{}, c.env...), "GIT_CONFIG_NOSYSTEM=1", "GIT_CONFIG_GLOBAL=/dev/null", "GIT_AUTHOR_NAME=Owner", "GIT_AUTHOR_EMAIL=owner@example.com", "GIT_COMMITTER_NAME=Owner", "GIT_COMMITTER_EMAIL=owner@example.com")
		out, err := cmd.CombinedOutput()
		if err != nil {
			t.Fatalf("git %v: %v: %s", args, err, out)
		}
		return strings.TrimSpace(string(out))
	}
	bare := filepath.Join(c.root, "remote.git")
	source := filepath.Join(c.root, "source")
	git("init", "--bare", "--initial-branch=main", bare)
	git("init", "--initial-branch=main", source)
	write(filepath.Join(source, "file.txt"), "committed\n")
	write(filepath.Join(source, ".gitattributes"), "*.txt filter=trap diff=trap\n")
	write(filepath.Join(source, ".gitignore"), "build/\n")
	git("-C", source, "add", ".")
	git("-C", source, "commit", "-m", "Published work")
	git("-C", source, "remote", "add", "origin", bare)
	git("-C", source, "push", "origin", "main")
	workspaces := filepath.Join(c.home, "workspaces")
	clone := func(name string) string {
		t.Helper()
		path := filepath.Join(workspaces, name)
		git("clone", "--no-local", bare, path)
		return path
	}
	clean := clone("clean")
	unpushed := clone("unpushed")
	git("-C", unpushed, "checkout", "-b", "unpublished")
	write(filepath.Join(unpushed, "file.txt"), "unpublished\n")
	git("-C", unpushed, "commit", "-am", "Unpublished work")
	git("-C", unpushed, "checkout", "main")
	dirty := clone("dirty")
	write(filepath.Join(dirty, "file.txt"), "modified\n")
	git("-C", dirty, "update-index", "--assume-unchanged", "file.txt")
	untracked := clone("untracked")
	write(filepath.Join(untracked, "new.txt"), "untracked\n")
	built := clone("built")
	if err := os.MkdirAll(filepath.Join(built, "build"), 0700); err != nil {
		t.Fatal(err)
	}
	write(filepath.Join(built, "build", "out.bin"), "build output\n")
	hidden := clone("hidden")
	if err := os.MkdirAll(filepath.Join(hidden, "scratch"), 0700); err != nil {
		t.Fatal(err)
	}
	write(filepath.Join(hidden, "scratch", ".gitignore"), "*\n")
	write(filepath.Join(hidden, "scratch", "work.txt"), "uncommitted work\n")
	staged := clone("staged")
	write(filepath.Join(staged, "file.txt"), "staged\n")
	git("-C", staged, "add", "file.txt")
	write(filepath.Join(staged, "file.txt"), "committed\n")
	mounted := clone("mounted")
	local := filepath.Join(workspaces, "local-source")
	git("clone", "--no-local", source, local)
	// Both source and clone configs must remain inert during verification.
	marker := filepath.Join(c.root, "executed-repository-code")
	for _, path := range []string{clean, bare, source} {
		git("-C", path, "config", "core.fsmonitor", "touch "+marker)
		git("-C", path, "config", "uploadpack.packObjectsHook", "touch "+marker)
		git("-C", path, "config", "filter.trap.clean", "touch "+marker)
		git("-C", path, "config", "diff.trap.command", "touch "+marker)
	}
	home := func(name string) string { return filepath.Join(c.home, "sleeves", name, "home") }
	lower := filepath.Join(c.root, "host-cache-lower")
	write(filepath.Join(lower, "valuable"), "keep host cache lower")
	for _, name := range []string{"stopped", "active", "leftover", "drop"} {
		for _, cache := range []string{"go-build", "go-mod", "pub", "flutter"} {
			write(filepath.Join(home(name), ".cache", cache, "data"), "cache data")
		}
		for _, cache := range []string{"flutter", "pub-hosted", "pub-hosted-hashes", "pub-git", "pub-global_packages", "go-build"} {
			for _, part := range []string{"upper", "work"} {
				write(filepath.Join(home(name), ".cache", "overlays", cache, part, "data"), "overlay data")
			}
		}
		if err := os.Symlink(lower, filepath.Join(home(name), ".cache", "overlays", "go-build", "upper", "host-lower")); err != nil {
			t.Fatal(err)
		}
		write(filepath.Join(home(name), ".codex", "sessions", "history"), "keep session")
		write(filepath.Join(home(name), ".codex", "auth.json"), "keep auth")
		write(filepath.Join(home(name), ".config", "settings"), "keep config")
	}
	// Real module downloads contain read-only directories.
	if err := os.Chmod(filepath.Join(home("stopped"), ".cache", "go-mod"), 0500); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Chmod(filepath.Join(home("stopped"), ".cache", "go-mod"), 0700) })
	outside := filepath.Join(c.root, "owner-workspace")
	write(filepath.Join(outside, "valuable"), "keep outside")
	write(filepath.Join(outside, "go-build", "valuable"), "keep linked cache")
	write(filepath.Join(home("linked"), ".codex", "auth.json"), "keep linked auth")
	if err := os.Symlink(outside, filepath.Join(home("linked"), ".cache")); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(outside, filepath.Join(workspaces, "escape")); err != nil {
		t.Fatal(err)
	}
	record, err := json.Marshal(map[string]any{"sleeves": []any{
		map[string]any{"name": "stopped", "harness": "codex", "workspace": outside, "state": "exited", "killed": true, "mode": "chat"},
		map[string]any{"name": "active", "harness": "codex", "workspace": outside, "state": "running", "mode": "chat", "read_only": []any{map[string]any{"source": mounted, "target": "/reference"}}},
		map[string]any{"name": "drop", "harness": "shell", "workspace": outside, "state": "exited"},
	}})
	if err != nil {
		t.Fatal(err)
	}
	write(filepath.Join(c.home, "sleeves.json"), string(record))
	c.boot()
	var stopped pb.GetSleeveResponse
	c.eventually(func() bool {
		out := c.cli("sleeve", "show", "stopped", "--json")
		return protojson.Unmarshal([]byte(out), &stopped) == nil && stopped.GetSleeve().GetUsage().GetHomeDiskBytes() > 0
	})
	u := stopped.Sleeve.Usage
	if !stopped.Sleeve.Killed || u.CacheDiskBytes != 184 || u.GoBuildCacheDiskBytes != 34 || u.HomeDiskBytes-u.CacheDiskBytes != 32 {
		t.Fatalf("killed home/cache/rest accounting: %v", &stopped)
	}
	c.rejectedCLI("killed", "chat", "send", "stopped", "must not restart a killed sleeve")
	for _, args := range [][]string{{"sleeve", "ls"}, {"sleeve", "show", "stopped"}} {
		out := c.cli(args...)
		if !strings.Contains(out, "killed") || !(strings.Contains(out, "HOME") || strings.Contains(out, "home disk")) {
			t.Fatalf("disk text output %v: %s", args, out)
		}
	}
	prune := func(args ...string) *pb.PruneSleevesResponse {
		t.Helper()
		var res pb.PruneSleevesResponse
		out := c.cli(append([]string{"sleeve", "prune", "--json"}, args...)...)
		if err := protojson.Unmarshal([]byte(out), &res); err != nil {
			t.Fatalf("prune JSON: %v: %s", err, out)
		}
		return &res
	}
	assertExists := func(path string, want bool) {
		t.Helper()
		_, err := os.Stat(path)
		if (err == nil) != want {
			t.Fatalf("exists(%s) = %v, want %t", path, err, want)
		}
	}
	dry := prune("--caches", "--workspaces", "--running")
	overlays := false
	for _, row := range dry.Entries {
		if row.Removed {
			t.Fatalf("dry run removed data: %v", row)
		}
		if row.Path == filepath.Join(home("stopped"), ".cache", "overlays") {
			overlays = row.Kind == "cache" && row.Eligible && row.Bytes == 144
		}
		if row.Kind == "workspace" {
			want := row.Sleeve == "clean" || row.Sleeve == "local-source"
			if row.Eligible != want || row.Bytes == 0 || row.Reason == "" {
				t.Fatalf("workspace eligibility: %v", row)
			}
		}
	}
	if !overlays {
		t.Fatalf("dry run did not select only sleeve overlay bytes: %v", dry)
	}
	if out := c.cli("sleeve", "prune", "--caches", "--workspaces"); !strings.Contains(out, "would remove") || !strings.Contains(out, "unpushed") || !strings.Contains(out, "Dry run") {
		t.Fatalf("dry-run decisions: %s", out)
	}
	assertExists(clean, true)
	assertExists(filepath.Join(home("stopped"), ".cache", "go-build", "data"), true)
	assertExists(filepath.Join(home("stopped"), ".cache", "overlays", "go-build", "upper", "data"), true)
	owner := c.token
	c.token = c.call("TokenService", "CreateToken", map[string]any{"name": "sleeve:active", "scopes": []string{"read", "sleeve_*"}})["secret"].(string)
	for _, request := range []struct {
		method string
		body   map[string]any
	}{
		{"PruneSleeves", map[string]any{"yes": true, "homes": true}},
		{"KillSleeve", map[string]any{"name": "drop", "dropCaches": true}},
	} {
		if status := c.rpc("SleeveService", request.method, request.body, nil); status != 403 {
			t.Fatalf("non-owner destructive operation %s: %d", request.method, status)
		}
	}
	c.token = owner
	c.cli("sleeve", "kill", "drop", "--drop-caches")
	assertExists(filepath.Join(home("drop"), ".cache", "go-build"), false)
	assertExists(filepath.Join(home("drop"), ".cache", "overlays"), false)
	assertExists(filepath.Join(lower, "valuable"), true)
	assertExists(filepath.Join(home("drop"), ".codex", "sessions", "history"), true)
	prune("--caches", "--workspaces", "--running", "--yes")
	for _, path := range []string{clean, local, filepath.Join(home("stopped"), ".cache", "go-mod"), filepath.Join(home("leftover"), ".cache", "pub"), filepath.Join(home("stopped"), ".cache", "overlays"), filepath.Join(home("leftover"), ".cache", "overlays")} {
		assertExists(path, false)
	}
	for _, path := range []string{unpushed, dirty, untracked, staged, mounted, built, hidden, filepath.Join(outside, "valuable"), filepath.Join(outside, "go-build", "valuable"), filepath.Join(home("active"), ".cache", "go-build", "data"), filepath.Join(home("stopped"), ".codex", "auth.json"), filepath.Join(home("stopped"), ".codex", "sessions", "history"), filepath.Join(home("stopped"), ".config", "settings")} {
		assertExists(path, true)
	}
	assertExists(marker, false)
	assertExists(filepath.Join(home("active"), ".cache", "overlays", "go-build", "upper", "data"), true)
	assertExists(filepath.Join(lower, "valuable"), true)
	// --ignored lets tracked .gitignore rules excuse build output only.
	prune("--workspaces", "--ignored", "--yes")
	assertExists(built, false)
	for _, path := range []string{hidden, untracked, unpushed, dirty, staged, mounted} {
		assertExists(path, true)
	}
	prune("--homes", "--yes")
	assertExists(home("stopped"), false)
	assertExists(home("leftover"), false)
	assertExists(home("active"), true)
	assertExists(filepath.Join(outside, "go-build", "valuable"), true)
	assertExists(filepath.Join(lower, "valuable"), true)
	audit := c.cli("audit", "tail", "--json")
	ok := map[string]bool{}
	for _, line := range strings.Split(strings.TrimSpace(audit), "\n") {
		var entry struct {
			Procedure string `json:"procedure"`
			Outcome   string `json:"outcome"`
		}
		if err := json.Unmarshal([]byte(line), &entry); err != nil {
			t.Fatalf("audit JSON: %v: %s", err, line)
		}
		if entry.Outcome == "ok" {
			ok[entry.Procedure] = true
		}
	}
	for _, method := range []string{"PruneSleeves", "KillSleeve"} {
		if !ok["/protectorate.v1.SleeveService/"+method] {
			t.Fatalf("successful destructive cleanup %s not audited: %s", method, audit)
		}
	}
}
