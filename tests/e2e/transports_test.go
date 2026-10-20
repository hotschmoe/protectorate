package e2e

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os/exec"
	"strings"
	"testing"
	"time"

	"github.com/coder/websocket"
)

func (c *cluster) transportScenario(t *testing.T) {
	ctx, cancel := context.WithTimeout(c.ctx, 10*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, c.binary, "--hub", c.url, "mcp", "--allow", "*")
	cmd.Env = c.env
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	input, err := cmd.StdinPipe()
	if err != nil {
		t.Fatal(err)
	}
	output, err := cmd.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	defer func() {
		input.Close()
		_ = cmd.Process.Kill()
		_ = cmd.Wait()
		c.checkRace("CLI mcp stderr", stderr.Bytes())
	}()
	enc := json.NewEncoder(input)
	scan := bufio.NewScanner(output)
	scan.Buffer(nil, 4<<20)
	request := func(id int, method string, params any) map[string]any {
		t.Helper()
		if err := enc.Encode(map[string]any{"jsonrpc": "2.0", "id": id, "method": method, "params": params}); err != nil {
			t.Fatal(err)
		}
		for scan.Scan() {
			var res map[string]any
			if err := json.Unmarshal(scan.Bytes(), &res); err != nil {
				t.Fatal(err)
			}
			if res["id"] == float64(id) {
				if res["error"] != nil {
					t.Fatalf("MCP %s: %v", method, res)
				}
				return res["result"].(map[string]any)
			}
		}
		t.Fatalf("MCP %s ended: %v", method, scan.Err())
		return nil
	}
	request(1, "initialize", map[string]any{"protocolVersion": "2025-03-26", "capabilities": map[string]any{}, "clientInfo": map[string]any{"name": "e2e", "version": "1"}})
	if err := enc.Encode(map[string]any{"jsonrpc": "2.0", "method": "notifications/initialized"}); err != nil {
		t.Fatal(err)
	}
	tools := request(2, "tools/list", map[string]any{})
	text := fmt.Sprint(tools)
	if !strings.Contains(text, "hub_get_status") || strings.Contains(text, "token_create_token") || strings.Contains(text, "node_add_node") {
		t.Fatalf("MCP tool authority: %v", tools)
	}
	result := request(3, "tools/call", map[string]any{"name": "hub_get_status", "arguments": map[string]any{}})
	if result["isError"] == true || !strings.Contains(fmt.Sprint(result), "version") {
		t.Fatalf("MCP hub call: %v", result)
	}
	if _, err := exec.LookPath("tmux"); err != nil {
		t.Skip("terminal scenario requires tmux")
	}
	c.call("SessionService", "CreateSession", map[string]any{"name": "wire", "command": []string{"sh", "-c", "printf terminal-replay; exec sleep 60"}})
	defer c.call("SessionService", "KillSession", map[string]any{"name": "wire"})
	url := "ws" + strings.TrimPrefix(c.url, "http") + "/ws/terminal/wire"
	for _, token := range []string{"invalid", c.token} {
		conn, res, err := websocket.Dial(ctx, url, &websocket.DialOptions{HTTPHeader: http.Header{"Authorization": []string{"Bearer " + token}}})
		if token == "invalid" {
			if err == nil {
				conn.CloseNow()
				t.Fatal("unauthorized terminal accepted")
			}
			if res == nil || res.StatusCode != 401 {
				t.Fatalf("terminal refusal: %v %v", res, err)
			}
			continue
		}
		if err != nil {
			t.Fatal(err)
		}
		var received string
		for !strings.Contains(received, "terminal-replay") {
			_, b, err := conn.Read(ctx)
			if err != nil {
				conn.CloseNow()
				t.Fatal(err)
			}
			received += string(b)
		}
		conn.Close(websocket.StatusNormalClosure, "")
	}
}
