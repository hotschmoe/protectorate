package e2e

import (
	"context"
	"encoding/json"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"
	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"google.golang.org/protobuf/encoding/protojson"
)

func TestBinaryBranchDispatch(t *testing.T) {
	c := newCluster(t)
	ctx, cancel := context.WithTimeout(c.ctx, 3*time.Second)
	defer cancel()
	if out, err := exec.CommandContext(ctx, "systemd-run", "--user", "--scope", "--quiet", "true").CombinedOutput(); err != nil {
		t.Skipf("branch dispatch requires a working systemd user manager: %v: %s", err, out)
	}
	if out, err := exec.CommandContext(ctx, "bwrap", "--ro-bind", "/", "/", "--unshare-user", "--", "true").CombinedOutput(); err != nil {
		t.Skipf("branch dispatch requires nested user namespaces: %v: %s", err, out)
	}
	repo := filepath.Join(c.root, "dispatch-source")
	if err := os.Mkdir(repo, 0700); err != nil {
		t.Fatal(err)
	}
	git := func(dir string, args ...string) string {
		t.Helper()
		cmd := exec.CommandContext(c.ctx, "git", append([]string{"-C", dir, "-c", "core.fsmonitor=false", "-c", "log.showSignature=false"}, args...)...)
		cmd.Env = append(c.env, "GIT_CONFIG_GLOBAL=/dev/null", "GIT_CONFIG_NOSYSTEM=1", "GIT_AUTHOR_NAME=E2E", "GIT_AUTHOR_EMAIL=e2e@example.invalid", "GIT_COMMITTER_NAME=E2E", "GIT_COMMITTER_EMAIL=e2e@example.invalid")
		out, err := cmd.CombinedOutput()
		if err != nil {
			t.Fatalf("git %v: %v: %s", args, err, out)
		}
		return strings.TrimSpace(string(out))
	}
	git(repo, "init", "--initial-branch=main")
	if err := os.WriteFile(filepath.Join(repo, "tracked"), []byte("base"), 0600); err != nil {
		t.Fatal(err)
	}
	git(repo, "add", "tracked")
	git(repo, "commit", "-m", "initial base")
	old := git(repo, "rev-parse", "HEAD")
	git(repo, "branch", "dispatch/base")
	git(repo, "remote", "add", "origin", "https://user:secret@example.invalid/owner/repo.git")
	brief := "Implement the dispatch task.\nReport every incomplete part; no reply needed.\n"
	briefPath := filepath.Join(c.root, "brief.txt")
	if err := os.WriteFile(briefPath, []byte(brief), 0600); err != nil {
		t.Fatal(err)
	}
	c.rejectedCLI("chat mode", "sleeve", "spawn", "bad-brief", "--workspace", repo, "--harness", "shell", "--brief", briefPath)
	var spawned pb.SpawnSleeveResponse
	out := c.cli("sleeve", "spawn", "dispatch", "--workspace", repo, "--branch", "dispatch/base", "--new-branch", "feature/dispatch", "--brief", briefPath, "--harness", "claude", "--env", "PROTECTORATE_E2E_BASE="+repo, "--json")
	if err := protojson.Unmarshal([]byte(out), &spawned); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { c.rpc("SleeveService", "KillSleeve", map[string]any{"name": "dispatch"}, nil) })
	ws := filepath.Join(c.home, "workspaces", "dispatch")
	if s := spawned.Sleeve; s.Workspace != ws || s.GitBranch != "feature/dispatch" || s.BaseRepo != repo || s.BaseBranch != "dispatch/base" || s.BaseTip != old {
		t.Fatalf("dispatch record: %v", s)
	}
	for _, tc := range []struct {
		args []string
		want string
	}{
		{[]string{"branch", "--show-current"}, "feature/dispatch"},
		{[]string{"remote", "get-url", "origin"}, "https://example.invalid/owner/repo.git"},
		{[]string{"remote", "get-url", "base"}, filepath.Join(repo, ".git")},
	} {
		if got := git(ws, tc.args...); got != tc.want {
			t.Fatalf("clone %v=%q want %q", tc.args, got, tc.want)
		}
	}
	if got := git(repo, "branch", "--show-current"); got != "main" {
		t.Fatalf("source branch changed: %s", got)
	}
	proof, err := os.ReadFile(filepath.Join(ws, "dispatch-proof"))
	if err != nil || string(proof) != "fetched read-only base" {
		t.Fatalf("base sandbox proof=%q err=%v", proof, err)
	}
	client := protectoratev1connect.NewChatServiceClient(&http.Client{}, c.url)
	watchCtx, watchCancel := context.WithTimeout(c.ctx, 5*time.Second)
	defer watchCancel()
	req := connect.NewRequest(&pb.WatchRequest{Sleeve: "dispatch"})
	req.Header().Set("Authorization", "Bearer "+c.token)
	stream, err := client.Watch(watchCtx, req)
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()
	users := 0
	for stream.Receive() {
		if user := stream.Msg().GetUser(); user != nil {
			users++
			if user.Text != brief {
				t.Fatalf("first message=%q", user.Text)
			}
		}
		if stream.Msg().GetSettled() != nil {
			break
		}
	}
	if users != 1 {
		t.Fatalf("brief user messages=%d; stream=%v", users, stream.Err())
	}
	for _, args := range [][]string{{"sleeve", "show", "dispatch"}, {"sleeve", "ls", "--json"}} {
		if text := c.cli(args...); !strings.Contains(text, repo) || !strings.Contains(text, "dispatch/base") {
			t.Fatalf("base absent from %v: %s", args, text)
		}
	}
	// A burst on an un-checked-out base ref tests loose and packed ref polling.
	for _, subject := range []string{"base advance one", "base advance two"} {
		git(repo, "commit", "--allow-empty", "-m", subject)
		git(repo, "update-ref", "refs/heads/dispatch/base", git(repo, "rev-parse", "HEAD"))
	}
	tip := git(repo, "rev-parse", "HEAD")
	git(repo, "pack-refs", "--all", "--prune")
	notices := func() []string {
		t.Helper()
		raw, err := os.ReadFile(filepath.Join(c.home, "mailbox.jsonl"))
		if err != nil {
			t.Fatal(err)
		}
		var bodies []string
		for _, line := range strings.Split(string(raw), "\n") {
			var row struct {
				Message struct{ To, Body string } `json:"msg"`
			}
			if json.Unmarshal([]byte(line), &row) == nil && row.Message.To == "dispatch" && strings.HasPrefix(row.Message.Body, "Base branch ") {
				bodies = append(bodies, row.Message.Body)
			}
		}
		return bodies
	}
	c.eventually(func() bool { return len(notices()) == 1 })
	body := notices()[0]
	for _, want := range []string{old[:7] + ".." + tip[:7], "base advance one", "base advance two", "git fetch base && git rebase base/dispatch/base"} {
		if !strings.Contains(body, want) {
			t.Fatalf("notice missing %q: %s", want, body)
		}
	}
	c.eventually(func() bool {
		var current pb.GetSleeveResponse
		if err := protojson.Unmarshal([]byte(c.cli("sleeve", "show", "dispatch", "--json")), &current); err != nil {
			t.Fatal(err)
		}
		return current.GetSleeve().GetBaseTip() == tip
	})
	c.stop(c.hub)
	c.boot()
	// Advance the source to this sleeve's own commit: no self-notification.
	git(ws, "commit", "--allow-empty", "-m", "own work")
	// This is a fast-forward only after the sleeve includes the new base.
	git(ws, "fetch", "base")
	git(ws, "rebase", "base/dispatch/base")
	own := git(ws, "rev-parse", "HEAD")
	git(repo, "fetch", ws, "feature/dispatch")
	git(repo, "update-ref", "refs/heads/dispatch/base", own)
	c.eventually(func() bool {
		var current pb.GetSleeveResponse
		if err := protojson.Unmarshal([]byte(c.cli("sleeve", "show", "dispatch", "--json")), &current); err != nil {
			t.Fatal(err)
		}
		return current.GetSleeve().GetBaseTip() == own
	})
	if got := notices(); len(got) != 1 {
		t.Fatalf("duplicate or self notice: %v", got)
	}
	c.call("SleeveService", "KillSleeve", map[string]any{"name": "dispatch"})
	c.rejectedCLI("fresh workspace", "sleeve", "spawn", "dispatch", "--workspace", repo, "--new-branch", "another", "--harness", "claude")
	linked := filepath.Join(c.root, "linked-source")
	git(repo, "worktree", "add", "-b", "linked/base", linked)
	sha256 := filepath.Join(c.root, "sha256-source")
	git(c.root, "init", "--object-format=sha256", "--initial-branch=main", sha256)
	git(sha256, "commit", "--allow-empty", "-m", "sha256 base")
	git(sha256, "remote", "add", "origin", "https://user:secret@example.invalid/owner/repo.git")
	for _, tc := range []struct {
		name, source, branch, base string
		flags                      []string
	}{
		{"current", repo, "feature/current", "main", []string{"--new-branch", "feature/current"}},
		{"selected", repo, "dispatch/base", "dispatch/base", []string{"--branch", "dispatch/base"}},
		{"url", "file://" + repo, "feature/url", "", []string{"--branch", "dispatch/base", "--new-branch", "feature/url"}},
		{"default-url", "file://" + repo, "main", "", nil},
		{"inplace", repo, "main", "", nil},
		{"linked", linked, "feature/linked", "linked/base", []string{"--new-branch", "feature/linked"}},
		{"sha256", sha256, "feature/sha256", "main", []string{"--new-branch", "feature/sha256"}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			local := *c
			local.t = t
			args := []string{"sleeve", "spawn", tc.name, "--workspace", tc.source, "--harness", "shell", "--json"}
			args = append(args, tc.flags...)
			command := "exec sleep 60"
			if tc.base != "" {
				command = "git fetch base && printf fetched > fetch-proof; exec sleep 60"
			}
			args = append(args, "--", "sh", "-c", command)
			var spawned pb.SpawnSleeveResponse
			if err := protojson.Unmarshal([]byte(local.cli(args...)), &spawned); err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { local.rpc("SleeveService", "KillSleeve", map[string]any{"name": tc.name}, nil) })
			s := spawned.Sleeve
			if s.GitBranch != tc.branch || s.BaseBranch != tc.base {
				t.Fatalf("branch defaults: %v", s)
			}
			if tc.name == "inplace" && s.Workspace != repo {
				t.Fatalf("in-place directory cloned: %v", s)
			}
			if tc.base != "" {
				local.eventually(func() bool {
					raw, err := os.ReadFile(filepath.Join(s.Workspace, "fetch-proof"))
					return err == nil && string(raw) == "fetched"
				})
				if got := git(s.Workspace, "remote", "get-url", "origin"); got != "https://example.invalid/owner/repo.git" {
					t.Fatalf("origin=%q", got)
				}
			}
			local.call("SleeveService", "KillSleeve", map[string]any{"name": tc.name})
		})
	}
	var reuse pb.SpawnSleeveResponse
	out = c.cli("sleeve", "spawn", "reuse", "--workspace", repo, "--branch", "dispatch/base", "--harness", "shell", "--json", "--", "sleep", "1")
	if err := protojson.Unmarshal([]byte(out), &reuse); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { c.rpc("SleeveService", "KillSleeve", map[string]any{"name": "reuse"}, nil) })
	c.eventually(func() bool {
		var current pb.GetSleeveResponse
		if err := protojson.Unmarshal([]byte(c.cli("sleeve", "show", "reuse", "--json")), &current); err != nil {
			t.Fatal(err)
		}
		return current.GetSleeve().GetState() == pb.SleeveState_SLEEVE_STATE_EXITED
	})
	out = c.cli("sleeve", "spawn", "reuse", "--workspace", reuse.Sleeve.Workspace, "--harness", "shell", "--json", "--", "sleep", "60")
	if err := protojson.Unmarshal([]byte(out), &reuse); err != nil {
		t.Fatal(err)
	}
	if reuse.Sleeve.BaseRepo != repo || reuse.Sleeve.BaseBranch != "dispatch/base" {
		t.Fatalf("respawn lost base: %v", reuse.Sleeve)
	}
	c.call("SleeveService", "KillSleeve", map[string]any{"name": "reuse"})
}

