package driver

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"sync"
	"time"

	"github.com/coder/websocket"
)

// codexSession talks to the user's Codex app-server daemon over its control
// socket (WebSocket on a unix socket). A private `codex app-server` cannot
// resume a thread the daemon already holds, and the daemon is never started,
// stopped or restarted from here.
type codexSession struct {
	*core
	conn *websocket.Conn
	rpc  *rpcPeer
	wmu  sync.Mutex
	ctx  context.Context
	stop context.CancelFunc

	tmu    sync.Mutex
	turnID string
}

type codexPending struct {
	rpcID  json.RawMessage
	method string
	params json.RawMessage
}

func codexSocketPath(opts Options) (string, error) {
	p := opts.CodexSocket
	if p == "" {
		home := os.Getenv("CODEX_HOME")
		if home == "" {
			h, err := os.UserHomeDir()
			if err != nil {
				return "", err
			}
			home = filepath.Join(h, ".codex")
		}
		p = filepath.Join(home, "app-server-control", "app-server-control.sock")
	}
	if r, err := filepath.EvalSymlinks(p); err == nil {
		p = r
	}
	return p, nil
}

func openCodex(ctx context.Context, cwd, sessionID string, opts Options) (Session, error) {
	sock, err := codexSocketPath(opts)
	if err != nil {
		return nil, fmt.Errorf("open codex: %w", err)
	}
	hc := &http.Client{Transport: &http.Transport{
		DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
			var d net.Dialer
			return d.DialContext(ctx, "unix", sock)
		},
	}}
	conn, _, err := websocket.Dial(ctx, "ws://localhost/", &websocket.DialOptions{HTTPClient: hc})
	if err != nil {
		return nil, fmt.Errorf("open codex: connect daemon socket %s: %w", sock, err)
	}
	conn.SetReadLimit(maxLine)
	s := &codexSession{core: newCore(Codex, opts), conn: conn}
	s.ctx, s.stop = context.WithCancel(context.Background())
	s.rpc = newRPCPeer(s.write)
	s.rpc.onNote = s.onNote
	s.rpc.onReq = s.onRequest
	go s.readLoop()

	fail := func(err error) (Session, error) {
		s.markShut()
		s.stop()
		_ = conn.CloseNow()
		<-s.done
		return nil, fmt.Errorf("open codex: %w", err)
	}
	if _, err := s.rpc.call(ctx, "initialize", map[string]any{
		"clientInfo":   map[string]any{"name": "protectorate", "version": "0"},
		"capabilities": map[string]any{"experimentalApi": true},
	}); err != nil {
		return fail(err)
	}
	if err := s.rpc.notify("initialized", nil); err != nil {
		return fail(err)
	}
	params := map[string]any{"cwd": cwd, "approvalPolicy": "untrusted", "sandbox": "workspace-write"}
	method := "thread/start"
	if sessionID != "" {
		method = "thread/resume"
		params["threadId"] = sessionID
		params["excludeTurns"] = true
	}
	res, err := s.rpc.call(ctx, method, params)
	if err != nil {
		return fail(err)
	}
	var r struct {
		Thread struct {
			ID string `json:"id"`
		} `json:"thread"`
	}
	if json.Unmarshal(res, &r) != nil || r.Thread.ID == "" {
		return fail(fmt.Errorf("%s returned no thread id", method))
	}
	s.setID(r.Thread.ID)
	return s, nil
}

func (s *codexSession) write(v any) error {
	b, err := json.Marshal(v)
	if err != nil {
		return err
	}
	ctx, cancel := context.WithTimeout(s.ctx, 10*time.Second)
	defer cancel()
	s.wmu.Lock()
	defer s.wmu.Unlock()
	return s.conn.Write(ctx, websocket.MessageText, b)
}

func (s *codexSession) readLoop() {
	var rerr error
	for {
		_, data, err := s.conn.Read(s.ctx)
		if err != nil {
			rerr = err
			break
		}
		s.rpc.handle(data)
	}
	s.rpc.close()
	code, tail := 0, ""
	if !s.shutting() {
		code, tail = 1, fmt.Sprintf("daemon connection closed: %v", rerr)
	}
	s.finish(code, tail)
}

func (s *codexSession) setTurn(id string) {
	s.tmu.Lock()
	s.turnID = id
	s.tmu.Unlock()
}

func (s *codexSession) turn() string {
	s.tmu.Lock()
	defer s.tmu.Unlock()
	return s.turnID
}

func codexInput(prompt string) []map[string]any {
	return []map[string]any{{"type": "text", "text": prompt}}
}

func (s *codexSession) Send(ctx context.Context, prompt string) error {
	steering, err := s.beginTurn(s.caps.Steer)
	if err != nil {
		return err
	}
	if steering {
		tid := s.turn()
		if tid == "" {
			return fmt.Errorf("codex steer: %w (turn not started yet)", ErrBusy)
		}
		_, err := s.rpc.call(ctx, "turn/steer", map[string]any{
			"threadId": s.SessionID(), "input": codexInput(prompt), "expectedTurnId": tid,
		})
		return err
	}
	res, err := s.rpc.call(ctx, "turn/start", map[string]any{"threadId": s.SessionID(), "input": codexInput(prompt)})
	if err != nil {
		s.endTurn()
		return fmt.Errorf("codex send: %w", err)
	}
	var r struct {
		Turn struct {
			ID string `json:"id"`
		} `json:"turn"`
	}
	if json.Unmarshal(res, &r) == nil && r.Turn.ID != "" {
		s.setTurn(r.Turn.ID)
	}
	return nil
}

