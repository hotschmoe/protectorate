package e2e

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func (c *cluster) gitScenario(t *testing.T) {
	local := *c
	c = &local
	c.t = t
	if _, err := exec.LookPath("git"); err != nil {
		t.Skipf("git bundle scenario requires git: %v", err)
	}
	repo := filepath.Join(c.root, "repository")
	if err := os.MkdirAll(repo, 0700); err != nil {
		t.Fatal(err)
	}
	git := func(args ...string) string {
		cmd := exec.CommandContext(c.ctx, "git", args...)
		cmd.Dir = repo
		cmd.Env = append(c.env, "GIT_CONFIG_NOSYSTEM=1", "GIT_CONFIG_GLOBAL=/dev/null", "GIT_AUTHOR_NAME=E2E", "GIT_AUTHOR_EMAIL=e2e@example.invalid", "GIT_COMMITTER_NAME=E2E", "GIT_COMMITTER_EMAIL=e2e@example.invalid")
		out, err := cmd.CombinedOutput()
		if err != nil {
			t.Fatalf("git %v: %v: %s", args, err, out)
		}
		return strings.TrimSpace(string(out))
	}
	git("init", "--initial-branch=main")
	for _, version := range []string{"one", "two"} {
		if err := os.WriteFile(filepath.Join(repo, "tracked"), []byte(version), 0600); err != nil {
			t.Fatal(err)
		}
		git("add", "tracked")
		git("commit", "-m", version)
		job := c.call("JobService", "SubmitJob", map[string]any{"target": map[string]any{"node": "peer"}, "source": map[string]any{"git": map[string]any{"repo": repo, "commit": git("rev-parse", "HEAD")}}, "command": []string{"sh", "-c", "cat tracked; if test -f warm; then printf warm; fi; printf cache > warm"}, "timeout": "20s", "requestId": "git-e2e-" + version})
		id := job["id"].(string)
		c.eventually(func() bool {
			v := c.call("JobService", "GetJob", map[string]any{"id": id})
			if v["state"] == "JOB_STATE_FAILED" {
				t.Fatalf("git job failed: %v", v)
			}
			return v["state"] == "JOB_STATE_SUCCEEDED"
		})
		log, err := os.ReadFile(filepath.Join(c.home, "jobs", id, "job.log"))
		want := version
		if version == "two" {
			want += "warm"
		}
		if err != nil || string(log) != want {
			t.Fatalf("git revision/cache output=%q want=%q err=%v", log, want, err)
		}
	}
}
