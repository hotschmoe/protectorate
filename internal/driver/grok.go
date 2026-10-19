package driver

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"sync/atomic"
)

// grokSession drives `grok agent stdio` (ACP). The user's global
// permission_mode may silently approve everything, so every session opens
// with the official `/always-approve off` slash command (session-scoped,
// zero tokens, no config change).
type grokSession struct {
	*core
	p    *proc
	rpc  *rpcPeer
	live atomic.Bool
	// perCall is set once a turn reported usage per model call.
	perCall atomic.Bool
}

type acpOption struct {
	ID   string `json:"optionId"`
	Kind string `json:"kind"`
}

func openGrok(ctx context.Context, cwd, sessionID string, opts Options) (Session, error) {
	p, err := startProc(opts.bin("grok"), []string{"agent", "stdio"}, cwd, opts)
	if err != nil {
		return nil, fmt.Errorf("open grok: %w", err)
	}
	s := &grokSession{core: newCore(Grok, opts), p: p}
	s.rpc = newRPCPeer(p.writeJSON)
	s.rpc.onNote = s.onNote
	s.rpc.onReq = s.onRequest
	go func() {
		p.run(s.core, s.rpc.handle)
	}()
	go func() {
		<-s.done
		s.rpc.close()
	}()

	fail := func(err error) (Session, error) {
		s.markShut()
		p.stop(opts.stopTimeout())
		<-s.done
		return nil, fmt.Errorf("open grok: %w", err)
	}
	if _, err := s.rpc.call(ctx, "initialize", map[string]any{
		"protocolVersion":    1,
		"clientCapabilities": map[string]any{},
		"clientInfo":         map[string]any{"name": "protectorate", "version": "0"},
	}); err != nil {
		return fail(err)
	}
	if sessionID == "" {
		res, err := s.rpc.call(ctx, "session/new", map[string]any{"cwd": cwd, "mcpServers": []any{}})
		if err != nil {
			return fail(err)
		}
		var r struct {
			SessionID string `json:"sessionId"`
		}
		if json.Unmarshal(res, &r) != nil || r.SessionID == "" {
			return fail(fmt.Errorf("session/new returned no session id"))
		}
		sessionID = r.SessionID
	} else if _, err := s.rpc.call(ctx, "session/load", map[string]any{"sessionId": sessionID, "cwd": cwd, "mcpServers": []any{}}); err != nil {
		return fail(err)
	}
	s.setID(sessionID)
	gate := "/always-approve off"
	if opts.Sandboxed {
		gate = "/always-approve on"
	}
	wait, err := s.startPrompt(gate)
	if err == nil {
		_, err = wait(ctx)
	}
	if err != nil {
		return fail(fmt.Errorf("%s: %w", gate, err))
	}
	return s, nil
}

func (s *grokSession) startPrompt(text string) (func(context.Context) (json.RawMessage, error), error) {
	return s.rpc.start("session/prompt", map[string]any{
		"sessionId": s.SessionID(),
		"prompt":    []map[string]any{{"type": "text", "text": text}},
	})
}

func (s *grokSession) Send(_ context.Context, prompt string) error {
	if _, err := s.beginTurn(false); err != nil {
		return err
	}
	s.live.Store(true)
	s.perCall.Store(false)
	wait, err := s.startPrompt(prompt)
	if err != nil {
		s.endTurn()
		return fmt.Errorf("grok send: %w", err)
	}
	go func() {
		res, err := wait(context.Background())
		if err == errPeerDead {
			return
		}
		if err != nil {
			if s.endTurn() {
				s.emit(Event{Kind: KindError, Text: err.Error()})
				s.emit(Event{Kind: KindSettled, StopReason: "error"})
			}
			return
		}
		var r struct {
			StopReason string `json:"stopReason"`
			Meta       struct {
				Usage *struct {
					In     int64 `json:"inputTokens"`
					Out    int64 `json:"outputTokens"`
					Cached int64 `json:"cachedReadTokens"`
				} `json:"usage"`
			} `json:"_meta"`
		}
		_ = json.Unmarshal(res, &r)
		if u := r.Meta.Usage; u != nil && !s.perCall.Load() {
			s.emit(Event{Kind: KindUsage, Usage: &Usage{
				ContextTokens: u.In, InputTokens: u.In - u.Cached, OutputTokens: u.Out, CacheReadTokens: u.Cached, Provider: "xai",
			}})
		}
		s.settle(r.StopReason, res)
	}()
	return nil
}

func (s *grokSession) Respond(_ context.Context, approvalID string, allow bool, _ string) error {
	d, err := s.takeApproval(approvalID)
	if err != nil {
		return err
	}
	pa := d.(grokPending)
	return s.rpc.reply(pa.rpcID, map[string]any{"outcome": pickOutcome(pa.options, allow)})
}

func pickOutcome(opts []acpOption, allow bool) map[string]any {
	want, prefix := "reject_once", "reject"
	if allow {
		want, prefix = "allow_once", "allow"
	}
	pick := ""
	for _, o := range opts {
		if o.Kind == want {
			pick = o.ID
			break
		}
		if pick == "" && strings.HasPrefix(o.Kind, prefix) {
			pick = o.ID
		}
	}
	if pick == "" {
		return map[string]any{"outcome": "cancelled"}
	}
	return map[string]any{"outcome": "selected", "optionId": pick}
}

