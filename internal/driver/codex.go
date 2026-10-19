package driver

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"sync"
)

// codexSession drives `codex app-server` over stdio, one process per
// session, speaking JSON-RPC lines. Inside a sleeve the per-sleeve CODEX_HOME
// holds the thread, so the process, its config and its threads are all the
// sleeve's own.
type codexSession struct {
	*core
	p   *proc
	rpc *rpcPeer

	tmu    sync.Mutex
	turnID string
	// context bookkeeping for compaction records
	ctxTokens   int64
	compactPost int64
}

type codexPending struct {
	rpcID  json.RawMessage
	method string
	params json.RawMessage
}

func openCodex(ctx context.Context, cwd, sessionID string, opts Options) (Session, error) {
	p, err := startProc(opts.bin("codex"), []string{"app-server"}, cwd, opts)
	if err != nil {
		return nil, fmt.Errorf("open codex: %w", err)
	}
	s := &codexSession{core: newCore(Codex, opts), p: p}
	s.rpc = newRPCPeer(p.writeJSON)
	s.rpc.onNote = s.onNote
	s.rpc.onReq = s.onRequest
	go p.run(s.core, s.rpc.handle)
	go func() {
		<-s.done
		s.rpc.close()
	}()

	fail := func(err error) (Session, error) {
		s.markShut()
		p.stop(opts.stopTimeout())
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
	params := map[string]any{"cwd": cwd}
	if !opts.Sandboxed {
		params["approvalPolicy"], params["sandbox"] = "untrusted", "workspace-write"
	}
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

// Compact starts a compaction turn and waits for the contextCompaction item
// and the turn that carries it to complete.
func (s *codexSession) Compact(ctx context.Context) (Compaction, error) {
	ch, err := s.startCompact()
	if err != nil {
		return Compaction{}, err
	}
	if _, err := s.rpc.call(ctx, "thread/compact/start", map[string]any{"threadId": s.SessionID()}); err != nil {
		s.resolveCompact(err)
		return Compaction{}, fmt.Errorf("codex compact: %w", (<-ch).err)
	}
	return s.waitCompact(ctx, ch)
}

// Close unsubscribes from the thread, then closes stdin so the app-server
// exits and the thread is free for a TUI to resume.
func (s *codexSession) Close() error {
	if s.markShut() {
		ctx, cancel := context.WithTimeout(context.Background(), s.opts.stopTimeout())
		_, _ = s.rpc.call(ctx, "thread/unsubscribe", map[string]any{"threadId": s.SessionID()})
		cancel()
		s.p.stop(s.opts.stopTimeout())
	}
	<-s.done
	return nil
}

func (s *codexSession) onNote(method string, params, raw json.RawMessage) {
	var p struct {
		ThreadID string `json:"threadId"`
		ItemID   string `json:"itemId"`
		Delta    string `json:"delta"`
		Turn     struct {
			ID     string `json:"id"`
			Status string `json:"status"`
			Error  *struct {
				Message string `json:"message"`
			} `json:"error"`
		} `json:"turn"`
		Item struct {
			Type             string `json:"type"`
			ID               string `json:"id"`
			Command          string `json:"command"`
			Tool             string `json:"tool"`
			AggregatedOutput string `json:"aggregatedOutput"`
			ExitCode         *int   `json:"exitCode"`
			Status           string `json:"status"`
		} `json:"item"`
		Usage struct {
			Last struct {
				In         int64 `json:"inputTokens"`
				Out        int64 `json:"outputTokens"`
				Cached     int64 `json:"cachedInputTokens"`
				CacheWrite int64 `json:"cacheWriteInputTokens"`
				Total      int64 `json:"totalTokens"`
			} `json:"last"`
			Window int64 `json:"modelContextWindow"`
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
			s.emit(Event{Kind: KindText, Text: p.Delta, MessageID: p.ItemID})
		}
	case "item/reasoning/summaryTextDelta", "item/reasoning/textDelta":
		if p.Delta != "" {
			s.emit(Event{Kind: KindText, Text: p.Delta, MessageID: p.ItemID, Thinking: true})
		}
	case "item/started":
		switch p.Item.Type {
		case "userMessage", "agentMessage", "reasoning":
		case "contextCompaction":
			s.tmu.Lock()
			s.compactPost = 0
			s.tmu.Unlock()
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
	case "item/completed":
		switch p.Item.Type {
		case "commandExecution":
			failed := p.Item.Status == "failed" || p.Item.ExitCode != nil && *p.Item.ExitCode != 0
			ev := toolResult(p.Item.ID, p.Item.AggregatedOutput, failed)
			ev.Raw = raw
			s.emit(ev)
		case "contextCompaction":
			s.tmu.Lock()
			c := Compaction{TokensBefore: s.ctxTokens, TokensAfter: s.compactPost, Native: !s.compactPending()}
			s.ctxTokens = s.compactPost
			s.tmu.Unlock()
			s.noteBoundary(c)
			s.emit(Event{Kind: KindCompaction, Compaction: &c, Raw: raw})
		}
	case "thread/tokenUsage/updated":
		u := p.Usage.Last
		s.tmu.Lock()
		s.compactPost = u.Total
		s.tmu.Unlock()
		if u.In == 0 {
			return
		}
		s.tmu.Lock()
		s.ctxTokens = u.In
		s.tmu.Unlock()
		s.emit(Event{Kind: KindUsage, Usage: &Usage{
			ContextTokens: u.In, ContextWindow: p.Usage.Window, InputTokens: u.In - u.Cached, OutputTokens: u.Out,
			CacheReadTokens: u.Cached, CacheWriteTokens: u.CacheWrite,
		}})
	case "turn/completed":
		s.setTurn("")
		stop := "end_turn"
		var terr error
		switch p.Turn.Status {
		case "interrupted":
			stop = "cancelled"
		case "failed":
			stop = "error"
			msg := "turn failed"
			if p.Turn.Error != nil {
				msg = p.Turn.Error.Message
			}
			terr = errors.New(msg)
			if !s.compactPending() {
				s.emit(Event{Kind: KindError, Text: msg, Raw: raw})
			}
		}
		if s.compactPending() {
			s.resolveCompact(terr)
			return
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
