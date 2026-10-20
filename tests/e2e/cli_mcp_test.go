package e2e

import (
	"bufio"
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"regexp"
	"slices"
	"strings"
	"testing"
	"time"
)

func TestBinaryCLIAndMCP(t *testing.T) {
	c := newCluster(t)
	t.Run("help snapshots", func(t *testing.T) {
		raw, err := os.ReadFile("testdata/cli_help.json")
		if err != nil {
			t.Fatal(err)
		}
		var snapshot map[string]string
		if err := json.Unmarshal(raw, &snapshot); err != nil {
			t.Fatal(err)
		}
		holder := regexp.MustCompile(`who holds the lease \(default "[^"]*"\)`)
		listing := regexp.MustCompile(`^  ([a-z][a-z0-9-]*)\s+`)
		visited := map[string]bool{}
		var walk func([]string)
		walk = func(path []string) {
			key := strings.Join(path, " ")
			visited[key] = true
			cmd := exec.CommandContext(c.ctx, c.binary, append(slices.Clone(path), "--help")...)
			cmd.Env = c.env
			out, err := cmd.CombinedOutput()
			if err != nil {
				t.Fatalf("%s --help: %v\n%s", key, err, out)
			}
			got := holder.ReplaceAllString(string(out), `who holds the lease (default "<holder>")`)
			if want, ok := snapshot[key]; !ok || got != want {
				t.Errorf("help %q differs from baseline snapshot:\n%s", key, got)
			}
			cmd = exec.CommandContext(c.ctx, c.binary, append([]string{"help"}, path...)...)
			cmd.Env = c.env
			viaHelp, err := cmd.CombinedOutput()
			if err != nil {
				t.Fatalf("help %s: %v\n%s", key, err, viaHelp)
			}
			if normalized := holder.ReplaceAllString(string(viaHelp), `who holds the lease (default "<holder>")`); normalized != got {
				t.Errorf("help %s and %s --help disagree", key, key)
			}
			inCommands := false
			for _, line := range strings.Split(string(out), "\n") {
				if line == "Available Commands:" || line == "Additional Commands:" {
					inCommands = true
					continue
				}
				if inCommands && line == "" {
					inCommands = false
				}
				if inCommands {
					if match := listing.FindStringSubmatch(line); match != nil {
						walk(append(slices.Clone(path), match[1]))
					}
				}
			}
		}
		walk(nil)
		for key := range snapshot {
			if !visited[key] {
				t.Errorf("command %q disappeared", key)
			}
		}
	})
	t.Run("CLI parsing across hub and node", func(t *testing.T) {
		c.t = t
		defer func() { c.t = c.owner }()
		c.cli("node", "add", "cli-tags", "--addr", "127.0.0.1:1", "--tag", "one,two", "--tag", `"three",four`)
		var node struct {
			Node struct {
				Tags []string `json:"tags"`
			} `json:"node"`
		}
		if c.rpc("NodeService", "GetNode", map[string]any{"name": "cli-tags"}, &node) != 200 {
			t.Fatal("read node")
		}
		if !slices.Equal(node.Node.Tags, []string{"four", "one", "three", "two"}) {
			t.Fatalf("StringSlice tags: %+v", node)
		}
		c.cli("node", "rm", "cli-tags")
		for _, args := range [][]string{{"--json", "status"}, {"status", "--json"}, {"--hub=" + c.url, "--json", "status"}} {
			var status map[string]any
			if err := json.Unmarshal([]byte(c.cli(args...)), &status); err != nil || status["hostname"] == nil {
				t.Fatalf("global flags %v: %v", args, err)
			}
		}
		for _, alias := range []string{"msg", "message"} {
			if out := c.cli(alias, "inbox", "--json"); !json.Valid([]byte(out)) {
				t.Fatalf("alias %s: %s", alias, out)
			}
		}
		for _, args := range [][]string{{"unknown-command"}, {"node", "unknown-command"}, {"status", "--unknown"}, {"job", "test-cross"}, {"lease", "acquire", "--ttl", "invalid"}} {
			cmd := exec.CommandContext(c.ctx, c.binary, args...)
			cmd.Env = c.env
			out, err := cmd.CombinedOutput()
			var exit *exec.ExitError
			if !errors.As(err, &exit) || exit.ExitCode() != 1 || !bytes.Contains(out, []byte("Usage:")) || !bytes.Contains(out, []byte("error:")) {
				t.Fatalf("invalid invocation %v: %v\n%s", args, err, out)
			}
		}
		cmd := exec.CommandContext(c.ctx, c.binary, "job", "run", "--hub", c.url, "--on", "peer", "--env", "CSV=a,b", "-e", "LITERAL=--json", "--timeout", "10s", "--", "sh", "-c", `printf '%s|%s|%s' "$CSV" "$LITERAL" "$1"; exit 7`, "sh", "--hub=passthrough")
		cmd.Env = c.env
		out, err := cmd.CombinedOutput()
		var exit *exec.ExitError
		if !errors.As(err, &exit) || exit.ExitCode() != 7 || !bytes.Contains(out, []byte("a,b|--json|--hub=passthrough")) {
			t.Fatalf("repeatable env, shorthand, duration, passthrough and exit status: %v\n%s", err, out)
		}
	})
	for _, modern := range []bool{false, true} {
		t.Run(fmt.Sprintf("stdio modern=%t", modern), func(t *testing.T) {
			peer := startCLIMCP(t, c, "--allow", "*")
			meta := map[string]any{}
			if modern {
				meta = map[string]any{"io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientInfo": map[string]any{"name": "e2e", "version": "1"}, "io.modelcontextprotocol/clientCapabilities": map[string]any{}}
				peer.send(1, "server/discover", map[string]any{"_meta": meta})
				res := peer.read()
				if res.Error != nil || !bytes.Contains(res.Result, []byte("2026-07-28")) {
					t.Fatalf("discover: %+v", res)
				}
			} else {
				peer.send(1, "initialize", map[string]any{"protocolVersion": "2025-11-25", "clientInfo": map[string]any{"name": "e2e", "version": "1"}, "capabilities": map[string]any{}})
				res := peer.read()
				if res.Error != nil || !bytes.Contains(res.Result, []byte(`"protocolVersion":"2025-11-25"`)) {
					t.Fatalf("initialize: %+v", res)
				}
				peer.notify("notifications/initialized", map[string]any{})
			}
			peer.send(2, "tools/list", map[string]any{"_meta": meta})
			listed := peer.read()
			var tools struct {
				Tools json.RawMessage `json:"tools"`
			}
			if err := json.Unmarshal(listed.Result, &tools); err != nil {
				t.Fatal(err)
			}
			var pretty bytes.Buffer
			if err := json.Indent(&pretty, tools.Tools, "", "  "); err != nil {
				t.Fatal(err)
			}
			pretty.WriteByte('\n')
			const goldenHash = "0eddcb19f121844d8e9bbc469226fc75e397ea4dadfdb1bd9d4409bea2659310"
			if got := fmt.Sprintf("%x", sha256.Sum256(pretty.Bytes())); got != goldenHash {
				t.Fatalf("tools/list differs byte-for-byte from expected catalog: %s", got)
			}
			peer.send(3, "tools/call", map[string]any{"_meta": meta, "name": "hub_get_status", "arguments": map[string]any{}})
			assertCLIMCPStatus(t, peer.read())
			peer.send(4, "tools/call", map[string]any{"_meta": meta, "name": "hub_get_status", "arguments": map[string]any{"unknown": true}})
			if res := peer.read(); res.Error != nil || !bytes.Contains(res.Result, []byte("invalid_argument:")) || !bytes.Contains(res.Result, []byte(`"isError":true`)) {
				t.Fatalf("strict arguments: %+v", res)
			}
			peer.send(5, "tools/call", map[string]any{"_meta": meta, "name": "token_create_token", "arguments": map[string]any{}})
			if res := peer.read(); res.Error == nil || res.Error.Code != -32602 {
				t.Fatalf("owner-only tool exposed: %+v", res)
			}
			if modern {
				peer.send(6, "subscriptions/listen", map[string]any{"_meta": meta, "notifications": map[string]any{"toolsListChanged": true}})
				if res := peer.read(); res.Method != "notifications/subscriptions/acknowledged" {
					t.Fatalf("subscription ack: %+v", res)
				}
				peer.send(7, "tools/call", map[string]any{"_meta": meta, "name": "hub_get_status", "arguments": map[string]any{}})
				assertCLIMCPStatus(t, peer.read())
				peer.notify("notifications/cancelled", map[string]any{"requestId": 6})
				if res := peer.read(); string(res.ID) != "6" {
					t.Fatalf("subscription cancellation: %+v", res)
				}
			} else {
				peer.send(6, "ping", map[string]any{})
				if res := peer.read(); res.Error != nil {
					t.Fatalf("ping: %+v", res)
				}
			}
		})
	}
	t.Run("stdio filters", func(t *testing.T) {
		peer := startCLIMCP(t, c, "--allow", "hub_*,backlog_*", "--deny", "backlog_*")
		peer.send(1, "initialize", map[string]any{"protocolVersion": "2024-11-05", "clientInfo": map[string]any{"name": "e2e", "version": "1"}, "capabilities": map[string]any{}})
		peer.read()
		peer.send(2, "tools/list", map[string]any{})
		res := peer.read()
		var tools struct {
			Tools []struct {
				Name string `json:"name"`
			} `json:"tools"`
		}
		if err := json.Unmarshal(res.Result, &tools); err != nil || len(tools.Tools) != 1 || tools.Tools[0].Name != "hub_get_status" {
			t.Fatalf("allow commas and deny: %+v %v", res, err)
		}
	})
	t.Run("cancel pending hub call", func(t *testing.T) {
		var sent struct {
			Message struct {
				ID string `json:"id"`
			} `json:"message"`
		}
		if c.rpc("MessageService", "Send", map[string]any{"to": "owner", "body": "MCP cancellation scenario", "kind": "MESSAGE_KIND_REQUEST", "ackRequired": true}, &sent) != 200 || sent.Message.ID == "" {
			t.Fatal("create pending mail")
		}
		peer := startCLIMCP(t, c)
		peer.send(1, "initialize", map[string]any{"protocolVersion": "2025-11-25", "clientInfo": map[string]any{"name": "e2e", "version": "1"}, "capabilities": map[string]any{}})
		peer.read()
		peer.send(2, "tools/call", map[string]any{"name": "message_wait", "arguments": map[string]any{"target": sent.Message.ID, "timeout": "60s"}})
		peer.send(3, "ping", map[string]any{})
		if res := peer.read(); string(res.ID) != "3" || res.Error != nil {
			t.Fatalf("pending call blocked reader: %+v", res)
		}
		peer.notify("notifications/cancelled", map[string]any{"requestId": 2})
		if res := peer.read(); string(res.ID) != "2" || !bytes.Contains(res.Result, []byte("canceled:")) {
			t.Fatalf("hub wait was not cancelled: %+v", res)
		}
		peer.send(4, "tools/call", map[string]any{"name": "hub_get_status", "arguments": map[string]any{}})
		assertCLIMCPStatus(t, peer.read())
	})
	t.Run("authenticated streamable HTTP", func(t *testing.T) { testCLIMCPHTTP(t, c) })
}

