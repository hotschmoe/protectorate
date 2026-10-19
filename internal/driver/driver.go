// Package driver runs a coding-agent harness through its own official
// headless protocol (dispatched sessions) behind one small interface.
// The harness is never wrapped or patched: each driver speaks only the
// documented stdio or socket protocol and the harness's own session store.
package driver

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"time"
)

type Harness string

const (
	Claude Harness = "claude"
	Codex  Harness = "codex"
	Grok   Harness = "grok"
	Pi     Harness = "pi"
)

var (
	ErrBusy            = errors.New("driver: turn in progress")
	ErrClosed          = errors.New("driver: session closed")
	ErrUnsupported     = errors.New("driver: not supported by this harness")
	ErrUnknownApproval = errors.New("driver: unknown or already answered approval")
	// ErrNothingToCompact means the harness found too little history to
	// compact; no compaction happened.
	ErrNothingToCompact = errors.New("driver: nothing to compact")
)

// Caps tells callers what a harness supports instead of faking it.
//
//	Approvals: asks before acting; Respond is meaningful
//	Interrupt: Interrupt cancels the running turn
//	Steer:     Send during a running turn is delivered to that turn
//	           (otherwise it returns ErrBusy)
type Caps struct {
	Approvals bool `json:"approvals"`
	Interrupt bool `json:"interrupt"`
	Steer     bool `json:"steer"`
}

func CapsFor(h Harness) (Caps, error) {
	switch h {
	case Claude:
		return Caps{Approvals: true, Interrupt: true}, nil
	case Codex:
		return Caps{Approvals: true, Interrupt: true, Steer: true}, nil
	case Grok:
		return Caps{Approvals: true, Interrupt: true}, nil
	case Pi:
		return Caps{Interrupt: true, Steer: true}, nil
	}
	return Caps{}, fmt.Errorf("driver: unknown harness %q", h)
}

type Kind string

const (
	KindText       Kind = "text"
	KindToolCall   Kind = "tool_call"
	KindToolResult Kind = "tool_result"
	KindApproval   Kind = "approval"
	KindUsage      Kind = "usage"
	KindCompaction Kind = "compaction"
	KindSettled    Kind = "settled"
	KindError      Kind = "error"
	KindExited     Kind = "exited"
)

type ToolCall struct {
	ID    string          `json:"id"`
	Name  string          `json:"name"`
	Input json.RawMessage `json:"input,omitempty"`
}

// MaxToolOutput bounds ToolResult.Output in bytes.
const MaxToolOutput = 16 << 10

// ToolResult is the outcome of a ToolCall. Output is cut at MaxToolOutput
// and Truncated says so.
type ToolResult struct {
	CallID    string `json:"call_id"`
	Output    string `json:"output,omitempty"`
	IsError   bool   `json:"is_error,omitempty"`
	Truncated bool   `json:"truncated,omitempty"`
}

type Approval struct {
	ID      string          `json:"id"`
	Kind    string          `json:"kind"`
	Summary string          `json:"summary"`
	Raw     json.RawMessage `json:"raw,omitempty"`
}

// Usage describes the last model call, never a sum over a turn. Zero means
// the harness did not say.
//
//	ContextTokens:  prompt size of the call (fresh input + cache read + cache write)
//	InputTokens:    the uncached part of that prompt
//	CacheExpires:   when the prompt cache goes cold, if derivable
type Usage struct {
	ContextTokens    int64     `json:"context_tokens"`
	ContextWindow    int64     `json:"context_window,omitempty"`
	InputTokens      int64     `json:"input_tokens"`
	OutputTokens     int64     `json:"output_tokens"`
	CacheReadTokens  int64     `json:"cache_read_tokens,omitempty"`
	CacheWriteTokens int64     `json:"cache_write_tokens,omitempty"`
	CacheExpires     time.Time `json:"cache_expires,omitempty"`
	Model            string    `json:"model,omitempty"`
}

// Compaction records a context compaction the harness confirmed. Native is
// true when the harness started it on its own.
type Compaction struct {
	TokensBefore int64 `json:"tokens_before"`
	TokensAfter  int64 `json:"tokens_after"`
	Native       bool  `json:"native,omitempty"`
}

