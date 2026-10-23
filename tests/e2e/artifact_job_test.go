package e2e

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestBinaryArtifactJobAndReplay(t *testing.T) {
	c := newCluster(t)
	source := filepath.Join(c.root, "artifact-source")
	if err := os.MkdirAll(filepath.Join(source, "pkg"), 0700); err != nil {
		t.Fatal(err)
	}
	payload := make([]byte, (1<<20)+137)
	for i := range payload {
		payload[i] = byte((i >> 11) ^ i)
	}
	if err := os.WriteFile(filepath.Join(source, "payload"), payload, 0600); err != nil {
		t.Fatal(err)
	}
	script := "#!/bin/sh\nset -eu\nprintf archive-start\ntest ! -e skipped\ntest -n \"$SECRET\"\nmkdir out\ncp ../payload out/result\nchmod 0750 out/result\nprintf archive-end\n"
	if err := os.WriteFile(filepath.Join(source, "pkg", "run"), []byte(script), 0750); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(filepath.Join(source, "payload"), filepath.Join(source, "pkg", "skipped")); err != nil {
		t.Fatal(err)
	}
	secret := "artifact-env-must-never-be-persisted"
	c.cli("job", "run", "--on", "peer", "--artifact", source, "--workdir", "pkg", "--artifact-glob", "out/*", "--env", "SECRET="+secret, "--", "./run")
	jobs := c.call("JobService", "ListJobs", map[string]any{"limit": 1})["jobs"].([]any)
	if len(jobs) != 1 {
		t.Fatalf("artifact run created %d jobs", len(jobs))
	}
	job := jobs[0].(map[string]any)
	id := job["id"].(string)
	if job["state"] != "JOB_STATE_SUCCEEDED" || job["sourceBytes"] == nil || !strings.Contains(c.cli("job", "ls"), id) {
		t.Fatalf("artifact job did not complete with a source upload: %v", job)
	}
	artifact := filepath.Join(c.home, "jobs", id, "artifacts", "out", "result")
	if c.rust {
		artifact = filepath.Join(c.home, "jobs-rust", id+".artifacts", "out", "result")
	}
	if b, err := os.ReadFile(artifact); err != nil || !bytes.Equal(b, payload) {
		t.Fatalf("artifact job corrupted its binary output: bytes=%d err=%v job=%v", len(b), err, job)
	}
	if info, err := os.Stat(artifact); err != nil || info.Mode().Perm() != 0750 {
		t.Fatalf("artifact return lost executable permissions: %v %v", info, err)
	}
	record := filepath.Join(c.home, "jobs", id, "job.json")
	if c.rust {
		record = filepath.Join(c.home, "jobs-rust", id+".json")
	}
	raw, err := os.ReadFile(record)
	if err != nil || bytes.Contains(raw, []byte(secret)) || !bytes.Contains(raw, []byte("SECRET")) {
		t.Fatalf("job persistence exposed environment values or lost their keys: %s %v", raw, err)
	}
	c.stop(c.hub)
	c.boot()
	if got := c.cli("job", "logs", id); got != "archive-startarchive-end" {
		t.Fatalf("artifact job log changed after restart: %q", got)
	}
	if got := c.cli("job", "show", id, "--json"); !strings.Contains(got, "JOB_STATE_SUCCEEDED") || strings.Contains(got, secret) {
		t.Fatalf("artifact job record changed after restart: %s", got)
	}
	for _, workdir := range []string{"../escape", "/absolute"} {
		if status := c.rpc("JobService", "SubmitJob", map[string]any{"target": map[string]any{"node": "peer"}, "source": map[string]any{"artifact": map[string]any{"path": source, "workdir": workdir}}, "command": []string{"true"}}, nil); status != 400 {
			t.Fatalf("artifact workdir escaped its uploaded tree: %s status=%d", workdir, status)
		}
	}
	single := filepath.Join(c.root, "single-source")
	if err := os.WriteFile(single, []byte("#!/bin/sh\nprintf single-source-executed\n"), 0600); err != nil {
		t.Fatal(err)
	}
	c.eventually(func() bool {
		return strings.Contains(c.cli("node", "show", "peer", "--json"), "NODE_FRESHNESS_FRESH")
	})
	if output := c.cli("job", "run", "--on", "peer", "--artifact", single, "--", "./single-source"); !strings.Contains(output, "single-source-executed") {
		t.Fatalf("single-file artifact was not made executable: %s", output)
	}
	if c.rust {
		archive := strings.Split(strings.TrimSpace(c.cli("hub", "backup", "--to", filepath.Join(c.root, "backups"))), "\n")[0]
		c.stop(c.hub)
		restored := filepath.Join(c.root, "restored-artifact-hub")
		c.cli("hub", "restore", archive, "--home", restored)
		c.home = restored
		c.env = append(c.env, "PROTECTORATE_HOME="+restored)
		c.boot()
		if got := c.cli("job", "logs", id); got != "archive-startarchive-end" {
			t.Fatalf("restored artifact job log changed: %q", got)
		}
		if got := c.call("JobService", "GetJob", map[string]any{"id": id}); got["state"] != "JOB_STATE_SUCCEEDED" {
			t.Fatalf("restored artifact job record changed: %v", got)
		}
		restoredArtifact := filepath.Join(restored, "jobs-rust", id+".artifacts", "out", "result")
		if b, err := os.ReadFile(restoredArtifact); err != nil || !bytes.Equal(b, payload) {
			t.Fatalf("restored artifact bytes changed: bytes=%d err=%v", len(b), err)
		}
		if info, err := os.Stat(restoredArtifact); err != nil || info.Mode().Perm() != 0750 {
			t.Fatalf("restored artifact permissions changed: %v %v", info, err)
		}
	}
}
