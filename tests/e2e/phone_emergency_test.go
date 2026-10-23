package e2e

import (
	"context"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"
	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

func TestBinaryPhoneEmergencyStop(t *testing.T) {
	c := newCluster(t)
	c.requireRuntime(t)
	owner := c.token
	phone := c.call("TokenService", "CreateToken", map[string]any{
		"name": "phone-emergency", "scopes": []string{"read", "sleeve_kill_sleeve", "chat_*"},
	})["secret"].(string)
	reader := c.call("TokenService", "CreateToken", map[string]any{
		"name": "phone-readonly", "scopes": []string{"read"},
	})["secret"].(string)
	if !c.rust {
		c.call("SleeveService", "SpawnEnvoy", map[string]any{"harness": "HARNESS_CLAUDE"})
	}
	t.Cleanup(func() {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		previous := c.ctx
		c.ctx = ctx
		defer func() { c.ctx = previous }()
		c.token = owner
		for _, name := range []string{"worker-a", "worker-b", "envoy"} {
			c.rpc("SleeveService", "KillSleeve", map[string]any{"name": name}, nil)
		}
	})
	cfg, err := sleeve.DefaultConfig(c.home, nil)
	if err != nil {
		t.Fatalf("load runtime config: %v", err)
	}
	cases := []struct {
		name     string
		hasEnvoy bool
	}{
		{"envoy running", true},
		{"envoy unavailable", false},
	}
	if c.rust {
		cases = cases[1:]
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			previous := c.t
			c.t = t
			defer func() { c.t = previous; c.token = owner }()
			c.token = owner
			if !tc.hasEnvoy && !c.rust {
				c.call("SleeveService", "KillSleeve", map[string]any{"name": "envoy"})
			}
			identities := make(map[string]string)
			for _, name := range []string{"worker-a", "worker-b"} {
				workspace := filepath.Join(c.root, strings.ReplaceAll(tc.name, " ", "-"), name)
				if err := os.MkdirAll(workspace, 0700); err != nil {
					t.Fatalf("create worker workspace: %v", err)
				}
				c.call("SleeveService", "SpawnSleeve", map[string]any{
					"name": name, "harness": "HARNESS_SHELL", "workspace": workspace,
					"args": []string{"sh", "-c", "sleep 600 & printf ready > proof; wait"},
				})
				c.eventually(func() bool {
					b, err := os.ReadFile(filepath.Join(workspace, "proof"))
					return err == nil && string(b) == "ready"
				})
				b, err := os.ReadFile(filepath.Join(c.home, "sleeves", name, "home", ".protectorate-token"))
				if err != nil {
					t.Fatalf("read worker identity: %v", err)
				}
				identities[name] = strings.TrimSpace(string(b))
			}
			c.token = reader
			if status := c.rpc("SleeveService", "KillSleeve", map[string]any{"name": "worker-a"}, nil); status != http.StatusForbidden {
				t.Fatalf("read-only phone stopped a sleeve: HTTP %d", status)
			}
			c.token = phone
			listed := c.call("SleeveService", "ListSleeves", map[string]any{})
			stopped := make(map[string]bool)
			for _, entry := range listed["sleeves"].([]any) {
				info := entry.(map[string]any)
				name := info["name"].(string)
				if name == "envoy" {
					continue
				}
				c.call("SleeveService", "KillSleeve", map[string]any{"name": name})
				stopped[name] = true
			}
			if len(stopped) != len(identities) {
				t.Fatalf("emergency stop missed workers: stopped=%v listed=%v", stopped, listed)
			}
			for name, identity := range identities {
				if !stopped[name] {
					t.Fatalf("worker %s was not stopped", name)
				}
				info := c.call("SleeveService", "GetSleeve", map[string]any{"name": name})["sleeve"].(map[string]any)
				if info["state"] != "SLEEVE_STATE_EXITED" || info["killed"] != true {
					t.Fatalf("worker %s survived stop: %v", name, info)
				}
				if cfg.CgroupRoot != "" {
					root := filepath.Join(cfg.CgroupRoot, cfg.UnitName(name)+".scope")
					c.eventually(func() bool { _, err := os.Stat(root); return os.IsNotExist(err) })
				}
				c.token = identity
				if status := c.rpc("HubService", "GetStatus", map[string]any{}, nil); status != http.StatusUnauthorized {
					t.Fatalf("stopped worker %s retained authority: HTTP %d", name, status)
				}
				c.token = phone
			}
			if !tc.hasEnvoy {
				return
			}
			info := c.call("SleeveService", "GetSleeve", map[string]any{"name": "envoy"})["sleeve"].(map[string]any)
			if info["state"] != "SLEEVE_STATE_RUNNING" {
				t.Fatalf("emergency stop killed Envoy: %v", info)
			}
			c.call("ChatService", "Send", map[string]any{"sleeve": "envoy", "text": "ping", "requestId": "phone-after-emergency"})
			ctx, cancel := context.WithTimeout(c.ctx, 5*time.Second)
			defer cancel()
			client := protectoratev1connect.NewChatServiceClient(c.client, c.url)
			req := connect.NewRequest(&pb.WatchRequest{Sleeve: "envoy"})
			req.Header().Set("Authorization", "Bearer "+phone)
			stream, err := client.Watch(ctx, req)
			if err != nil {
				t.Fatalf("watch surviving Envoy: %v", err)
			}
			defer stream.Close()
			var text string
			for stream.Receive() {
				if chunk := stream.Msg().GetText(); chunk != nil {
					text += chunk.Delta
				}
				if stream.Msg().GetSettled() != nil {
					if text != "pong" {
						t.Fatalf("surviving Envoy reply: %q", text)
					}
					return
				}
			}
			t.Fatalf("surviving Envoy did not settle: %v", stream.Err())
		})
	}
}
