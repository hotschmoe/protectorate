package driver

import (
	"context"
	"crypto/rand"
	"encoding/json"
	"fmt"
	"strconv"
	"strings"
	"sync/atomic"
	"time"
)

type claudeSession struct {
	*core
	p   *proc
	ctl atomic.Int64

	// the fields below belong to the stdout handler goroutine
	usageID  string
	last     Usage
	window   int64
	cacheTTL time.Duration
}

const (
	cacheTTL5m = 5 * time.Minute
	cacheTTL1h = time.Hour
)

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
	if opts.Autonomous {
		args[len(args)-1] = "bypassPermissions"
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
	s := &claudeSession{core: newCore(Claude, opts), p: p, cacheTTL: cacheTTL5m}
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

// Compact sends /compact as a user message and waits for the compact_boundary
// system record, which is Claude's own proof that it compacted.
func (s *claudeSession) Compact(ctx context.Context) (Compaction, error) {
	ch, err := s.startCompact()
	if err != nil {
		return Compaction{}, err
	}
	err = s.p.writeJSON(map[string]any{
		"type":    "user",
		"message": map[string]any{"role": "user", "content": "/compact"},
	})
	if err != nil {
		s.resolveCompact(err)
		return Compaction{}, fmt.Errorf("claude compact: %w", err)
	}
	return s.waitCompact(ctx, ch)
}

func (s *claudeSession) Close() error {
	if s.markShut() {
		s.p.stop(s.opts.stopTimeout())
	}
	<-s.done
	return nil
}

type claudeLine struct {
	Type            string  `json:"type"`
	Subtype         string  `json:"subtype"`
	SessionID       string  `json:"session_id"`
	RequestID       string  `json:"request_id"`
	StopReason      string  `json:"stop_reason"`
	IsError         bool    `json:"is_error"`
	Result          string  `json:"result"`
	ParentToolUseID *string `json:"parent_tool_use_id"`
	ModelUsage      map[string]struct {
		ContextWindow int64 `json:"contextWindow"`
	} `json:"modelUsage"`
	CompactMetadata struct {
		Trigger string `json:"trigger"`
		Pre     int64  `json:"pre_tokens"`
		Post    int64  `json:"post_tokens"`
	} `json:"compact_metadata"`
	Message struct {
		ID      string          `json:"id"`
		Model   string          `json:"model"`
		Content json.RawMessage `json:"content"`
		Usage   *claudeUsage    `json:"usage"`
	} `json:"message"`
	Request struct {
		Subtype  string          `json:"subtype"`
		ToolName string          `json:"tool_name"`
		Input    json.RawMessage `json:"input"`
	} `json:"request"`
}

type claudeUsage struct {
	In            int64 `json:"input_tokens"`
	Out           int64 `json:"output_tokens"`
	CacheWrite    int64 `json:"cache_creation_input_tokens"`
	CacheRead     int64 `json:"cache_read_input_tokens"`
	CacheCreation struct {
		Ephemeral5m int64 `json:"ephemeral_5m_input_tokens"`
		Ephemeral1h int64 `json:"ephemeral_1h_input_tokens"`
	} `json:"cache_creation"`
}

type claudeBlock struct {
	Type      string          `json:"type"`
	Text      string          `json:"text"`
	Thinking  string          `json:"thinking"`
	ID        string          `json:"id"`
	Name      string          `json:"name"`
	Input     json.RawMessage `json:"input"`
	ToolUseID string          `json:"tool_use_id"`
	Content   json.RawMessage `json:"content"`
	IsError   bool            `json:"is_error"`
}

func (s *claudeSession) handle(raw []byte) {
	var m claudeLine
	if json.Unmarshal(raw, &m) != nil {
		return
	}
	raw = append([]byte(nil), raw...)
	switch m.Type {
	case "system":
		switch m.Subtype {
		case "init":
			s.setID(m.SessionID)
		case "compact_boundary":
			c := Compaction{TokensBefore: m.CompactMetadata.Pre, TokensAfter: m.CompactMetadata.Post, Native: m.CompactMetadata.Trigger == "auto"}
			s.noteBoundary(c)
			s.emit(Event{Kind: KindCompaction, Compaction: &c, Raw: raw})
		}
	case "assistant":
		if m.ParentToolUseID != nil {
			return
		}
		var blocks []claudeBlock
		if json.Unmarshal(m.Message.Content, &blocks) != nil {
			return
		}
		if u := m.Message.Usage; u != nil && m.Message.ID != "" && m.Message.ID != s.usageID {
			s.usageID = m.Message.ID
			s.emitUsage(u, m.Message.Model)
		}
		for _, b := range blocks {
			switch b.Type {
			case "text":
				if b.Text != "" {
					s.emit(Event{Kind: KindText, Text: b.Text, MessageID: m.Message.ID, Raw: raw})
				}
			case "thinking":
				if b.Thinking != "" {
					s.emit(Event{Kind: KindText, Text: b.Thinking, MessageID: m.Message.ID, Thinking: true, Raw: raw})
				}
			case "tool_use":
				s.emit(Event{Kind: KindToolCall, Tool: &ToolCall{ID: b.ID, Name: b.Name, Input: b.Input}, Raw: raw})
			}
		}
	case "user":
		if m.ParentToolUseID != nil {
			return
		}
		var blocks []claudeBlock
		if json.Unmarshal(m.Message.Content, &blocks) != nil {
			return
		}
		for _, b := range blocks {
			if b.Type == "tool_result" {
				ev := toolResult(b.ToolUseID, claudeText(b.Content), b.IsError)
				ev.Raw = raw
				s.emit(ev)
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
		var w int64
		for _, mu := range m.ModelUsage {
			w = max(w, mu.ContextWindow)
		}
		if w > 0 && w != s.window {
			s.window = w
			if s.last.ContextTokens > 0 {
				u := s.last
				u.ContextWindow = w
				s.last = u
				s.emit(Event{Kind: KindUsage, Usage: &u})
			}
		}
		if s.compactPending() {
			var err error
			if m.IsError {
				err = fmt.Errorf("claude compact: %s", m.Result)
			}
			s.resolveCompact(err)
			return
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

// emitUsage reports the prompt size of one model call. The cache lifetime is
// the 1 hour bucket when Claude wrote any, otherwise 5 minutes, and carries
// over to calls that only read the cache.
func (s *claudeSession) emitUsage(u *claudeUsage, model string) {
	if u.In+u.CacheWrite+u.CacheRead == 0 {
		return // Claude's own synthetic messages, such as a login error
	}
	switch {
	case u.CacheCreation.Ephemeral1h > 0:
		s.cacheTTL = cacheTTL1h
	case u.CacheCreation.Ephemeral5m > 0 || u.CacheWrite > 0:
		s.cacheTTL = cacheTTL5m
	}
	out := Usage{
		ContextTokens: u.In + u.CacheWrite + u.CacheRead, ContextWindow: s.window,
		InputTokens: u.In, OutputTokens: u.Out, CacheReadTokens: u.CacheRead, CacheWriteTokens: u.CacheWrite, Model: model, Provider: "anthropic",
	}
	if u.CacheWrite+u.CacheRead > 0 {
		out.CacheExpires = time.Now().Add(s.cacheTTL)
	}
	s.last = out
	s.emit(Event{Kind: KindUsage, Usage: &out})
}

// claudeText flattens a tool_result content, a string or a list of blocks.
func claudeText(raw json.RawMessage) string {
	var str string
	if json.Unmarshal(raw, &str) == nil {
		return str
	}
	var blocks []claudeBlock
	if json.Unmarshal(raw, &blocks) != nil {
		return string(raw)
	}
	var parts []string
	for _, b := range blocks {
		if b.Type == "text" {
			parts = append(parts, b.Text)
		}
	}
	return strings.Join(parts, "\n")
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
