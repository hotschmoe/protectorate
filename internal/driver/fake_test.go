package driver

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"strings"
	"syscall"
	"testing"
	"time"

	"github.com/coder/websocket"
)

// The fakes replay the message shapes recorded in spikes/drivers. The stdio
// ones run as this test binary re-executed with DRIVER_FAKE set.
func TestMain(m *testing.M) {
	if mode := os.Getenv("DRIVER_FAKE"); mode != "" {
		runFake(mode)
		os.Exit(0)
	}
	os.Exit(m.Run())
}

type fakeIO struct {
	in  *bufio.Scanner
	out *json.Encoder
}

func (f fakeIO) read() map[string]any {
	if !f.in.Scan() {
		os.Exit(0)
	}
	var m map[string]any
	_ = json.Unmarshal(f.in.Bytes(), &m)
	return m
}

func (f fakeIO) send(v any) { _ = f.out.Encode(v) }

func argAfter(flag string) string {
	for i, a := range os.Args {
		if a == flag && i+1 < len(os.Args) {
			return os.Args[i+1]
		}
	}
	return ""
}

func runFake(mode string) {
	sc := bufio.NewScanner(os.Stdin)
	sc.Buffer(make([]byte, 1<<20), 1<<20)
	f := fakeIO{in: sc, out: json.NewEncoder(os.Stdout)}
	switch mode {
	case "claude":
		fakeClaude(f)
	case "pi":
		fakePi(f)
	case "grok":
		fakeGrok(f)
	case "hang":
		select {}
	case "stubborn":
		signal.Ignore(syscall.SIGTERM)
		select {}
	}
}

func fakeClaude(f fakeIO) {
	sid := argAfter("--resume")
	if sid == "" {
		sid = argAfter("--session-id")
	}
	if os.Getenv("DRIVER_FAKE_STDERR") != "" {
		fmt.Fprint(os.Stderr, strings.Repeat("e", 100000))
	}
	text := func(s string) {
		f.send(map[string]any{"type": "assistant", "message": map[string]any{"content": []any{map[string]any{"type": "text", "text": s}}}})
	}
	result := func(stop string) {
		f.send(map[string]any{"type": "result", "subtype": "success", "stop_reason": stop, "session_id": sid, "total_cost_usd": 0.01,
			"usage": map[string]any{"input_tokens": 2, "output_tokens": 6, "cache_read_input_tokens": 44}})
	}
	for {
		m := f.read()
		if m["type"] != "user" {
			continue
		}
		prompt, _ := m["message"].(map[string]any)["content"].(string)
		f.send(map[string]any{"type": "system", "subtype": "init", "session_id": sid})
		switch {
		case strings.Contains(prompt, "die"):
			os.Exit(3)
		case strings.Contains(prompt, "slow"):
			for f.read()["type"] != "control_request" {
			}
			result("cancelled")
		case strings.Contains(prompt, "approve"):
			f.send(map[string]any{"type": "assistant", "message": map[string]any{"content": []any{
				map[string]any{"type": "tool_use", "id": "toolu_1", "name": "Write", "input": map[string]any{"file_path": "/x/hello.txt", "content": "hi\n"}}}}})
			f.send(map[string]any{"type": "control_request", "request_id": "req1", "request": map[string]any{
				"subtype": "can_use_tool", "tool_name": "Write", "input": map[string]any{"file_path": "/x/hello.txt", "content": "hi\n"}}})
			var resp map[string]any
			for {
				if resp = f.read(); resp["type"] == "control_response" {
					break
				}
			}
			r := resp["response"].(map[string]any)["response"].(map[string]any)
			if r["behavior"] == "allow" {
				_, hasInput := r["updatedInput"]
				text(fmt.Sprintf("allowed input=%v", hasInput))
			} else {
				text(fmt.Sprintf("denied:%v", r["message"]))
			}
			result("end_turn")
		default:
			text("ok id=" + sid)
			result("end_turn")
		}
	}
}

func fakePi(f fakeIO) {
	sid := argAfter("--session")
	if sid == "" {
		sid = "pi-new-1"
	}
	delta := func(s string) {
		f.send(map[string]any{"type": "message_update", "assistantMessageEvent": map[string]any{"type": "text_delta", "delta": s}})
	}
	for {
		m := f.read()
		id := m["id"]
		switch m["type"] {
		case "get_state":
			f.send(map[string]any{"id": id, "type": "response", "command": "get_state", "success": true, "data": map[string]any{"sessionId": sid}})
		case "prompt":
			prompt, _ := m["message"].(string)
			f.send(map[string]any{"id": id, "type": "response", "command": "prompt", "success": true, "data": map[string]any{"disposition": "started"}})
			f.send(map[string]any{"type": "agent_start"})
			switch {
			case strings.Contains(prompt, "die"):
				os.Exit(3)
			case strings.Contains(prompt, "slow"):
				for {
					n := f.read()
					if n["type"] == "steer" {
						delta("steered")
						f.send(map[string]any{"type": "agent_settled", "aborted": false})
						break
					}
					if n["type"] == "abort" {
						f.send(map[string]any{"type": "agent_settled", "aborted": true})
						break
					}
				}
			default:
				f.send(map[string]any{"type": "tool_execution_start", "toolCallId": "c1", "toolName": "write", "args": map[string]any{"path": "a"}})
				delta("ok id=" + sid)
				f.send(map[string]any{"type": "message_end", "message": map[string]any{"role": "assistant", "usage": map[string]any{"input": 3, "output": 4, "totalTokens": 7}}})
				f.send(map[string]any{"type": "agent_end"})
				f.send(map[string]any{"type": "agent_settled", "aborted": false})
			}
		}
	}
}

