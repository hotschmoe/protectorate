package driver

import (
	"context"
	"encoding/json"
	"fmt"
	"strconv"
	"sync"
	"sync/atomic"
)

// piSession drives `pi --mode rpc`. pi has no approval gate of its own, so
// Caps.Approvals is false and no extension is injected; isolation is the
// caller's job.
type piSession struct {
	*core
	p    *proc
	seq  atomic.Int64
	wmu  sync.Mutex
	wait map[string]chan json.RawMessage
}

func openPi(ctx context.Context, cwd, sessionID string, opts Options) (Session, error) {
	args := []string{"--mode", "rpc"}
	if sessionID != "" {
		args = append(args, "--session", sessionID)
	}
	p, err := startProc(opts.bin("pi"), args, cwd, opts)
	if err != nil {
		return nil, fmt.Errorf("open pi: %w", err)
	}
	s := &piSession{core: newCore(Pi, opts), p: p, wait: map[string]chan json.RawMessage{}}
	go p.run(s.core, s.handle)

	data, err := s.command(ctx, "get_state", nil)
	if err != nil {
		s.markShut()
		p.stop(opts.stopTimeout())
		<-s.done
		return nil, fmt.Errorf("open pi: get_state: %w", err)
	}
	var st struct {
		SessionID string `json:"sessionId"`
	}
	if json.Unmarshal(data, &st) != nil || st.SessionID == "" {
		s.markShut()
		p.stop(opts.stopTimeout())
		<-s.done
		return nil, fmt.Errorf("open pi: no session id in get_state")
	}
	s.setID(st.SessionID)
	return s, nil
}

// command sends a request and waits for its response data.
func (s *piSession) command(ctx context.Context, typ string, fields map[string]any) (json.RawMessage, error) {
	id := "c" + strconv.FormatInt(s.seq.Add(1), 10)
	ch := make(chan json.RawMessage, 1)
	s.wmu.Lock()
	s.wait[id] = ch
	s.wmu.Unlock()
	drop := func() {
		s.wmu.Lock()
		delete(s.wait, id)
		s.wmu.Unlock()
	}
	msg := map[string]any{"id": id, "type": typ}
	for k, v := range fields {
		msg[k] = v
	}
	if err := s.p.writeJSON(msg); err != nil {
		drop()
		return nil, err
	}
	select {
	case raw := <-ch:
		var r struct {
			Success bool            `json:"success"`
			Error   string          `json:"error"`
			Data    json.RawMessage `json:"data"`
		}
		if err := json.Unmarshal(raw, &r); err != nil {
			return nil, err
		}
		if !r.Success {
			return nil, fmt.Errorf("%s failed: %s", typ, r.Error)
		}
		return r.Data, nil
	case <-ctx.Done():
		drop()
		return nil, ctx.Err()
	case <-s.done:
		return nil, fmt.Errorf("pi exited")
	}
}

func (s *piSession) Send(_ context.Context, prompt string) error {
	steering, err := s.beginTurn(s.caps.Steer)
	if err != nil {
		return err
	}
	typ := "prompt"
	if steering {
		typ = "steer"
	}
	id := "c" + strconv.FormatInt(s.seq.Add(1), 10)
	if err := s.p.writeJSON(map[string]any{"id": id, "type": typ, "message": prompt}); err != nil {
		if !steering {
			s.endTurn()
		}
		return fmt.Errorf("pi send: %w", err)
	}
	return nil
}

func (s *piSession) Respond(context.Context, string, bool, string) error { return ErrUnsupported }

func (s *piSession) Interrupt(context.Context) error {
	if !s.isBusy() {
		return nil
	}
	return s.p.writeJSON(map[string]any{"type": "abort"})
}

func (s *piSession) Close() error {
	if s.markShut() {
		s.p.stop(s.opts.stopTimeout())
	}
	<-s.done
	return nil
}

func (s *piSession) handle(raw []byte) {
	var m struct {
		ID      string          `json:"id"`
		Type    string          `json:"type"`
		Command string          `json:"command"`
		Success bool            `json:"success"`
		Error   string          `json:"error"`
		Aborted bool            `json:"aborted"`
		Name    string          `json:"toolName"`
		CallID  string          `json:"toolCallId"`
		Args    json.RawMessage `json:"args"`
		Message struct {
			Role  string `json:"role"`
			Usage *struct {
				Input      int64 `json:"input"`
				Output     int64 `json:"output"`
				CacheRead  int64 `json:"cacheRead"`
				TotalToken int64 `json:"totalTokens"`
				Cost       struct {
					Total float64 `json:"total"`
				} `json:"cost"`
			} `json:"usage"`
		} `json:"message"`
		Delta struct {
			Type  string `json:"type"`
			Delta string `json:"delta"`
		} `json:"assistantMessageEvent"`
	}
	if json.Unmarshal(raw, &m) != nil {
		return
	}
	raw = append([]byte(nil), raw...)
	switch m.Type {
	case "response":
		s.wmu.Lock()
		ch := s.wait[m.ID]
		delete(s.wait, m.ID)
		s.wmu.Unlock()
		if ch != nil {
			ch <- raw
			return
		}
		if !m.Success {
			s.emit(Event{Kind: KindError, Text: fmt.Sprintf("pi %s failed: %s", m.Command, m.Error), Raw: raw})
			if m.Command == "prompt" {
				s.settle("error", raw)
			}
		}
	case "message_update":
		if m.Delta.Type == "text_delta" && m.Delta.Delta != "" {
			s.emit(Event{Kind: KindText, Text: m.Delta.Delta})
		}
	case "message_end":
		if u := m.Message.Usage; m.Message.Role == "assistant" && u != nil && u.TotalToken > 0 {
			s.emit(Event{Kind: KindUsage, Usage: &Usage{
				InputTokens: u.Input, OutputTokens: u.Output, CachedTokens: u.CacheRead, CostUSD: u.Cost.Total,
			}})
		}
	case "tool_execution_start":
		s.emit(Event{Kind: KindToolCall, Tool: &ToolCall{ID: m.CallID, Name: m.Name, Input: m.Args}, Raw: raw})
	case "agent_settled":
		stop := "end_turn"
		if m.Aborted {
			stop = "cancelled"
		}
		s.settle(stop, raw)
	}
}
