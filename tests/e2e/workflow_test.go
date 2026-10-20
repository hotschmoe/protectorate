package e2e

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestBinaryClusterRestartAndRestore(t *testing.T) {
	c := newCluster(t)
	issued := c.call("TokenService", "CreateToken", map[string]any{"name": "reader", "scopes": []string{"read"}, "ttl": "3600s"})
	secret := issued["secret"].(string)
	owner := c.token
	c.token = secret
	if s := c.rpc("HubService", "GetStatus", map[string]any{}, nil); s != 200 {
		t.Fatalf("read token status=%d", s)
	}
	if s := c.rpc("TokenService", "CreateToken", map[string]any{"name": "escalation", "scopes": []string{"*"}}, nil); s != 403 {
		t.Fatalf("scope escalation status=%d", s)
	}
	c.token = "incorrect"
	if s := c.rpc("HubService", "GetStatus", map[string]any{}, nil); s != 401 {
		t.Fatalf("invalid bearer status=%d", s)
	}
	c.token = owner
	lease := c.call("LeaseService", "AcquireLease", map[string]any{"resource": "peer", "holder": "owner", "ttl": "60s"})
	if s := c.rpc("LeaseService", "AcquireLease", map[string]any{"resource": "peer", "holder": "other", "ttl": "60s"}, nil); s != 400 && s != 412 {
		t.Fatalf("conflicting lease status=%d", s)
	}
	c.call("LeaseService", "ReleaseLease", map[string]any{"id": lease["id"]})
	lease = c.call("LeaseService", "AcquireLease", map[string]any{"resource": "peer", "holder": "owner", "ttl": "60s"})
	mailRequest := map[string]any{"to": "owner", "body": "persist this request", "requestId": "mail-e2e-0001", "ackRequired": true}
	mail := c.call("MessageService", "Send", mailRequest)
	messageID := mail["message"].(map[string]any)["id"]
	input := filepath.Join(c.root, "upload")
	payload := strings.Repeat("binary transfer\x00\xff\n", 80000)
	if err := os.WriteFile(input, []byte(payload), 0600); err != nil {
		t.Fatal(err)
	}
	c.cli("node", "put", "peer", input, "roundtrip.bin")
	output := filepath.Join(c.root, "download")
	c.cli("node", "get", "peer", "roundtrip.bin", output)
	got, err := os.ReadFile(output)
	if err != nil || string(got) != payload {
		t.Fatalf("file roundtrip: bytes=%d err=%v", len(got), err)
	}
	c.rejectFileEscapes(input)
	notes := c.noteCRUD()
	jobRequest := map[string]any{"target": map[string]any{"leaseId": lease["id"]}, "command": []string{"sh", "-c", "printf 'before\\n'; sleep 3; printf 'after\\n'; printf artifact > result.txt"}, "artifactGlobs": []string{"result.txt"}, "requestId": "restart-e2e-0001", "timeout": "30s"}
	job := c.call("JobService", "SubmitJob", jobRequest)
	id := job["id"]
	c.eventually(func() bool {
		v := c.call("JobService", "GetJob", map[string]any{"id": id})
		return v["state"] == "JOB_STATE_RUNNING"
	})
	c.stop(c.hub)
	c.boot()
	c.checkNotes(notes)
	leases := c.call("LeaseService", "ListLeases", map[string]any{})
	if !strings.Contains(fmt.Sprint(leases), lease["id"].(string)) {
		t.Fatalf("lease lost across restart: %v", leases)
	}
	c.call("LeaseService", "ReleaseLease", map[string]any{"id": lease["id"]})
	duplicateMail := c.call("MessageService", "Send", mailRequest)
	if duplicateMail["duplicate"] != true || duplicateMail["message"].(map[string]any)["id"] != messageID {
		t.Fatalf("mail dedupe lost: %v", duplicateMail)
	}
	inbox := c.call("MessageService", "Inbox", map[string]any{"peek": true})
	if !strings.Contains(fmt.Sprint(inbox), messageID.(string)) {
		t.Fatalf("mail lost after restart: %v", inbox)
	}
	c.call("MessageService", "Ack", map[string]any{"id": messageID})
	c.eventually(func() bool {
		v := c.call("JobService", "GetJob", map[string]any{"id": id})
		return v["state"] == "JOB_STATE_SUCCEEDED"
	})
	if dup := c.call("JobService", "SubmitJob", jobRequest); dup["id"] != id {
		t.Fatalf("restart lost idempotency: %v != %v", dup["id"], id)
	}
	log, err := os.ReadFile(filepath.Join(c.home, "jobs", id.(string), "job.log"))
	if err != nil || strings.Count(string(log), "before") != 1 || strings.Count(string(log), "after") != 1 {
		t.Fatalf("resume output=%q err=%v", log, err)
	}
	artifact, err := os.ReadFile(filepath.Join(c.home, "jobs", id.(string), "artifacts", "result.txt"))
	if err != nil || string(artifact) != "artifact" {
		t.Fatalf("artifact=%q err=%v", artifact, err)
	}
	t.Run("large binary job output", c.outputScenario)
	t.Run("git bundle and warm worktree", c.gitScenario)
	t.Run("scopes and care tier", c.securityScenario)
	t.Run("diagnostics", func(t *testing.T) {
		previous := c.t
		c.t = t
		defer func() { c.t = previous }()
		c.diagnosticsScenario(t)
	})
	t.Run("binary transports", func(t *testing.T) {
		previous := c.t
		c.t = t
		defer func() { c.t = previous }()
		c.transportScenario(t)
	})
	t.Run("node distribution and update", func(t *testing.T) { previous := c.t; c.t = t; defer func() { c.t = previous }(); c.updateScenario(t) })
	t.Run("lease priority and FIFO across restart", c.leaseQueueScenario)
	t.Run("sleeve isolation", c.runtimeScenario)
	c.stop(c.hub)
	backup := c.cli("hub", "backup", "--to", filepath.Join(c.root, "backups"))
	archive := strings.Split(strings.TrimSpace(backup), "\n")[0]
	corrupt := filepath.Join(c.root, "corrupt.tar.zst")
	archiveBytes, err := os.ReadFile(archive)
	if err != nil || len(archiveBytes) == 0 {
		t.Fatalf("read backup for integrity probe: %v", err)
	}
	c.rejectTamperedBackups(archiveBytes)
	archiveBytes[0] ^= 0xff
	if err := os.WriteFile(corrupt, archiveBytes, 0600); err != nil {
		t.Fatal(err)
	}
	c.rejectedCLI("read archive", "hub", "restore", corrupt, "--home", filepath.Join(c.root, "rejected-restore"))
	restored := filepath.Join(c.root, "restored")
	c.cli("hub", "restore", archive, "--home", restored)
	c.home = restored
	c.env = append(c.env, "PROTECTORATE_HOME="+restored)
	c.boot()
	c.checkNotes(notes)
	if v := c.call("JobService", "GetJob", map[string]any{"id": id}); v["state"] != "JOB_STATE_SUCCEEDED" {
		t.Fatalf("restored job=%v", v)
	}
	c.token = secret
	if s := c.rpc("HubService", "GetStatus", map[string]any{}, nil); s != 200 {
		t.Fatalf("restored scoped token status=%d", s)
	}
	c.token = owner
	tokenID := issued["token"].(map[string]any)["id"]
	c.call("TokenService", "RevokeToken", map[string]any{"id": tokenID})
	c.rejectedCLI("empty", "hub", "restore", archive, "--home", restored)
	c.token = secret
	if s := c.rpc("HubService", "GetStatus", map[string]any{}, nil); s != 401 {
		t.Fatalf("revoked token status=%d", s)
	}
}