func (s *codexSession) Respond(_ context.Context, approvalID string, allow bool, _ string) error {
	d, err := s.takeApproval(approvalID)
	if err != nil {
		return err
	}
	pa := d.(codexPending)
	if pa.method == "item/permissions/requestApproval" {
		granted := json.RawMessage(`{}`)
		if allow {
			var p struct {
				Permissions json.RawMessage `json:"permissions"`
			}
			if json.Unmarshal(pa.params, &p) == nil && len(p.Permissions) > 0 {
				granted = p.Permissions
			}
		}
		return s.rpc.reply(pa.rpcID, map[string]any{"permissions": granted, "scope": "turn"})
	}
	decision := "decline"
	if allow {
		decision = "accept"
	}
	return s.rpc.reply(pa.rpcID, map[string]any{"decision": decision})
}

func (s *codexSession) Interrupt(ctx context.Context) error {
	tid := s.turn()
	if !s.isBusy() || tid == "" {
		return nil
	}
	_, err := s.rpc.call(ctx, "turn/interrupt", map[string]any{"threadId": s.SessionID(), "turnId": tid})
	return err
}

// Close unsubscribes from the thread and drops the connection; the daemon
// itself keeps running.
func (s *codexSession) Close() error {
	if s.markShut() {
		ctx, cancel := context.WithTimeout(context.Background(), s.opts.stopTimeout())
		_, _ = s.rpc.call(ctx, "thread/unsubscribe", map[string]any{"threadId": s.SessionID()})
		cancel()
		_ = s.conn.Close(websocket.StatusNormalClosure, "")
		s.stop()
		_ = s.conn.CloseNow()
	}
	<-s.done
	return nil
}

func (s *codexSession) onNote(method string, params, raw json.RawMessage) {
	var p struct {
		ThreadID string `json:"threadId"`
		Delta    string `json:"delta"`
		Turn     struct {
			ID     string `json:"id"`
			Status string `json:"status"`
			Error  *struct {
				Message string `json:"message"`
			} `json:"error"`
		} `json:"turn"`
		Item struct {
			Type    string `json:"type"`
			ID      string `json:"id"`
			Command string `json:"command"`
			Tool    string `json:"tool"`
		} `json:"item"`
		Usage struct {
			Last struct {
				In     int64 `json:"inputTokens"`
				Out    int64 `json:"outputTokens"`
				Cached int64 `json:"cachedInputTokens"`
			} `json:"last"`
		} `json:"tokenUsage"`
	}
	if json.Unmarshal(params, &p) != nil {
		return
	}
	if p.ThreadID != "" && p.ThreadID != s.SessionID() {
		return
	}
	raw = append([]byte(nil), raw...)
	switch method {
	case "turn/started":
		s.setTurn(p.Turn.ID)
	case "item/agentMessage/delta":
		if p.Delta != "" {
			s.emit(Event{Kind: KindText, Text: p.Delta})
		}
	case "item/started":
		switch p.Item.Type {
		case "userMessage", "agentMessage", "reasoning":
		default:
			name, input := p.Item.Type, json.RawMessage(nil)
			if p.Item.Command != "" {
				input, _ = json.Marshal(map[string]string{"command": p.Item.Command})
			}
			if p.Item.Tool != "" {
				name = p.Item.Tool
			}
			s.emit(Event{Kind: KindToolCall, Tool: &ToolCall{ID: p.Item.ID, Name: name, Input: input}, Raw: raw})
		}
	case "thread/tokenUsage/updated":
		u := p.Usage.Last
		s.emit(Event{Kind: KindUsage, Usage: &Usage{InputTokens: u.In, OutputTokens: u.Out, CachedTokens: u.Cached}})
	case "turn/completed":
		s.setTurn("")
		stop := "end_turn"
		switch p.Turn.Status {
		case "interrupted":
			stop = "cancelled"
		case "failed":
			stop = "error"
			msg := "turn failed"
			if p.Turn.Error != nil {
				msg = p.Turn.Error.Message
			}
			s.emit(Event{Kind: KindError, Text: msg, Raw: raw})
		}
		s.settle(stop, raw)
	}
}

var errCodexUnsupported = errors.New("request not supported by driver")

func (s *codexSession) onRequest(id json.RawMessage, method string, params, raw json.RawMessage) {
	var kind, summary string
	switch method {
	case "item/commandExecution/requestApproval":
		var p struct {
			Command string `json:"command"`
			Reason  string `json:"reason"`
		}
		_ = json.Unmarshal(params, &p)
		kind, summary = "command", p.Command
		if summary == "" {
			summary = p.Reason
		}
	case "item/fileChange/requestApproval":
		var p struct {
			Reason    string `json:"reason"`
			GrantRoot string `json:"grantRoot"`
		}
		_ = json.Unmarshal(params, &p)
		kind, summary = "file_change", p.Reason
		if summary == "" {
			summary = p.GrantRoot
		}
	case "item/permissions/requestApproval":
		var p struct {
			Reason string `json:"reason"`
		}
		_ = json.Unmarshal(params, &p)
		kind, summary = "permissions", p.Reason
	default:
		_ = s.rpc.replyError(id, -32601, errCodexUnsupported.Error()+": "+method)
		s.emit(Event{Kind: KindError, Text: "unsupported codex server request: " + method})
		return
	}
	aid := rpcIDString(id)
	s.addApproval(aid, codexPending{rpcID: append(json.RawMessage(nil), id...), method: method, params: append(json.RawMessage(nil), params...)})
	s.emit(Event{Kind: KindApproval, Approval: &Approval{
		ID: aid, Kind: kind, Summary: truncate(summary, 300), Raw: append([]byte(nil), raw...),
	}})
}
