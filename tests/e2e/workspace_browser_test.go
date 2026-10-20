package e2e

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"google.golang.org/protobuf/encoding/protojson"
)

func TestBinaryWorkspaceBrowser(t *testing.T) {
	t.Setenv("PROTECTORATE_TOKEN_FILE", filepath.Join(t.TempDir(), "missing-sleeve-token"))
	t.Setenv("PROTECTORATE_HUB", "http://127.0.0.1:1")
	t.Setenv("PROTECTORATE_TOKEN", "inherited-sleeve-token")
	c := newCluster(t)
	if roots := c.call("SleeveService", "BrowseWorkspaces", map[string]any{}); len(roots) != 0 {
		t.Fatalf("roots without ~/github: %v", roots)
	}
	c.stop(c.hub)
	root := filepath.Join(c.root, "github")
	repo := filepath.Join(root, "project")
	other := filepath.Join(c.root, "other-root")
	for _, dir := range []string{filepath.Join(repo, ".git"), filepath.Join(repo, "nested"), filepath.Join(root, ".hidden"), filepath.Join(root, "many"), other} {
		if err := os.MkdirAll(dir, 0700); err != nil {
			t.Fatal(err)
		}
	}
	write := func(path, body string) {
		t.Helper()
		if err := os.WriteFile(path, []byte(body), 0600); err != nil {
			t.Fatal(err)
		}
	}
	write(filepath.Join(repo, ".git", "HEAD"), "ref: refs/heads/feature/browser\n")
	write(filepath.Join(repo, ".git", "config"), "[remote \"origin\"]\nurl = https://user:secret@example.com/owner/project.git\n[core]\nfsmonitor = touch should-never-run\n")
	write(filepath.Join(root, "file"), "not a directory")
	if err := os.Symlink(c.root, filepath.Join(root, "escape")); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(repo, filepath.Join(root, "alias")); err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 502; i++ {
		if err := os.Mkdir(filepath.Join(root, "many", fmt.Sprintf("dir-%03d", i)), 0700); err != nil {
			t.Fatal(err)
		}
	}
	// An exited sleeve exercises list/get without requiring a host user manager.
	record, err := json.Marshal(map[string]any{"sleeves": []any{map[string]any{
		"name": "browser", "harness": "codex", "mode": "chat", "workspace": repo, "state": "exited", "created": "2026-01-01T00:00:00Z",
		"read_only": []any{map[string]any{"source": other, "target": "/reference"}}, "exit_after_idle": 300000000000,
		"chat": map[string]any{
			"state": "CHAT_STATE_IDLE", "session_id": "browser-session", "model": "browser-model", "effort": "high",
			"requested": map[string]any{"effort": "high"}, "native_defaults": map[string]any{"model": "browser-native", "effort": "low"},
			"provider": "openai", "local_model": true, "context_tokens": 123, "context_window": 1000,
			"last_seq": 7, "cache_expires": "2026-01-01T01:00:00Z", "last_activity": "2026-01-01T00:01:00Z",
		},
	}}})
	if err != nil {
		t.Fatal(err)
	}
	write(filepath.Join(c.home, "sleeves.json"), string(record))
	c.boot()
	show := c.cli("sleeve", "show", "browser")
	for _, value := range []string{"read-only mount", other + " -> /reference", "exit after idle", "5m0s", "mode", "chat", "CHAT_STATE_IDLE", "browser-session", "browser-model", "high", "browser-native", "low", "openai", "local model", "true", "123/1000", "last sequence", "7", "cache expires", "last activity"} {
		if !strings.Contains(show, value) {
			t.Fatalf("sleeve show missing %q: %s", value, show)
		}
	}
	rows := strings.Split(strings.TrimSpace(c.cli("sleeve", "ls")), "\n")
	if len(rows) != 2 || !strings.Contains(rows[0], "MODEL") || !strings.Contains(rows[0], "EFFORT") || !strings.Contains(rows[1], "browser-model") || !strings.Contains(rows[1], "high") {
		t.Fatalf("sleeve ls missing model/effort: %v", rows)
	}
	browse := func(path string) *pb.BrowseWorkspacesResponse {
		t.Helper()
		var res pb.BrowseWorkspacesResponse
		args := []string{"sleeve", "browse", "--json"}
		if path != "" {
			args = append(args, path)
		}
		out := c.cli(args...)
		if err := protojson.Unmarshal([]byte(out), &res); err != nil {
			t.Fatalf("browse JSON: %v: %s", err, out)
		}
		return &res
	}
	roots := browse("")
	if len(roots.Directories) != 1 || roots.Directories[0].Path != root {
		t.Fatalf("default roots: %v", roots)
	}
	listing := browse(root)
	if len(listing.Directories) != 3 {
		t.Fatalf("hidden/files/escape filtering: %v", listing)
	}
	for _, d := range listing.Directories {
		if d.Name == "many" {
			continue
		}
		if !d.IsGit || !d.Acceptable || d.GitBranch != "feature/browser" || d.GitRemote != "https://example.com/owner/project.git" || d.Path != repo {
			t.Fatalf("repository details: %v", d)
		}
	}
	if got := browse(repo); got.Parent != root || len(got.Directories) != 1 || got.Directories[0].Name != "nested" {
		t.Fatalf("drill down: %v", got)
	}
	if got := browse(filepath.Join(root, "many")); !got.Truncated || len(got.Directories) != 500 {
		t.Fatalf("capped listing: %v", got)
	}
	c.rejectedCLI("outside workspace roots", "sleeve", "browse", filepath.Join(root, "escape"))
	c.rejectedCLI("absolute", "sleeve", "browse", "relative")
	owner := c.token
	paired := c.call("TokenService", "CreateToken", map[string]any{"name": "pair: browser", "scopes": []string{"read", "sleeve_*"}})["secret"].(string)
	identity := c.call("TokenService", "CreateToken", map[string]any{"name": "sleeve:browser", "scopes": []string{"read", "sleeve_*"}})["secret"].(string)
	c.token = paired
	if status := c.rpc("SleeveService", "BrowseWorkspaces", map[string]any{"path": root}, nil); status != 200 {
		t.Fatalf("paired token browse: %d", status)
	}
	c.token = identity
	for _, path := range []string{"", root, repo} {
		if status := c.rpc("SleeveService", "BrowseWorkspaces", map[string]any{"path": path}, nil); status != 403 {
			t.Fatalf("sleeve token browse %q: %d", path, status)
		}
	}
	c.token = owner
	for _, branch := range []string{"feature/browser", "changed-live"} {
		write(filepath.Join(repo, ".git", "HEAD"), "ref: refs/heads/"+branch+"\n")
		for _, args := range [][]string{{"sleeve", "ls"}, {"sleeve", "show", "browser"}, {"sleeve", "ls", "--json"}, {"sleeve", "show", "browser", "--json"}} {
			if got := c.cli(args...); !strings.Contains(got, branch) {
				t.Fatalf("%v missing branch %s: %s", args, branch, got)
			}
		}
	}
	write(filepath.Join(repo, ".git", "HEAD"), strings.Repeat("a", 40)+"\n")
	var detached pb.GetSleeveResponse
	if err := protojson.Unmarshal([]byte(c.cli("sleeve", "show", "browser", "--json")), &detached); err != nil {
		t.Fatal(err)
	}
	if detached.GetSleeve().GetGitBranch() != "" {
		t.Fatalf("detached HEAD retained branch: %v", &detached)
	}
	c.stop(c.hub)
	c.hub = c.start("hub-roots", c.env, "hub", "serve", "--listen", strings.TrimPrefix(c.url, "http://"), "--workspace-root", root, "--workspace-root", other, "--workspace-root", c.home)
	c.eventually(func() bool { return c.rpc("HubService", "GetStatus", map[string]any{}, nil) == 200 })
	if got := browse(""); len(got.Directories) != 3 {
		t.Fatalf("explicit roots: %v", got)
	}
	for _, d := range browse("").Directories {
		if d.Path == c.home && (d.Acceptable || d.Reason == "") {
			t.Fatalf("state root accepted: %v", d)
		}
	}
}