func fakeGrok(f fakeIO) {
	sid := "grok-new-1"
	approvalsOff := false
	chunk := func(s string) {
		f.send(map[string]any{"jsonrpc": "2.0", "method": "session/update", "params": map[string]any{
			"sessionId": sid, "update": map[string]any{"sessionUpdate": "agent_message_chunk", "content": map[string]any{"type": "text", "text": s}}}})
	}
	for {
		m := f.read()
		id := m["id"]
		params, _ := m["params"].(map[string]any)
		reply := func(res any) { f.send(map[string]any{"jsonrpc": "2.0", "id": id, "result": res}) }
		switch m["method"] {
		case "initialize":
			reply(map[string]any{"protocolVersion": 1, "agentCapabilities": map[string]any{"loadSession": true}})
		case "session/new":
			reply(map[string]any{"sessionId": sid})
		case "session/load":
			sid, _ = params["sessionId"].(string)
			chunk("REPLAY")
			reply(map[string]any{})
		case "session/prompt":
			text := params["prompt"].([]any)[0].(map[string]any)["text"].(string)
			switch {
			case text == "/always-approve off":
				approvalsOff = true
				reply(map[string]any{"stopReason": "end_turn"})
			case strings.Contains(text, "die"):
				os.Exit(3)
			case strings.Contains(text, "slow"):
				for f.read()["method"] != "session/cancel" {
				}
				reply(map[string]any{"stopReason": "cancelled"})
			case strings.Contains(text, "approve"):
				f.send(map[string]any{"jsonrpc": "2.0", "id": 100, "method": "session/request_permission", "params": map[string]any{
					"sessionId": sid,
					"toolCall":  map[string]any{"toolCallId": "call-1", "kind": "edit", "title": "Write `/x/hello.txt`"},
					"options": []any{
						map[string]any{"optionId": "allow-edits-session", "kind": "allow_always"},
						map[string]any{"optionId": "allow-once", "kind": "allow_once"},
						map[string]any{"optionId": "reject-once", "kind": "reject_once"}}}})
				var resp map[string]any
				for {
					if resp = f.read(); resp["id"] == float64(100) {
						break
					}
				}
				out := resp["result"].(map[string]any)["outcome"].(map[string]any)
				if out["optionId"] == "allow-once" {
					chunk(fmt.Sprintf("allowed gate=%v", approvalsOff))
				} else {
					chunk("denied " + fmt.Sprint(out["optionId"]))
				}
				reply(map[string]any{"stopReason": "end_turn"})
			default:
				chunk("ok id=" + sid)
				reply(map[string]any{"stopReason": "end_turn", "_meta": map[string]any{"usage": map[string]any{"inputTokens": 9, "outputTokens": 2}}})
			}
		}
	}
}

// fakeCodex is an in-process daemon: a WebSocket server on a unix socket.
func fakeCodex(t *testing.T) string {
	t.Helper()
	dir, err := os.MkdirTemp("", "cxs")
	if err != nil {
		t.Fatal(err)
	}
	sock := filepath.Join(dir, "d.sock")
	ln, err := net.Listen("unix", sock)
	if err != nil {
		t.Fatal(err)
	}
	srv := &http.Server{Handler: http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		c, err := websocket.Accept(w, r, nil)
		if err != nil {
			return
		}
		serveFakeCodex(r.Context(), c)
	})}
	go srv.Serve(ln)
	t.Cleanup(func() { srv.Close() })
	return sock
}

