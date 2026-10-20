package driver

import (
	"context"
	"encoding/json"
	"fmt"
	"strconv"
	"strings"
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

	manualCompact atomic.Bool
	// the fields below belong to the stdout handler goroutine
	msgSeq int
	model  string
	window int64
	// provider and baseURL come from the model object in get_state
	provider string
	baseURL  string
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
		Model     struct {
			ID            string `json:"id"`
			Provider      string `json:"provider"`
			BaseURL       string `json:"baseUrl"`
			ContextWindow int64  `json:"contextWindow"`
		} `json:"model"`
	}
	if json.Unmarshal(data, &st) != nil || st.SessionID == "" {
		s.markShut()
		p.stop(opts.stopTimeout())
		<-s.done
		return nil, fmt.Errorf("open pi: no session id in get_state")
	}
	s.setID(st.SessionID)
	s.model, s.window, s.provider, s.baseURL = st.Model.ID, st.Model.ContextWindow, st.Model.Provider, st.Model.BaseURL
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

// Compact sends the rpc compact command and trusts its response: pi reports
// success false for a session too small to compact.
func (s *piSession) Compact(ctx context.Context) (Compaction, error) {
	ch, err := s.startCompact()
	if err != nil {
		return Compaction{}, err
	}
	s.manualCompact.Store(true)
	data, err := s.command(ctx, "compact", nil)
	if err != nil {
		s.manualCompact.Store(false)
		if strings.Contains(err.Error(), "Nothing to compact") {
			err = fmt.Errorf("%w: %v", ErrNothingToCompact, err)
		}
		s.resolveCompact(err)
		r := <-ch
		return Compaction{}, r.err
	}
	var r struct {
		Before int64 `json:"tokensBefore"`
		After  int64 `json:"estimatedTokensAfter"`
	}
	if err := json.Unmarshal(data, &r); err != nil {
		s.resolveCompact(fmt.Errorf("pi compact: %w", err))
		return Compaction{}, (<-ch).err
	}
	s.noteBoundary(Compaction{TokensBefore: r.Before, TokensAfter: r.After})
	s.resolveCompact(nil)
	res := <-ch
	return res.c, res.err
}

func (s *piSession) Close() error {
	return s.closeProc(s.p)
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
		IsError bool            `json:"isError"`
		Result  json.RawMessage `json:"result"`
		Message struct {
			Role     string `json:"role"`
			Provider string `json:"provider"`
			Model    string `json:"model"`
			Usage    *struct {
				Input      int64 `json:"input"`
				Output     int64 `json:"output"`
				CacheRead  int64 `json:"cacheRead"`
				CacheWrite int64 `json:"cacheWrite"`
				TotalToken int64 `json:"totalTokens"`
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
	case "message_start":
		if m.Message.Role == "assistant" {
			s.msgSeq++
		}
	case "message_update":
		id := "pi-" + strconv.Itoa(s.msgSeq)
		switch {
		case m.Delta.Type == "text_delta" && m.Delta.Delta != "":
			s.emit(Event{Kind: KindText, Text: m.Delta.Delta, MessageID: id})
		case m.Delta.Type == "thinking_delta" && m.Delta.Delta != "":
			s.emit(Event{Kind: KindText, Text: m.Delta.Delta, MessageID: id, Thinking: true})
		}
	case "message_end":
		if m.Message.Role != "assistant" {
			return
		}
		if m.Message.Model != "" {
			s.model = m.Message.Model
		}
		if m.Message.Provider != "" && m.Message.Provider != s.provider {
			s.provider, s.baseURL = m.Message.Provider, ""
		}
		if u := m.Message.Usage; u != nil && u.TotalToken > 0 {
			s.emit(Event{Kind: KindUsage, Usage: &Usage{
				ContextTokens: u.TotalToken, ContextWindow: s.window, InputTokens: u.Input, OutputTokens: u.Output,
				CacheReadTokens: u.CacheRead, CacheWriteTokens: u.CacheWrite, Model: s.model,
				Provider: s.provider, BaseURL: s.baseURL,
			}})
		}
	case "tool_execution_start":
		s.emit(Event{Kind: KindToolCall, Tool: &ToolCall{ID: m.CallID, Name: m.Name, Input: m.Args}, Raw: raw})
	case "tool_execution_end":
		ev := toolResult(m.CallID, piText(m.Result), m.IsError)
		ev.Raw = raw
		s.emit(ev)
	case "compaction_end":
		manual := s.manualCompact.Swap(false)
		var r struct {
			Before int64 `json:"tokensBefore"`
			After  int64 `json:"estimatedTokensAfter"`
		}
		if len(m.Result) == 0 || json.Unmarshal(m.Result, &r) != nil {
			return
		}
		c := Compaction{TokensBefore: r.Before, TokensAfter: r.After, Native: !manual}
		s.emit(Event{Kind: KindCompaction, Compaction: &c, Raw: raw})
	case "agent_settled":
		stop := "end_turn"
		if m.Aborted {
			stop = "cancelled"
		}
		s.settle(stop, raw)
	}
}

// piText flattens a tool result's content blocks to text.
func piText(raw json.RawMessage) string {
	var r struct {
		Content []struct {
			Type string `json:"type"`
			Text string `json:"text"`
		} `json:"content"`
	}
	if json.Unmarshal(raw, &r) != nil {
		return string(raw)
	}
	var parts []string
	for _, b := range r.Content {
		if b.Type == "text" {
			parts = append(parts, b.Text)
		}
	}
	return strings.Join(parts, "\n")
}
