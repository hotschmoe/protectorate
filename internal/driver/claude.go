package driver

import (
	"context"
	"crypto/rand"
	"encoding/json"
	"fmt"
	"strconv"
	"sync/atomic"
)

type claudeSession struct {
	*core
	p   *proc
	ctl atomic.Int64
}

func newUUID() (string, error) {
	var b [16]byte
	if _, err := rand.Read(b[:]); err != nil {
		return "", err
	}
	b[6] = b[6]&0x0f | 0x40
	b[8] = b[8]&0x3f | 0x80
	return fmt.Sprintf("%x-%x-%x-%x-%x", b[0:4], b[4:6], b[6:8], b[8:10], b[10:]), nil
}

func openClaude(_ context.Context, cwd, sessionID string, opts Options) (Session, error) {
	args := []string{
		"-p", "--output-format", "stream-json", "--input-format", "stream-json", "--verbose",
		"--permission-prompt-tool", "stdio", "--permission-mode", "default",
	}
	if sessionID != "" {
		args = append(args, "--resume", sessionID)
	} else {
		var err error
		if sessionID, err = newUUID(); err != nil {
			return nil, fmt.Errorf("claude session id: %w", err)
		}
		args = append(args, "--session-id", sessionID)
	}
	p, err := startProc(opts.bin("claude"), args, cwd, opts)
	if err != nil {
		return nil, fmt.Errorf("open claude: %w", err)
	}
	s := &claudeSession{core: newCore(Claude, opts), p: p}
	s.setID(sessionID)
	go p.run(s.core, s.handle)
	return s, nil
}

func (s *claudeSession) Send(_ context.Context, prompt string) error {
	if _, err := s.beginTurn(false); err != nil {
		return err
	}
	err := s.p.writeJSON(map[string]any{
		"type":    "user",
		"message": map[string]any{"role": "user", "content": prompt},
	})
	if err != nil {
		s.endTurn()
		return fmt.Errorf("claude send: %w", err)
	}
	return nil
}

func (s *claudeSession) Respond(_ context.Context, approvalID string, allow bool, note string) error {
	d, err := s.takeApproval(approvalID)
	if err != nil {
		return err
	}
	input := d.(json.RawMessage)
	resp := map[string]any{"behavior": "deny", "message": note}
	if note == "" {
		resp["message"] = "denied by user"
	}
	if allow {
		resp = map[string]any{"behavior": "allow", "updatedInput": input}
	}
	return s.p.writeJSON(map[string]any{
		"type": "control_response",
		"response": map[string]any{
			"subtype":    "success",
			"request_id": approvalID,
			"response":   resp,
		},
	})
}

func (s *claudeSession) Interrupt(context.Context) error {
	if !s.isBusy() {
		return nil
	}
	return s.p.writeJSON(map[string]any{
		"type":       "control_request",
		"request_id": "int-" + strconv.FormatInt(s.ctl.Add(1), 10),
		"request":    map[string]any{"subtype": "interrupt"},
	})
}

func (s *claudeSession) Close() error {
	if s.markShut() {
		s.p.stop(s.opts.stopTimeout())
	}
	<-s.done
	return nil
}

type claudeLine struct {
	Type       string  `json:"type"`
	Subtype    string  `json:"subtype"`
	SessionID  string  `json:"session_id"`
	RequestID  string  `json:"request_id"`
	StopReason string  `json:"stop_reason"`
	IsError    bool    `json:"is_error"`
	Result     string  `json:"result"`
	CostUSD    float64 `json:"total_cost_usd"`
	Usage      *struct {
		In     int64 `json:"input_tokens"`
		Out    int64 `json:"output_tokens"`
		Cached int64 `json:"cache_read_input_tokens"`
	} `json:"usage"`
	Message struct {
		Content json.RawMessage `json:"content"`
	} `json:"message"`
	Request struct {
		Subtype  string          `json:"subtype"`
		ToolName string          `json:"tool_name"`
		Input    json.RawMessage `json:"input"`
	} `json:"request"`
}

type claudeBlock struct {
	Type  string          `json:"type"`
	Text  string          `json:"text"`
	ID    string          `json:"id"`
	Name  string          `json:"name"`
	Input json.RawMessage `json:"input"`
}

func (s *claudeSession) handle(raw []byte) {
	var m claudeLine
	if json.Unmarshal(raw, &m) != nil {
		return
	}
	raw = append([]byte(nil), raw...)
	switch m.Type {
	case "system":
		if m.Subtype == "init" {
			s.setID(m.SessionID)
		}
	case "assistant":
		var blocks []claudeBlock
		if json.Unmarshal(m.Message.Content, &blocks) != nil {
			return
		}
		for _, b := range blocks {
			switch b.Type {
			case "text":
				if b.Text != "" {
					s.emit(Event{Kind: KindText, Text: b.Text, Raw: raw})
				}
			case "tool_use":
				s.emit(Event{Kind: KindToolCall, Tool: &ToolCall{ID: b.ID, Name: b.Name, Input: b.Input}, Raw: raw})
			}
		}
	case "control_request":
		if m.Request.Subtype != "can_use_tool" {
			return
		}
		s.addApproval(m.RequestID, m.Request.Input)
		s.emit(Event{Kind: KindApproval, Approval: &Approval{
			ID:      m.RequestID,
			Kind:    m.Request.ToolName,
			Summary: summarizeTool(m.Request.ToolName, m.Request.Input),
			Raw:     raw,
		}})
	case "result":
		s.setID(m.SessionID)
		if m.Usage != nil {
			s.emit(Event{Kind: KindUsage, Usage: &Usage{
				InputTokens: m.Usage.In, OutputTokens: m.Usage.Out, CachedTokens: m.Usage.Cached, CostUSD: m.CostUSD,
			}})
		}
		if m.IsError {
			s.emit(Event{Kind: KindError, Text: m.Result, Raw: raw})
		}
		stop := m.StopReason
		if stop == "" {
			stop = m.Subtype
		}
		s.settle(stop, raw)
	}
}

// summarizeTool renders the one input field a human decides on.
func summarizeTool(name string, input json.RawMessage) string {
	var f map[string]any
	if json.Unmarshal(input, &f) == nil {
		for _, k := range []string{"command", "file_path", "path", "url", "pattern"} {
			if v, ok := f[k].(string); ok && v != "" {
				return truncate(name+": "+v, 300)
			}
		}
	}
	return truncate(name+" "+string(input), 300)
}

func truncate(s string, n int) string {
	if len(s) <= n {
		return s
	}
	return s[:n] + "..."
}
