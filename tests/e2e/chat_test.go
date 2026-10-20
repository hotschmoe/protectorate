package e2e

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"
	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"google.golang.org/protobuf/encoding/protojson"
)

func TestMain(m *testing.M) {
	if len(os.Args) == 2 && os.Args[1] == "app-server" {
		codexProtocolPeer()
		os.Exit(0)
	}
	os.Exit(m.Run())
}

func codexProtocolPeer() {
	in, out := bufio.NewScanner(os.Stdin), json.NewEncoder(os.Stdout)
	model, effort := "gpt-fixture", "low"
	for in.Scan() {
		var req struct {
			ID     json.RawMessage `json:"id"`
			Method string          `json:"method"`
			Params struct {
				Model, Effort string
			} `json:"params"`
		}
		if json.Unmarshal(in.Bytes(), &req) != nil || len(req.ID) == 0 {
			continue
		}
		reply := func(result any) { _ = out.Encode(map[string]any{"id": req.ID, "result": result}) }
		note := func(method string, params any) { _ = out.Encode(map[string]any{"method": method, "params": params}) }
		switch req.Method {
		case "model/list":
			reply(map[string]any{"data": []any{
				map[string]any{"model": "gpt-fixture", "isDefault": true, "defaultReasoningEffort": "low", "supportedReasoningEfforts": []any{map[string]any{"reasoningEffort": "low"}, map[string]any{"reasoningEffort": "high"}, map[string]any{"reasoningEffort": "xhigh"}}},
				map[string]any{"model": "gpt-low", "defaultReasoningEffort": "low", "supportedReasoningEfforts": []any{map[string]any{"reasoningEffort": "low"}}},
			}})
		case "thread/start", "thread/resume":
			reply(map[string]any{"thread": map[string]any{"id": "fixture-thread"}, "model": model, "reasoningEffort": effort})
		case "thread/settings/update":
			model, effort = req.Params.Model, req.Params.Effort
			reply(map[string]any{})
			note("thread/settings/updated", map[string]any{"threadSettings": map[string]any{"model": model, "effort": effort}})
		case "turn/start":
			reply(map[string]any{"turn": map[string]any{"id": "fixture-turn"}})
			note("item/agentMessage/delta", map[string]any{"itemId": "fixture-reply", "delta": model + "/" + effort})
			note("turn/completed", map[string]any{"turn": map[string]any{"id": "fixture-turn", "status": "completed"}})
		default:
			reply(map[string]any{})
		}
	}
}

const protocolPeer = `#!/bin/sh
sid=e2e-session
previous=
for arg in "$@"; do
 case "$previous" in --session-id|--resume) sid="$arg";; esac
 previous="$arg"
done
printf '{"type":"system","subtype":"init","session_id":"%s"}\n' "$sid"
while IFS= read -r line; do
 printf '%s\n' '{"type":"assistant","message":{"id":"reply","model":"fixture","content":[{"type":"text","text":"po"}]}}'
 printf '%s\n' '{"type":"assistant","message":{"id":"reply","model":"fixture","content":[{"type":"text","text":"ng"}]}}'
 printf '{"type":"result","subtype":"success","session_id":"%s"}\n' "$sid"
done
`

func (c *cluster) chatScenario(t *testing.T) {
	previous := c.t
	defer func() { c.t = previous }()
	c.t = t
	workspace := filepath.Join(c.root, "chat-project")
	if err := os.MkdirAll(workspace, 0700); err != nil {
		t.Fatal(err)
	}
	c.call("SleeveService", "SpawnSleeve", map[string]any{"name": "chatfixture", "harness": "HARNESS_CLAUDE", "workspace": workspace})
	t.Cleanup(func() { c.rpc("SleeveService", "KillSleeve", map[string]any{"name": "chatfixture"}, nil) })
	client := protectoratev1connect.NewChatServiceClient(&http.Client{}, c.url)
	watch := func() string {
		ctx, cancel := context.WithTimeout(c.ctx, 5*time.Second)
		defer cancel()
		req := connect.NewRequest(&pb.WatchRequest{Sleeve: "chatfixture"})
		req.Header().Set("Authorization", "Bearer "+c.token)
		stream, err := client.Watch(ctx, req)
		if err != nil {
			t.Fatal(err)
		}
		defer stream.Close()
		text := ""
		for stream.Receive() {
			ev := stream.Msg()
			if chunk := ev.GetText(); chunk != nil {
				text += chunk.Delta
			}
			if ev.GetSettled() != nil {
				return text
			}
		}
		t.Fatalf("watch stopped before settlement: %v", stream.Err())
		return ""
	}
	request := map[string]any{"sleeve": "chatfixture", "text": "ping", "requestId": "chat-e2e-0001"}
	c.call("ChatService", "Send", request)
	if text := watch(); text != "pong" {
		t.Fatalf("live/replay text=%q", text)
	}
	c.stop(c.hub)
	c.boot()
	c.call("ChatService", "Send", request)
	if text := watch(); text != "pong" {
		t.Fatalf("duplicate send changed replay=%q", text)
	}
	c.codexSelectionScenario(t)
}

