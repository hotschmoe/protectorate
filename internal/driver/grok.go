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
	wait, err := s.startPrompt("/always-approve off")
	if err == nil {
		_, err = wait(ctx)
	}
	if err != nil {
		return fail(fmt.Errorf("disable auto-approve: %w", err))
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
		if u := r.Meta.Usage; u != nil {
			s.emit(Event{Kind: KindUsage, Usage: &Usage{InputTokens: u.In, OutputTokens: u.Out, CachedTokens: u.Cached}})
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
	if method != "session/update" || !s.live.Load() {
		return
	}
	var p struct {
		Update struct {
			Kind    string `json:"sessionUpdate"`
			Content struct {
				Text string `json:"text"`
			} `json:"content"`
			ID    string          `json:"toolCallId"`
			Title string          `json:"title"`
			Input json.RawMessage `json:"rawInput"`
		} `json:"update"`
	}
	if json.Unmarshal(params, &p) != nil {
		return
	}
	raw = append([]byte(nil), raw...)
	switch p.Update.Kind {
	case "agent_message_chunk":
		if p.Update.Content.Text != "" {
			s.emit(Event{Kind: KindText, Text: p.Update.Content.Text})
		}
	case "tool_call":
		s.emit(Event{Kind: KindToolCall, Tool: &ToolCall{ID: p.Update.ID, Name: p.Update.Title, Input: p.Update.Input}, Raw: raw})
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