type cliMCPReply struct {
	ID     json.RawMessage `json:"id"`
	Method string          `json:"method"`
	Result json.RawMessage `json:"result"`
	Error  *struct {
		Code    int    `json:"code"`
		Message string `json:"message"`
	} `json:"error"`
}
type cliMCPPeer struct {
	t      *testing.T
	input  io.WriteCloser
	output *bufio.Scanner
}

func startCLIMCP(t *testing.T, c *cluster, args ...string) *cliMCPPeer {
	t.Helper()
	ctx, cancel := context.WithTimeout(c.ctx, 20*time.Second)
	cmd := exec.CommandContext(ctx, c.binary, append([]string{"mcp", "--hub", c.url}, args...)...)
	cmd.Env = c.env
	in, err := cmd.StdinPipe()
	if err != nil {
		cancel()
		t.Fatal(err)
	}
	out, err := cmd.StdoutPipe()
	if err != nil {
		cancel()
		t.Fatal(err)
	}
	log, err := os.CreateTemp(c.root, "mcp-*.log")
	if err != nil {
		cancel()
		t.Fatal(err)
	}
	cmd.Stderr = log
	if err := cmd.Start(); err != nil {
		cancel()
		log.Close()
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- cmd.Wait() }()
	t.Cleanup(func() {
		in.Close()
		select {
		case err := <-done:
			if err != nil {
				t.Errorf("MCP process: %v", err)
			}
		case <-time.After(2 * time.Second):
			cancel()
			<-done
			t.Error("MCP failed to exit on stdin EOF")
		}
		cancel()
		log.Close()
		if t.Failed() {
			b, _ := os.ReadFile(log.Name())
			t.Logf("MCP stderr: %s", b)
		}
	})
	scanner := bufio.NewScanner(out)
	scanner.Buffer(make([]byte, 4096), 16<<20)
	return &cliMCPPeer{t: t, input: in, output: scanner}
}
func (p *cliMCPPeer) send(id int, method string, params any) {
	p.t.Helper()
	if err := json.NewEncoder(p.input).Encode(map[string]any{"jsonrpc": "2.0", "id": id, "method": method, "params": params}); err != nil {
		p.t.Fatal(err)
	}
}
func (p *cliMCPPeer) notify(method string, params any) {
	p.t.Helper()
	if err := json.NewEncoder(p.input).Encode(map[string]any{"jsonrpc": "2.0", "method": method, "params": params}); err != nil {
		p.t.Fatal(err)
	}
}
func (p *cliMCPPeer) read() cliMCPReply {
	p.t.Helper()
	if !p.output.Scan() {
		p.t.Fatalf("MCP ended: %v", p.output.Err())
	}
	var reply cliMCPReply
	if err := json.Unmarshal(p.output.Bytes(), &reply); err != nil {
		p.t.Fatalf("MCP output %q: %v", p.output.Text(), err)
	}
	return reply
}
func assertCLIMCPStatus(t *testing.T, res cliMCPReply) {
	t.Helper()
	var result struct {
		IsError bool                          `json:"isError"`
		Content []struct{ Type, Text string } `json:"content"`
	}
	if err := json.Unmarshal(res.Result, &result); err != nil || res.Error != nil || result.IsError || len(result.Content) != 1 || result.Content[0].Type != "text" {
		t.Fatalf("status tool response: %+v %v", res, err)
	}
	var status struct {
		Hostname string `json:"hostname"`
	}
	if err := json.Unmarshal([]byte(result.Content[0].Text), &status); err != nil || status.Hostname == "" {
		t.Fatalf("status tool did not reach real hub: %+v %v", result, err)
	}
}
func testCLIMCPHTTP(t *testing.T, c *cluster) {
	addr := loopback(t)
	c.start("mcp-http", c.env, "mcp", "--hub", c.url, "--http", addr)
	url := "http://" + addr
	post := func(token, session string, id int, method string, params any) (int, string, cliMCPReply) {
		t.Helper()
		body, err := json.Marshal(map[string]any{"jsonrpc": "2.0", "id": id, "method": method, "params": params})
		if err != nil {
			t.Fatal(err)
		}
		req, err := http.NewRequestWithContext(c.ctx, http.MethodPost, url, bytes.NewReader(body))
		if err != nil {
			t.Fatal(err)
		}
		req.Header.Set("Content-Type", "application/json")
		req.Header.Set("Accept", "application/json, text/event-stream")
		req.Header.Set("Authorization", "Bearer "+token)
		if session != "" {
			req.Header.Set("Mcp-Session-Id", session)
		}
		res, err := c.client.Do(req)
		if err != nil {
			return 0, "", cliMCPReply{}
		}
		defer res.Body.Close()
		var reply cliMCPReply
		if res.StatusCode == 200 {
			if err := json.NewDecoder(res.Body).Decode(&reply); err != nil {
				t.Fatal(err)
			}
		}
		return res.StatusCode, res.Header.Get("Mcp-Session-Id"), reply
	}
	c.eventually(func() bool { status, _, _ := post("wrong", "", 1, "ping", map[string]any{}); return status == 401 })
	status, id, res := post(c.token, "", 1, "initialize", map[string]any{"protocolVersion": "2025-11-25", "clientInfo": map[string]any{"name": "e2e", "version": "1"}, "capabilities": map[string]any{}})
	if status != 200 || id == "" || res.Error != nil {
		t.Fatalf("HTTP initialize: %d %q %+v", status, id, res)
	}
	status, _, res = post(c.token, id, 2, "tools/list", map[string]any{})
	if status != 200 || res.Error != nil || !bytes.Contains(res.Result, []byte("hub_get_status")) || bytes.Contains(res.Result, []byte("backlog_create_note")) {
		t.Fatalf("default read-only tools: %d %+v", status, res)
	}
	status, _, res = post(c.token, id, 3, "tools/call", map[string]any{"name": "hub_get_status", "arguments": map[string]any{}})
	if status != 200 {
		t.Fatalf("HTTP tool call: %d", status)
	}
	assertCLIMCPStatus(t, res)
	req, err := http.NewRequestWithContext(c.ctx, http.MethodDelete, url, nil)
	if err != nil {
		t.Fatal(err)
	}
	req.Header.Set("Authorization", "Bearer "+c.token)
	req.Header.Set("Mcp-Session-Id", id)
	deleted, err := c.client.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	deleted.Body.Close()
	if deleted.StatusCode != 204 {
		t.Fatalf("delete session: %d", deleted.StatusCode)
	}
	if status, _, _ := post(c.token, id, 4, "ping", map[string]any{}); status != 404 {
		t.Fatalf("deleted session still usable: %d", status)
	}
}