func serveFakeCodex(ctx context.Context, c *websocket.Conn) {
	defer c.CloseNow()
	send := func(v any) {
		b, _ := json.Marshal(v)
		_ = c.Write(ctx, websocket.MessageText, b)
	}
	read := func() map[string]any {
		_, b, err := c.Read(ctx)
		if err != nil {
			return nil
		}
		var m map[string]any
		_ = json.Unmarshal(b, &m)
		return m
	}
	tid := "cx-new-1"
	note := func(method string, p map[string]any) {
		p["threadId"] = tid
		send(map[string]any{"method": method, "params": p})
	}
	complete := func(status string) {
		note("turn/completed", map[string]any{"turn": map[string]any{"id": "t1", "status": status}})
	}
	for {
		m := read()
		if m == nil {
			return
		}
		id := m["id"]
		params, _ := m["params"].(map[string]any)
		reply := func(res any) { send(map[string]any{"id": id, "result": res}) }
		switch m["method"] {
		case "initialize", "thread/unsubscribe":
			reply(map[string]any{})
		case "thread/start":
			reply(map[string]any{"thread": map[string]any{"id": tid}})
		case "thread/resume":
			tid, _ = params["threadId"].(string)
			if tid == "locked" {
				send(map[string]any{"id": id, "error": map[string]any{"code": -32600, "message": "thread locked already has an active writer"}})
				continue
			}
			reply(map[string]any{"thread": map[string]any{"id": tid}})
		case "turn/start":
			text := params["input"].([]any)[0].(map[string]any)["text"].(string)
			reply(map[string]any{"turn": map[string]any{"id": "t1"}})
			note("turn/started", map[string]any{"turn": map[string]any{"id": "t1"}})
			switch {
			case strings.Contains(text, "die"):
				c.CloseNow()
				return
			case strings.Contains(text, "slow"):
				for {
					n := read()
					if n == nil {
						return
					}
					if n["method"] == "turn/steer" {
						send(map[string]any{"id": n["id"], "result": map[string]any{"turnId": "t1"}})
						note("item/agentMessage/delta", map[string]any{"delta": "steered"})
						complete("completed")
						break
					}
					if n["method"] == "turn/interrupt" {
						send(map[string]any{"id": n["id"], "result": map[string]any{}})
						complete("interrupted")
						break
					}
				}
			case strings.Contains(text, "approve"):
				send(map[string]any{"id": 0, "method": "item/commandExecution/requestApproval", "params": map[string]any{
					"threadId": tid, "turnId": "t1", "itemId": "exec-1", "command": "touch hello.txt"}})
				var resp map[string]any
				for {
					if resp = read(); resp == nil {
						return
					}
					if resp["id"] == float64(0) {
						break
					}
				}
				note("item/agentMessage/delta", map[string]any{"delta": "decision=" + map[string]string{"accept": "allowed", "decline": "denied"}[fmt.Sprint(resp["result"].(map[string]any)["decision"])]})
				complete("completed")
			default:
				note("item/started", map[string]any{"item": map[string]any{"type": "commandExecution", "id": "exec-0", "command": "ls"}})
				note("item/agentMessage/delta", map[string]any{"delta": "ok id=" + tid})
				note("thread/tokenUsage/updated", map[string]any{"tokenUsage": map[string]any{"last": map[string]any{"inputTokens": 5, "outputTokens": 1, "cachedInputTokens": 2}}})
				complete("completed")
			}
		}
	}
}

type fixture struct {
	h    Harness
	open func(t *testing.T, id string) Session
}

func fixtures() []fixture {
	stdio := func(h Harness) fixture {
		return fixture{h, func(t *testing.T, id string) Session {
			t.Helper()
			s, err := Open(ctx5(t), h, t.TempDir(), id, Options{Bin: os.Args[0], Env: fakeEnv(string(h)), StopTimeout: time.Second})
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { s.Close() })
			return s
		}}
	}
	return []fixture{
		stdio(Claude),
		{Codex, func(t *testing.T, id string) Session {
			t.Helper()
			s, err := Open(ctx5(t), Codex, t.TempDir(), id, Options{CodexSocket: fakeCodex(t), StopTimeout: time.Second})
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { s.Close() })
			return s
		}},
		stdio(Grok),
		stdio(Pi),
	}
}

func ctx5(t *testing.T) context.Context {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	t.Cleanup(cancel)
	return ctx
}

func next(t *testing.T, s Session) Event {
	t.Helper()
	select {
	case ev, ok := <-s.Events():
		if !ok {
			t.Fatal("event stream closed")
		}
		return ev
	case <-time.After(10 * time.Second):
		t.Fatal("timed out waiting for event")
	}
	return Event{}
}

// until collects events up to and including the first of kind.
func until(t *testing.T, s Session, kind Kind) []Event {
	t.Helper()
	var evs []Event
	for {
		ev := next(t, s)
		evs = append(evs, ev)
		if ev.Kind == kind {
			return evs
		}
	}
}

func joinText(evs []Event) string {
	var b strings.Builder
	for _, e := range evs {
		if e.Kind == KindText {
			b.WriteString(e.Text)
		}
	}
	return b.String()
}

func kinds(evs []Event) []Kind {
	var k []Kind
	for _, e := range evs {
		k = append(k, e.Kind)
	}
	return k
}

// fakeEnv selects the fake and stops the race runtime from sleeping 1s at
// every child exit.
func fakeEnv(mode string, extra ...string) []string {
	return append([]string{"DRIVER_FAKE=" + mode, "GORACE=atexit_sleep_ms=0"}, extra...)
}
