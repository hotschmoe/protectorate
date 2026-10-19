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
	KindText     Kind = "text"
	KindToolCall Kind = "tool_call"
	KindApproval Kind = "approval"
	KindUsage    Kind = "usage"
	KindSettled  Kind = "settled"
	KindError    Kind = "error"
	KindExited   Kind = "exited"
)

type ToolCall struct {
	ID    string          `json:"id"`
	Name  string          `json:"name"`
	Input json.RawMessage `json:"input,omitempty"`
}

type Approval struct {
	ID      string          `json:"id"`
	Kind    string          `json:"kind"`
	Summary string          `json:"summary"`
	Raw     json.RawMessage `json:"raw,omitempty"`
}

type Usage struct {
	InputTokens  int64   `json:"input_tokens"`
	OutputTokens int64   `json:"output_tokens"`
	CachedTokens int64   `json:"cached_tokens"`
	CostUSD      float64 `json:"cost_usd,omitempty"`
}

// Event is one item of a session's stream. Every Send ends in exactly one
// KindSettled, or in KindError followed by KindExited if the harness died.
// KindExited is always last; the channel closes after it.
//
//	KindText:     Text is a delta (Claude delivers whole message blocks)
//	KindToolCall: Tool
//	KindApproval: Approval; the turn is blocked until Respond
//	KindUsage:    Usage (per turn or cumulative, harness-native)
//	KindSettled:  StopReason
//	KindError:    Text
//	KindExited:   ExitCode, Text holds the bounded stderr tail
type Event struct {
	Kind       Kind            `json:"kind"`
	Text       string          `json:"text,omitempty"`
	Tool       *ToolCall       `json:"tool,omitempty"`
	Approval   *Approval       `json:"approval,omitempty"`
	Usage      *Usage          `json:"usage,omitempty"`
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
	// Close stops the harness and returns after it has exited, so the
	// session id is free for a TUI to resume.
	Close() error
}

type Options struct {
	// Bin overrides the harness executable (stdio harnesses).
	Bin string
	// Env entries are appended to the process environment.
	Env []string
	// CodexSocket overrides the Codex daemon control socket path.
	CodexSocket string
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
