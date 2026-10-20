package e2e

import (
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"testing"

	"connectrpc.com/connect"
	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

func TestBinaryJobCancellationAndIdempotency(t *testing.T) {
	c := newCluster(t)
	key := "cancel-before-submit-0001"
	if status := c.rpc("JobService", "CancelJob", map[string]any{"requestId": key}, nil); status != 404 {
		t.Fatalf("cancel unseen request: %d", status)
	}
	c.stop(c.hub)
	c.boot()
	if status := c.rpc("JobService", "SubmitJob", map[string]any{"requestId": key, "target": map[string]any{"node": "peer"}, "command": []string{"true"}}, nil); status != 409 {
		t.Fatalf("persisted cancellation allowed a late submit: %d", status)
	}
	if status := c.rpc("JobService", "SubmitJob", map[string]any{"requestId": "retry-failed-submit", "target": map[string]any{"node": "missing"}, "command": []string{"true"}}, nil); status == 200 {
		t.Fatal("missing node accepted")
	}
	retried := c.call("JobService", "SubmitJob", map[string]any{"requestId": "retry-failed-submit", "target": map[string]any{"node": "peer"}, "command": []string{"true"}})
	c.eventually(func() bool {
		return c.call("JobService", "GetJob", map[string]any{"id": retried["id"]})["state"] == "JOB_STATE_SUCCEEDED"
	})
	// Plain jobs report PIDs in this namespace, including on sandbox-capable hosts.
	c.call("NodeService", "SetNodeTier", map[string]any{"name": "peer", "tier": "care"})

	client := protectoratev1connect.NewJobServiceClient(c.client, c.url)
	type result struct {
		id  string
		err error
	}
	results := make(chan result, 8)
	start := make(chan struct{})
	var workers sync.WaitGroup
	for range 8 {
		workers.Go(func() {
			<-start
			req := connect.NewRequest(&pb.SubmitJobRequest{RequestId: "concurrent-submit-0001", Target: &pb.JobTarget{Target: &pb.JobTarget_Node{Node: "peer"}}, Command: []string{"sh", "-c", `(trap '' TERM; exec sleep 60) & printf 'child=%s\n' "$!"; wait`}})
			req.Header().Set("Authorization", "Bearer "+c.token)
			res, err := client.SubmitJob(c.ctx, req)
			var id string
			if res != nil {
				id = res.Msg.Id
			}
			results <- result{id, err}
		})
	}
	close(start)
	workers.Wait()
	close(results)
	var id string
	for result := range results {
		if result.err != nil || result.id == "" || (id != "" && id != result.id) {
			t.Fatalf("concurrent submit created inconsistent jobs: previous=%s result=%+v", id, result)
		}
		id = result.id
	}
	var child int
	c.eventually(func() bool {
		b, _ := os.ReadFile(filepath.Join(c.home, "jobs", id, "job.log"))
		_, err := fmt.Sscanf(string(b), "child=%d", &child)
		return err == nil && child > 0
	})
	owner := c.token
	reader := c.call("TokenService", "CreateToken", map[string]any{"name": "cannot-cancel", "scopes": []string{"read"}})["secret"].(string)
	c.token = reader
	if status := c.rpc("JobService", "CancelJob", map[string]any{"id": id}, nil); status != 403 {
		t.Fatalf("reader canceled another job: %d", status)
	}
	c.token = owner
	c.cli("job", "cancel", id)
	c.eventually(func() bool {
		return c.call("JobService", "GetJob", map[string]any{"id": id})["state"] == "JOB_STATE_CANCELED"
	})
	c.eventually(func() bool {
		b, err := os.ReadFile(filepath.Join("/proc", strconv.Itoa(child), "stat"))
		if os.IsNotExist(err) {
			return true
		}
		end := strings.LastIndexByte(string(b), ')')
		return end >= 0 && strings.HasPrefix(string(b[end+1:]), " Z ")
	})
	c.stop(c.hub)
	c.boot()
	if duplicate := c.call("JobService", "SubmitJob", map[string]any{"requestId": "concurrent-submit-0001", "target": map[string]any{"node": "peer"}, "command": []string{"false"}}); duplicate["id"] != id || duplicate["state"] != "JOB_STATE_CANCELED" {
		t.Fatalf("canceled job reran after restart: %v", duplicate)
	}
	if finished := c.call("JobService", "CancelJob", map[string]any{"requestId": "concurrent-submit-0001"}); finished["state"] != "JOB_STATE_CANCELED" {
		t.Fatalf("repeat cancel changed terminal state: %v", finished)
	}
	timed := c.call("JobService", "SubmitJob", map[string]any{"target": map[string]any{"node": "peer"}, "command": []string{"sleep", "60"}, "timeout": "0.2s"})
	c.eventually(func() bool {
		return c.call("JobService", "GetJob", map[string]any{"id": timed["id"]})["state"] == "JOB_STATE_TIMED_OUT"
	})
	c.eventually(func() bool {
		return strings.Contains(fmt.Sprint(c.call("NodeService", "GetNode", map[string]any{"name": "peer"})), "NODE_FRESHNESS_FRESH")
	})
	if output := c.cli("job", "run", "--on", "peer", "--", "printf", "capacity-recovered"); !strings.Contains(output, "capacity-recovered") {
		t.Fatalf("cancellation or timeout leaked the node's execution slot: %s", output)
	}
}
