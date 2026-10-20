package e2e

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"google.golang.org/protobuf/reflect/protoreflect"
	"google.golang.org/protobuf/reflect/protoregistry"
)

func (c *cluster) securityScenario(t *testing.T) {
	previous := c.t
	c.t = t
	defer func() { c.t = previous }()
	owner := c.token
	defer func() { c.token = owner }()
	expiring := c.call("TokenService", "CreateToken", map[string]any{"name": "expiry", "scopes": []string{"read"}, "ttl": "0.001s"})["secret"].(string)
	c.token = expiring
	c.eventually(func() bool { return c.rpc("HubService", "GetStatus", map[string]any{}, nil) == 401 })
	c.token = owner
	issued := c.call("TokenService", "CreateToken", map[string]any{"name": "orchestrator-envoy", "scopes": []string{"sleeve_spawn_sleeve"}})
	secret := issued["secret"].(string)
	id := issued["token"].(map[string]any)["id"]
	for _, scopes := range []struct {
		add, remove []string
		want        int
	}{
		{[]string{"read"}, nil, 200}, {nil, []string{"read", "sleeve_spawn_sleeve"}, 403}, {[]string{"read"}, nil, 200},
	} {
		c.token = secret
		if s := c.rpc("TokenService", "UpdateTokenScopes", map[string]any{"id": id, "add": []string{"*"}}, nil); s != 403 {
			t.Fatalf("self widening: %d", s)
		}
		c.token = owner
		c.call("TokenService", "UpdateTokenScopes", map[string]any{"id": id, "add": scopes.add, "remove": scopes.remove})
		c.stop(c.hub)
		c.boot()
		c.token = secret
		if s := c.rpc("HubService", "GetStatus", map[string]any{}, nil); s != scopes.want {
			t.Fatalf("persisted scope status=%d want=%d", s, scopes.want)
		}
	}
	c.token = owner
	entries, err := filepath.Glob(filepath.Join(c.home, "audit", "*.jsonl"))
	if err != nil {
		t.Fatal(err)
	}
	changes := 0
	for _, path := range entries {
		b, err := os.ReadFile(path)
		if err != nil {
			t.Fatal(err)
		}
		if strings.Contains(string(b), secret) {
			t.Fatal("audit leaked token")
		}
		for _, line := range strings.Split(strings.TrimSpace(string(b)), "\n") {
			var e map[string]any
			if err := json.Unmarshal([]byte(line), &e); err != nil {
				t.Fatal(err)
			}
			if strings.HasSuffix(fmt.Sprint(e["procedure"]), "/UpdateTokenScopes") && e["outcome"] == "ok" {
				changes++
				if e["actor"] != "owner" || !strings.Contains(fmt.Sprint(e["detail"]), "scopes ") {
					t.Fatalf("scope audit=%v", e)
				}
			}
		}
	}
	if changes < 3 {
		t.Fatalf("scope audit changes=%d", changes)
	}
	c.call("TokenService", "UpdateTokenScopes", map[string]any{"id": id, "remove": []string{"read"}})
	c.token = secret
	checked := 0
	protoregistry.GlobalFiles.RangeFiles(func(file protoreflect.FileDescriptor) bool {
		if file.Package() != "protectorate.v1" {
			return true
		}
		services := file.Services()
		for i := 0; i < services.Len(); i++ {
			service := services.Get(i)
			if service.Name() == "NodeAgentService" {
				continue
			}
			methods := service.Methods()
			for j := 0; j < methods.Len(); j++ {
				method := methods.Get(j)
				if method.IsStreamingClient() || method.IsStreamingServer() {
					continue
				}
				if status := c.rpc(string(service.Name()), string(method.Name()), map[string]any{}, nil); status != 403 {
					t.Fatalf("empty scopes admitted %s/%s: %d", service.Name(), method.Name(), status)
				}
				checked++
			}
		}
		return true
	})
	if checked < 50 {
		t.Fatalf("authorization matrix only checked %d methods", checked)
	}
	c.token = owner
	c.call("NodeService", "SetNodeTier", map[string]any{"name": "peer", "tier": "care"})
	defer func() {
		c.token = owner
		c.call("NodeService", "SetNodeTier", map[string]any{"name": "peer", "tier": "shared"})
	}()
	plain := c.call("TokenService", "CreateToken", map[string]any{"name": "jobs", "scopes": []string{"job_*"}})["secret"].(string)
	grant := c.call("GrantService", "Admin", map[string]any{"node": "peer", "task": "e2e care authorization"})["secret"].(string)
	other := c.call("GrantService", "Admin", map[string]any{"node": "another", "task": "e2e wrong node"})["secret"].(string)
	for _, tc := range []struct {
		name, token string
		want        int
	}{{"plain", plain, 403}, {"owner grant", grant, 200}, {"other grant", other, 403}} {
		c.token = owner
		c.eventually(func() bool {
			return strings.Contains(fmt.Sprint(c.call("NodeService", "GetNode", map[string]any{"name": "peer"})), "NODE_FRESHNESS_FRESH")
		})
		c.token = tc.token
		var direct map[string]any
		if s := c.rpc("JobService", "SubmitJob", map[string]any{"target": map[string]any{"node": "peer"}, "command": []string{"true"}}, &direct); s != tc.want {
			t.Fatalf("care %s: %d want=%d", tc.name, s, tc.want)
		}
		if tc.want == 200 {
			c.eventually(func() bool {
				return c.call("JobService", "GetJob", map[string]any{"id": direct["id"]})["state"] == "JOB_STATE_SUCCEEDED"
			})
		} else if direct["code"] != "permission_denied" || !strings.Contains(fmt.Sprint(direct["message"]), "tier care") {
			t.Fatalf("care denial came from an unrelated gate: %v", direct)
		}
		var job map[string]any
		var status int
		// A poll that lands late under load makes a node briefly not fresh;
		// retry until the refusal, if any, names the care tier.
		c.eventually(func() bool {
			job = nil
			status = c.rpc("JobService", "SubmitJob", map[string]any{"target": map[string]any{"selector": map[string]any{}}, "command": []string{"true"}}, &job)
			return status != 400 || !strings.Contains(fmt.Sprint(job["message"]), "not fresh")
		})
		if status != 200 && status != 400 {
			t.Fatalf("selector status=%d", status)
		}
		if status == 400 && (job["code"] != "failed_precondition" || !strings.Contains(fmt.Sprint(job["message"]), "peer: tier care")) {
			t.Fatalf("selector refused for an unrelated reason: %v", job)
		}
		if job["node"] == "peer" {
			t.Fatalf("selector placed on care: %v", job)
		}
	}
	c.token = owner
}
