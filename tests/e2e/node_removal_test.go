package e2e

import (
	"fmt"
	"strings"
	"testing"

	"connectrpc.com/connect"
	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

func TestBinaryNodeDrainAndRemoval(t *testing.T) {
	c := newCluster(t)
	job := c.call("JobService", "SubmitJob", map[string]any{"target": map[string]any{"node": "peer"}, "command": []string{"sleep", "60"}})
	c.eventually(func() bool {
		return c.call("JobService", "GetJob", map[string]any{"id": job["id"]})["state"] == "JOB_STATE_RUNNING"
	})
	c.rejectedCLI("job(s) running", "node", "rm", "peer")
	client := protectoratev1connect.NewNodeServiceClient(c.client, c.url, connect.WithProtoJSON())
	req := connect.NewRequest(&pb.RemoveNodeRequest{Name: "peer", Jobs: pb.RemoveJobs_REMOVE_JOBS_WAIT})
	req.Header().Set("Authorization", "Bearer "+c.token)
	stream, err := client.RemoveNode(c.ctx, req)
	if err != nil {
		t.Fatal(err)
	}
	if !stream.Receive() || stream.Msg().Phase != pb.RemovePhase_REMOVE_PHASE_DRAINING || stream.Msg().RunningJobs != 1 {
		stream.Close()
		t.Fatalf("drain did not announce the live job: %v", stream.Err())
	}
	for _, call := range []struct {
		service, method string
		input           map[string]any
	}{
		{"JobService", "SubmitJob", map[string]any{"target": map[string]any{"node": "peer"}, "command": []string{"true"}}},
		{"LeaseService", "AcquireLease", map[string]any{"resource": "peer", "holder": "late", "ttl": "20s"}},
	} {
		if status := c.rpc(call.service, call.method, call.input, nil); status != 400 {
			stream.Close()
			t.Fatalf("draining node accepted %s: %d", call.method, status)
		}
	}
	stream.Close()
	var held map[string]any
	c.eventually(func() bool {
		return c.rpc("LeaseService", "AcquireLease", map[string]any{"resource": "peer", "holder": "remove-holder", "ttl": "60s"}, &held) == 200
	})
	if current := c.call("JobService", "GetJob", map[string]any{"id": job["id"]}); current["state"] != "JOB_STATE_RUNNING" {
		t.Fatalf("aborted removal killed the job: %v", current)
	}
	queued := c.call("LeaseService", "AcquireLease", map[string]any{"resource": "peer", "holder": "remove-waiter", "wait": true, "ttl": "60s"})
	if output := c.cli("node", "rm", "peer", "--jobs", "cancel"); !strings.Contains(output, "removed peer") {
		t.Fatalf("node removal did not finish: %s", output)
	}
	if current := c.call("JobService", "GetJob", map[string]any{"id": job["id"]}); current["state"] != "JOB_STATE_CANCELED" {
		t.Fatalf("node removal lost the running job: %v", current)
	}
	for _, record := range []map[string]any{held, queued} {
		if status := c.rpc("LeaseService", "RenewLease", map[string]any{"id": record["id"]}, nil); status != 404 {
			t.Fatalf("node removal retained lease/ticket %v: %d", record, status)
		}
	}
	c.stop(c.hub)
	c.boot()
	if status := c.rpc("NodeService", "GetNode", map[string]any{"name": "peer"}, nil); status != 404 {
		t.Fatalf("removed node returned after restart: %d", status)
	}
	if list := c.call("LeaseService", "ListLeases", map[string]any{}); strings.Contains(fmt.Sprint(list), "remove-") {
		t.Fatalf("removed node's leases returned after restart: %v", list)
	}
}
