package e2e

import (
	"net"
	"os"
	"testing"
	"time"
)

func TestBinaryRustCancelBeforeSubmit(t *testing.T) {
	requireRustE2E(t)
	c := newCluster(t)
	const key = "cancel-before-submit-rust-e2e"
	if status := c.rpc("JobService", "CancelJob", map[string]any{"requestId": key}, nil); status != 404 {
		t.Fatalf("cancel unseen request: %d", status)
	}
	c.stop(c.hub)
	c.boot()
	for range 2 {
		if status := c.rpc("JobService", "SubmitJob", map[string]any{"requestId": key, "target": map[string]any{"node": "peer"}, "command": []string{"true"}}, nil); status != 409 {
			t.Fatalf("late submit after cancellation: %d", status)
		}
	}
	if jobs := c.call("JobService", "ListJobs", map[string]any{})["jobs"]; jobs != nil && len(jobs.([]any)) != 0 {
		t.Fatalf("canceled request started a job: %v", jobs)
	}
}

func TestBinaryRustJobCancelRestart(t *testing.T) {
	requireRustE2E(t)
	c := newCluster(t)
	const key = "rust-cancel-running-job-0001"
	job := c.call("JobService", "SubmitJob", map[string]any{"requestId": key, "target": map[string]any{"node": "peer"}, "command": []string{"sleep", "60"}})
	id := job["id"]
	c.eventually(func() bool {
		return c.call("JobService", "GetJob", map[string]any{"id": id})["state"] == "JOB_STATE_RUNNING"
	})
	c.call("JobService", "CancelJob", map[string]any{"id": id})
	c.eventually(func() bool {
		return c.call("JobService", "GetJob", map[string]any{"id": id})["state"] == "JOB_STATE_CANCELED"
	})
	c.stop(c.hub)
	c.boot()
	duplicate := c.call("JobService", "SubmitJob", map[string]any{"requestId": key, "target": map[string]any{"node": "peer"}, "command": []string{"false"}})
	if duplicate["id"] != id || duplicate["state"] != "JOB_STATE_CANCELED" {
		t.Fatalf("canceled job reran after restart: %v", duplicate)
	}
	if finished := c.call("JobService", "CancelJob", map[string]any{"requestId": key}); finished["state"] != "JOB_STATE_CANCELED" {
		t.Fatalf("repeat cancel changed terminal state: %v", finished)
	}
}

func TestBinaryRustJobCancellationReapsDescendant(t *testing.T) {
	requireRustE2E(t)
	c := newCluster(t)
	c.call("NodeService", "SetNodeTier", map[string]any{"name": "peer", "tier": "care"})
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	address := listener.Addr().String()
	if err := listener.Close(); err != nil {
		t.Fatal(err)
	}
	executable, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	const key = "rust-descendant-cancel-0001"
	command := []string{"sh", "-c", `"$1" -test.run=^TestBinaryRustPortHoldingDescendant$ & wait`, "sh", executable}
	request := map[string]any{"requestId": key, "target": map[string]any{"node": "peer"}, "command": command, "env": map[string]string{"PROT_E2E_PORT_HOLDER": address}}
	job := c.call("JobService", "SubmitJob", request)
	id := job["id"]
	defer c.rpc("JobService", "CancelJob", map[string]any{"id": id}, nil)
	c.eventually(func() bool {
		conn, err := net.DialTimeout("tcp", address, 100*time.Millisecond)
		if err != nil {
			return false
		}
		conn.Close()
		return c.call("JobService", "GetJob", map[string]any{"id": id})["state"] == "JOB_STATE_RUNNING"
	})
	if replay := c.call("JobService", "SubmitJob", request); replay["id"] != id {
		t.Fatalf("same request started a second job: first=%v replay=%v", id, replay)
	}
	c.call("JobService", "CancelJob", map[string]any{"id": id})
	c.eventually(func() bool {
		return c.call("JobService", "GetJob", map[string]any{"id": id})["state"] == "JOB_STATE_CANCELED"
	})
	c.eventually(func() bool {
		probe, err := net.Listen("tcp", address)
		if err != nil {
			return false
		}
		probe.Close()
		return true
	})
	c.stop(c.hub)
	c.boot()
	request["command"] = []string{"false"}
	if replay := c.call("JobService", "SubmitJob", request); replay["id"] != id || replay["state"] != "JOB_STATE_CANCELED" {
		t.Fatalf("canceled job reran after restart: first=%v replay=%v", id, replay)
	}
}

func TestBinaryRustPortHoldingDescendant(t *testing.T) {
	address := os.Getenv("PROT_E2E_PORT_HOLDER")
	if address == "" {
		t.Skip("job descendant helper")
	}
	listener, err := net.Listen("tcp", address)
	if err != nil {
		t.Fatal(err)
	}
	defer listener.Close()
	if err := listener.(*net.TCPListener).SetDeadline(time.Now().Add(2 * time.Minute)); err != nil {
		t.Fatal(err)
	}
	for {
		conn, err := listener.Accept()
		if err != nil {
			t.Fatal(err)
		}
		conn.Close()
	}
}

func requireRustE2E(t *testing.T) {
	t.Helper()
	if os.Getenv("PROT_E2E_COMPANION_BINARY") == "" {
		t.Skip("Rust real-binary parity: set PROT_E2E_COMPANION_BINARY")
	}
}