// Event is one item of a session's stream. Every Send ends in exactly one
// KindSettled, or in KindError followed by KindExited if the harness died.
// KindExited is always last; the channel closes after it.
//
//	KindText:       Text is a delta (Claude delivers whole message blocks);
//	                MessageID groups the deltas of one message when the
//	                harness names it; Thinking marks reasoning text
//	KindToolCall:   Tool
//	KindToolResult: ToolResult
//	KindApproval:   Approval; the turn is blocked until Respond
//	KindUsage:      Usage
//	KindCompaction: Compaction
//	KindSettled:    StopReason
//	KindError:      Text
//	KindExited:     ExitCode, Text holds the bounded stderr tail
type Event struct {
	Kind       Kind            `json:"kind"`
	Text       string          `json:"text,omitempty"`
	MessageID  string          `json:"message_id,omitempty"`
	Thinking   bool            `json:"thinking,omitempty"`
	Tool       *ToolCall       `json:"tool,omitempty"`
	ToolResult *ToolResult     `json:"tool_result,omitempty"`
	Approval   *Approval       `json:"approval,omitempty"`
	Usage      *Usage          `json:"usage,omitempty"`
	Compaction *Compaction     `json:"compaction,omitempty"`
	StopReason string          `json:"stop_reason,omitempty"`
	ExitCode   int             `json:"exit_code,omitempty"`
	Raw        json.RawMessage `json:"raw,omitempty"`
}

type Session interface {
	Harness() Harness
	Caps() Caps
	// SessionID is the harness-native id, known once Open returns.
	SessionID() string
	Send(ctx context.Context, prompt string) error
	Events() <-chan Event
	// Respond answers a KindApproval. note is passed to harnesses that can
	// carry a deny reason and ignored by the rest.
	Respond(ctx context.Context, approvalID string, allow bool, note string) error
	Interrupt(ctx context.Context) error
	// Compact runs the harness's own compaction between turns and returns
	// once the harness's own record confirms it. It fails with ErrBusy
	// during a turn, ErrNothingToCompact when there is too little history
	// and ErrUnsupported on harnesses where it is not verified. The result
	// is also delivered as a KindCompaction event.
	Compact(ctx context.Context) (Compaction, error)
	// Close stops the harness and returns after it has exited, so the
	// session id is free for a TUI to resume.
	Close() error
}

type Options struct {
	// Bin overrides the harness executable (stdio harnesses).
	Bin string
	// Env entries are appended to the process environment.
	Env []string
	// StopTimeout bounds graceful shutdown before the process group is
	// signalled. Default 5s.
	StopTimeout time.Duration
	// StderrLimit bounds captured stderr bytes. Default 8 KiB.
	StderrLimit int
	// Command, when set, turns the harness executable and arguments the
	// driver would run into the argv to start instead, so the sleeve layer can
	// put the harness in its sandbox. stdio stays connected to the harness.
	Command func(bin string, args []string) ([]string, error)
	// Sandboxed says the caller isolates the harness, so the driver runs it
	// in its own autonomous permission mode and raises native
	// auto-compaction, using only the harness's official flags and commands.
	Sandboxed bool

	release func()
}

func (o Options) stopTimeout() time.Duration {
	if o.StopTimeout > 0 {
		return o.StopTimeout
	}
	return 5 * time.Second
}

func (o Options) bin(def string) string {
	if o.Bin != "" {
		return o.Bin
	}
	return def
}

// Open starts a new session (sessionID == "") or resumes a native one. It
// returns once the harness is ready to accept Send.
func Open(ctx context.Context, h Harness, cwd, sessionID string, opts Options) (Session, error) {
	switch h {
	case Claude:
		return openClaude(ctx, cwd, sessionID, opts)
	case Codex:
		return openCodex(ctx, cwd, sessionID, opts)
	case Grok:
		return openGrok(ctx, cwd, sessionID, opts)
	case Pi:
		return openPi(ctx, cwd, sessionID, opts)
	}
	return nil, fmt.Errorf("driver: unknown harness %q", h)
}