func (c *cluster) codexSelectionScenario(t *testing.T) {
	exe, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	peer, err := os.ReadFile(exe)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(c.root, ".local", "bin", "codex"), peer, 0700); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(c.home, "auth", "codex"), 0700); err != nil {
		t.Fatal(err)
	}
	status := func(name string) *pb.ChatStatus {
		t.Helper()
		var res pb.GetSleeveResponse
		if err := protojson.Unmarshal([]byte(c.cli("sleeve", "show", name, "--json")), &res); err != nil {
			t.Fatal(err)
		}
		return res.GetSleeve().GetChat()
	}
	check := func(name, requested, model, effort string) {
		t.Helper()
		c.eventually(func() bool { return status(name).State == pb.ChatState_CHAT_STATE_IDLE })
		st := status(name)
		if st.GetSettings().GetRequested().GetEffort() != requested || st.Model != model || st.ReasoningEffort != effort {
			t.Fatalf("%s selection: %v", name, st)
		}
		c.cli("chat", "send", name, "report native selection")
		ctx, cancel := context.WithTimeout(c.ctx, 5*time.Second)
		defer cancel()
		client := protectoratev1connect.NewChatServiceClient(&http.Client{}, c.url)
		req := connect.NewRequest(&pb.WatchRequest{Sleeve: name, AfterSeq: st.LastSeq})
		req.Header().Set("Authorization", "Bearer "+c.token)
		stream, err := client.Watch(ctx, req)
		if err != nil {
			t.Fatal(err)
		}
		defer stream.Close()
		var text string
		for stream.Receive() {
			if chunk := stream.Msg().GetText(); chunk != nil {
				text += chunk.Delta
			}
			if stream.Msg().GetSettled() != nil {
				if text != model+"/"+effort {
					t.Fatalf("native peer used %q, want %s/%s", text, model, effort)
				}
				return
			}
		}
		t.Fatalf("selection turn did not settle: %v", stream.Err())
	}
	for i, tc := range []struct {
		flags     []string
		requested string
		model     string
		effort    string
	}{
		{nil, "high", "gpt-fixture", "high"},
		{[]string{"--effort", "low"}, "low", "gpt-fixture", "low"},
		{[]string{"--model", "gpt-low"}, "high", "gpt-low", "low"},
	} {
		name := fmt.Sprintf("selection-%d", i)
		workspace := filepath.Join(c.root, name)
		if err := os.Mkdir(workspace, 0700); err != nil {
			t.Fatal(err)
		}
		args := []string{"sleeve", "spawn", name, "--harness", "codex", "--workspace", workspace}
		c.cli(append(args, tc.flags...)...)
		check(name, tc.requested, tc.model, tc.effort)
		if i == 2 {
			log, err := os.ReadFile(c.hub.log.Name())
			if err != nil || !strings.Contains(string(log), `does not advertise hub default effort "high"; using native default effort "low"`) {
				t.Fatalf("missing native effort fallback log: %v: %s", err, log)
			}
			c.stop(c.hub)
			c.boot()
			check(name, tc.requested, tc.model, tc.effort)
		}
		c.cli("sleeve", "kill", name)
	}
	c.cli("sleeve", "resleeve", "chatfixture", "--to", "codex")
	check("chatfixture", "high", "gpt-fixture", "high")
	c.cli("sleeve", "kill", "chatfixture")
	c.cli("orchestrator", "spawn", "--harness", "codex")
	check("envoy", "high", "gpt-fixture", "high")
	c.cli("sleeve", "kill", "envoy")
	for _, configured := range []string{"xhigh", ""} {
		c.stop(c.hub)
		c.hub = c.start("hub-effort", c.env, "hub", "serve", "--listen", strings.TrimPrefix(c.url, "http://"), "--sleeve-effort="+configured)
		c.eventually(func() bool { return c.rpc("HubService", "GetStatus", map[string]any{}, nil) == 200 })
		workspace := filepath.Join(c.root, "chat-project")
		c.cli("sleeve", "spawn", "configured-effort", "--harness", "codex", "--workspace", workspace)
		effective := configured
		if effective == "" {
			effective = "low"
		}
		check("configured-effort", configured, "gpt-fixture", effective)
		c.cli("sleeve", "kill", "configured-effort")
	}
}