func (s *grokSession) Interrupt(context.Context) error {
	if !s.isBusy() {
		return nil
	}
	for _, d := range s.takeAllApprovals() {
		pa := d.(grokPending)
		_ = s.rpc.reply(pa.rpcID, map[string]any{"outcome": map[string]any{"outcome": "cancelled"}})
	}
	return s.rpc.notify("session/cancel", map[string]any{"sessionId": s.SessionID()})
}

// Compact is not verified for Grok over ACP: the documented /compact slash
// command has not been observed to run or report through this protocol.
func (s *grokSession) Compact(context.Context) (Compaction, error) {
	return Compaction{}, ErrUnsupported
}

func (s *grokSession) Close() error {
	if s.markShut() {
		s.p.stop(s.opts.stopTimeout())
	}
	<-s.done
	return nil
}

type grokPending struct {
	rpcID   json.RawMessage
	options []acpOption
}

func (s *grokSession) onNote(method string, params, raw json.RawMessage) {
	if !s.live.Load() {
		return
	}
	switch method {
	case "session/update":
		s.onUpdate(params, raw)
	case "_x.ai/session_notification":
		var p struct {
			Update struct {
				Usage *struct {
					In         int64 `json:"input_tokens"`
					Out        int64 `json:"output_tokens"`
					CacheRead  int64 `json:"cache_read_input_tokens"`
					CacheWrite int64 `json:"cache_creation_input_tokens"`
				} `json:"usage"`
			} `json:"update"`
		}
		if json.Unmarshal(params, &p) != nil || p.Update.Usage == nil {
			return
		}
		u := p.Update.Usage
		if u.In+u.CacheRead+u.CacheWrite == 0 {
			return
		}
		s.perCall.Store(true)
		s.emit(Event{Kind: KindUsage, Usage: &Usage{
			ContextTokens: u.In + u.CacheRead + u.CacheWrite, InputTokens: u.In, OutputTokens: u.Out,
			CacheReadTokens: u.CacheRead, CacheWriteTokens: u.CacheWrite, Provider: "xai",
		}})
	}
}

func (s *grokSession) onUpdate(params, raw json.RawMessage) {
	var p struct {
		Update struct {
			Kind      string          `json:"sessionUpdate"`
			MessageID string          `json:"messageId"`
			Content   json.RawMessage `json:"content"`
			ID        string          `json:"toolCallId"`
			Title     string          `json:"title"`
			Status    string          `json:"status"`
			Input     json.RawMessage `json:"rawInput"`
			RawOutput json.RawMessage `json:"rawOutput"`
		} `json:"update"`
	}
	if json.Unmarshal(params, &p) != nil {
		return
	}
	raw = append([]byte(nil), raw...)
	u := p.Update
	switch u.Kind {
	case "agent_message_chunk", "agent_thought_chunk":
		var c struct {
			Text string `json:"text"`
		}
		if json.Unmarshal(u.Content, &c) == nil && c.Text != "" {
			s.emit(Event{Kind: KindText, Text: c.Text, MessageID: u.MessageID, Thinking: u.Kind == "agent_thought_chunk"})
		}
	case "tool_call":
		s.emit(Event{Kind: KindToolCall, Tool: &ToolCall{ID: u.ID, Name: u.Title, Input: u.Input}, Raw: raw})
	case "tool_call_update":
		if u.Status != "completed" && u.Status != "failed" {
			return
		}
		var items []struct {
			Content struct {
				Text string `json:"text"`
			} `json:"content"`
		}
		_ = json.Unmarshal(u.Content, &items)
		var parts []string
		for _, c := range items {
			if c.Content.Text != "" {
				parts = append(parts, c.Content.Text)
			}
		}
		out := strings.Join(parts, "\n")
		if out == "" && len(u.RawOutput) > 0 {
			var str string
			if json.Unmarshal(u.RawOutput, &str) == nil {
				out = str
			} else {
				out = string(u.RawOutput)
			}
		}
		ev := toolResult(u.ID, out, u.Status == "failed")
		ev.Raw = raw
		s.emit(ev)
	}
}

func (s *grokSession) onRequest(id json.RawMessage, method string, params, raw json.RawMessage) {
	if method != "session/request_permission" {
		_ = s.rpc.replyError(id, -32601, "method not supported: "+method)
		return
	}
	var p struct {
		ToolCall struct {
			Kind  string          `json:"kind"`
			Title string          `json:"title"`
			Input json.RawMessage `json:"rawInput"`
		} `json:"toolCall"`
		Options []acpOption `json:"options"`
	}
	if json.Unmarshal(params, &p) != nil {
		_ = s.rpc.replyError(id, -32602, "invalid params")
		return
	}
	aid := rpcIDString(id)
	s.addApproval(aid, grokPending{rpcID: append(json.RawMessage(nil), id...), options: p.Options})
	s.emit(Event{Kind: KindApproval, Approval: &Approval{
		ID: aid, Kind: p.ToolCall.Kind, Summary: truncate(p.ToolCall.Title, 300), Raw: append([]byte(nil), raw...),
	}})
}

func rpcIDString(id json.RawMessage) string {
	return strings.Trim(string(id), `"`)
}
