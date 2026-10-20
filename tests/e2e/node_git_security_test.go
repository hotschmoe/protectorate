package e2e

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestBinaryNodeRepositoryProgramsStayIsolated(t *testing.T) {
	c := newCluster(t)
	repo := filepath.Join(c.root, "git-source")
	if err := os.Mkdir(repo, 0700); err != nil {
		t.Fatal(err)
	}
	git := func(args ...string) string {
		t.Helper()
		cmd := exec.CommandContext(c.ctx, "git", args...)
		cmd.Dir = repo
		cmd.Env = append(c.env, "GIT_CONFIG_NOSYSTEM=1", "GIT_CONFIG_GLOBAL=/dev/null", "GIT_AUTHOR_NAME=E2E", "GIT_AUTHOR_EMAIL=e2e@example.invalid", "GIT_COMMITTER_NAME=E2E", "GIT_COMMITTER_EMAIL=e2e@example.invalid")
		out, err := cmd.CombinedOutput()
		if err != nil {
			t.Fatalf("git %v: %v: %s", args, err, out)
		}
		return strings.TrimSpace(string(out))
	}
	git("init", "--quiet")
	if err := os.WriteFile(filepath.Join(repo, "tracked"), []byte("tracked"), 0600); err != nil {
		t.Fatal(err)
	}
	git("add", "tracked")
	git("commit", "--quiet", "-m", "fixture")
	commit := git("rev-parse", "HEAD")
	run := func(command []string) string {
		t.Helper()
		job := c.call("JobService", "SubmitJob", map[string]any{
			"target": map[string]any{"node": "peer"}, "source": map[string]any{"git": map[string]any{"repo": repo, "commit": commit}},
			"command": command, "timeout": "20s",
		})
		id := job["id"].(string)
		c.eventually(func() bool {
			job = c.call("JobService", "GetJob", map[string]any{"id": id})
			if job["state"] == "JOB_STATE_FAILED" {
				t.Fatalf("git job failed: %v", job)
			}
			return job["state"] == "JOB_STATE_SUCCEEDED"
		})
		out, err := os.ReadFile(filepath.Join(c.home, "jobs", id, "job.log"))
		if err != nil {
			t.Fatal(err)
		}
		return string(out)
	}
	marker := filepath.Join(c.root, "host-hook-marker")
	run([]string{"sh", "-c", `printf '#!/bin/sh\nprintf escaped > "%s"\n' "$1" > .git/hooks/post-checkout; chmod 700 .git/hooks/post-checkout`, "sh", marker})
	if got := run([]string{"cat", "tracked"}); got != "tracked" {
		t.Fatalf("warm checkout output=%q", got)
	}
	if _, err := os.Stat(marker); !os.IsNotExist(err) {
		t.Fatalf("node checkout executed an untrusted hook on the host: %v", err)
	}
	t.Run("filters have only job authority", func(t *testing.T) {
		status := c.call("NodeService", "GetNode", map[string]any{"name": "peer"})
		if !strings.Contains(fmt.Sprint(status), "bwrap") {
			t.Skip("filter confinement requires a node with working bwrap and systemd user manager")
		}
		secret := filepath.Join(c.root, "host-secret")
		if err := os.WriteFile(secret, []byte("host credential"), 0600); err != nil {
			t.Fatal(err)
		}
		run([]string{"sh", "-c", `printf '#!/bin/sh\nif test -f "%s"; then cat "%s"; else printf confined; fi\n' "$1" "$1" > .git/filter; chmod 700 .git/filter; git config filter.audit.smudge .git/filter; git config filter.audit.clean cat; git config filter.audit.required true; printf 'tracked filter=audit\n' > .git/info/attributes; rm tracked`, "sh", secret})
		if got := run([]string{"cat", "tracked"}); got != "confined" {
			t.Fatalf("repository filter read host credentials: %q", got)
		}
	})
}