func TestBinaryBaseNotices(t *testing.T) {
	c := newCluster(t)
	probe, cancel := context.WithTimeout(c.ctx, 3*time.Second)
	defer cancel()
	if out, err := exec.CommandContext(probe, "bwrap", "--ro-bind", "/", "/", "--unshare-all", "--", "true").CombinedOutput(); err != nil {
		t.Skipf("base history requires bwrap namespaces: %v: %s", err, out)
	}
	source, clone := filepath.Join(c.root, "base-source"), filepath.Join(c.root, "base-worker")
	if err := os.Mkdir(source, 0700); err != nil {
		t.Fatal(err)
	}
	git := func(dir string, args ...string) string {
		t.Helper()
		cmd := exec.CommandContext(c.ctx, "git", append([]string{"-C", dir, "-c", "core.fsmonitor=false", "-c", "log.showSignature=false"}, args...)...)
		cmd.Env = append(c.env, "GIT_CONFIG_GLOBAL=/dev/null", "GIT_CONFIG_NOSYSTEM=1", "GIT_AUTHOR_NAME=E2E", "GIT_AUTHOR_EMAIL=e2e@example.invalid", "GIT_COMMITTER_NAME=E2E", "GIT_COMMITTER_EMAIL=e2e@example.invalid")
		out, err := cmd.CombinedOutput()
		if err != nil {
			t.Fatalf("git %v: %v: %s", args, err, out)
		}
		return strings.TrimSpace(string(out))
	}
	git(source, "init", "--initial-branch=main")
	git(source, "commit", "--allow-empty", "-m", "base start")
	old := git(source, "rev-parse", "HEAD")
	git(c.root, "clone", source, clone)
	c.stop(c.hub)
	raw, err := json.Marshal(map[string]any{"sleeves": []any{map[string]any{
		"name": "base-worker", "harness": "claude", "mode": "chat", "workspace": clone,
		"state": "running", "created": "2026-01-01T00:00:00Z", "base_repo": source, "base_branch": "main", "base_tip": old,
		"chat": map[string]any{"state": "CHAT_STATE_FAILED"},
	}}})
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(c.home, "sleeves.json"), raw, 0600); err != nil {
		t.Fatal(err)
	}
	c.boot()
	for _, subject := range []string{"first merged change", "second merged change"} {
		git(source, "commit", "--allow-empty", "-m", subject)
	}
	tip := git(source, "rev-parse", "HEAD")
	git(source, "pack-refs", "--all", "--prune")
	marker := filepath.Join(c.root, "repository-program-ran")
	git(source, "config", "core.fsmonitor", "touch "+marker)
	git(source, "config", "log.showSignature", "true")
	git(source, "config", "gpg.program", "touch "+marker)
	currentTip := func() string {
		var current pb.GetSleeveResponse
		if err := protojson.Unmarshal([]byte(c.cli("sleeve", "show", "base-worker", "--json")), &current); err != nil {
			t.Fatal(err)
		}
		return current.GetSleeve().GetBaseTip()
	}
	c.eventually(func() bool { return currentTip() == tip })
	if _, err := os.Stat(marker); !os.IsNotExist(err) {
		t.Fatalf("base inspection ran repository program: %v", err)
	}
	notices := func() []string {
		raw, err := os.ReadFile(filepath.Join(c.home, "mailbox.jsonl"))
		if err != nil {
			t.Fatal(err)
		}
		var bodies []string
		for _, line := range strings.Split(string(raw), "\n") {
			var row struct {
				Msg struct{ To, Body string } `json:"msg"`
			}
			if json.Unmarshal([]byte(line), &row) == nil && row.Msg.To == "base-worker" && strings.HasPrefix(row.Msg.Body, "Base branch ") {
				bodies = append(bodies, row.Msg.Body)
			}
		}
		return bodies
	}
	bodies := notices()
	if len(bodies) != 1 {
		t.Fatalf("coalesced notices: %v", bodies)
	}
	for _, want := range []string{old[:7] + ".." + tip[:7], "first merged change", "second merged change", "git fetch base && git rebase base/main"} {
		if !strings.Contains(bodies[0], want) {
			t.Fatalf("notice missing %q: %s", want, bodies[0])
		}
	}
	c.stop(c.hub)
	c.boot()
	git(clone, "fetch", "origin")
	git(clone, "merge", "--ff-only", "origin/main")
	git(clone, "commit", "--allow-empty", "-m", "worker's own commit")
	own := git(clone, "rev-parse", "HEAD")
	git(source, "fetch", clone, "main")
	git(source, "update-ref", "refs/heads/main", own)
	c.eventually(func() bool { return currentTip() == own })
	if bodies := notices(); len(bodies) != 1 {
		t.Fatalf("restart/self notices: %v", bodies)
	}
	if _, err := os.Stat(marker); !os.IsNotExist(err) {
		t.Fatalf("repository program escaped: %v", err)
	}
}
