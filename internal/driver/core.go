package driver

import (
	"context"
	"fmt"
	"sync"
	"unicode/utf8"
)

// core holds the state every session shares: the event stream, the session
// id, the single-turn flag and the pending approvals.
type core struct {
	harness Harness
	caps    Caps
	opts    Options
	events  chan Event
	closing chan struct{}
	done    chan struct{}

	closeOnce sync.Once

	mu        sync.Mutex
	id        string
	busy      bool
	shut      bool
	approvals map[string]any

	compact  chan compactResult
	boundary *Compaction
}

type compactResult struct {
	c   Compaction
	err error
}

func newCore(h Harness, opts Options) *core {
	caps, _ := CapsFor(h)
	return &core{
		harness:   h,
		caps:      caps,
		opts:      opts,
		events:    make(chan Event, 256),
		closing:   make(chan struct{}),
		done:      make(chan struct{}),
		approvals: map[string]any{},
	}
}

func (c *core) Harness() Harness     { return c.harness }
func (c *core) Caps() Caps           { return c.caps }
func (c *core) Events() <-chan Event { return c.events }

func (c *core) SessionID() string {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.id
}

func (c *core) setID(id string) {
	if id == "" {
		return
	}
	c.mu.Lock()
	c.id = id
	c.mu.Unlock()
}

func (c *core) emit(ev Event) {
	select {
	case c.events <- ev:
		return
	default:
	}
	select {
	case c.events <- ev:
	case <-c.closing:
		select {
		case c.events <- ev:
		default:
		}
	}
}

// beginTurn marks a turn active. With steer it reports whether the caller
// should deliver the prompt to the running turn instead.
func (c *core) beginTurn(steer bool) (steering bool, err error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.shut {
		return false, ErrClosed
	}
	if c.busy {
		if steer {
			return true, nil
		}
		return false, ErrBusy
	}
	c.busy = true
	return false, nil
}

// endTurn clears the active flag and reports whether a turn was active, so
// exactly one path emits the terminating event for a turn.
func (c *core) endTurn() bool {
	c.mu.Lock()
	defer c.mu.Unlock()
	was := c.busy
	c.busy = false
	return was
}

func (c *core) isBusy() bool {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.busy
}

func (c *core) settle(stop string, raw []byte) {
	if c.endTurn() {
		c.emit(Event{Kind: KindSettled, StopReason: stop, Raw: raw})
	}
}

func (c *core) addApproval(id string, data any) {
	c.mu.Lock()
	c.approvals[id] = data
	c.mu.Unlock()
}

func (c *core) takeApproval(id string) (any, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	d, ok := c.approvals[id]
	if !ok {
		return nil, fmt.Errorf("%w: %s", ErrUnknownApproval, id)
	}
	delete(c.approvals, id)
	return d, nil
}

func (c *core) takeAllApprovals() map[string]any {
	c.mu.Lock()
	defer c.mu.Unlock()
	out := c.approvals
	c.approvals = map[string]any{}
	return out
}

func (c *core) shutting() bool {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.shut
}

// markShut flags an intentional close; it returns false if already closing.
func (c *core) markShut() bool {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.shut {
		return false
	}
	c.shut = true
	close(c.closing)
	return true
}

// finish runs once when the harness is gone: it reports a mid-turn death,
// emits the final exited event, closes the stream and releases ownership.
func (c *core) finish(exitCode int, tail string) {
	c.closeOnce.Do(func() {
		if c.endTurn() && !c.shutting() {
			msg := fmt.Sprintf("%s exited mid-turn (code %d)", c.harness, exitCode)
			if tail != "" {
				msg += ": " + tail
			}
			c.emit(Event{Kind: KindError, Text: msg})
		}
		c.mu.Lock()
		c.shut = true
		c.mu.Unlock()
		c.emit(Event{Kind: KindExited, ExitCode: exitCode, Text: tail})
		close(c.events)
		if c.opts.release != nil {
			c.opts.release()
		}
		close(c.done)
	})
}

// toolResult builds a KindToolResult event with the output bounded.
func toolResult(callID, output string, isError bool) Event {
	tr := &ToolResult{CallID: callID, IsError: isError}
	if len(output) > MaxToolOutput {
		cut := MaxToolOutput
		for cut > 0 && !utf8.RuneStart(output[cut]) {
			cut--
		}
		output, tr.Truncated = output[:cut], true
	}
	tr.Output = output
	return Event{Kind: KindToolResult, ToolResult: tr}
}

// startCompact marks the session busy for a manual compaction. The harness's
// terminating record must call resolveCompact; settle is skipped for it.
func (c *core) startCompact() (<-chan compactResult, error) {
	if _, err := c.beginTurn(false); err != nil {
		return nil, err
	}
	ch := make(chan compactResult, 1)
	c.mu.Lock()
	c.compact, c.boundary = ch, nil
	c.mu.Unlock()
	return ch, nil
}

func (c *core) compactPending() bool {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.compact != nil
}

// noteBoundary records the harness's own compaction record for a pending
// manual compaction.
func (c *core) noteBoundary(b Compaction) {
	c.mu.Lock()
	if c.compact != nil {
		c.boundary = &b
	}
	c.mu.Unlock()
}

// resolveCompact ends a pending manual compaction and reports whether there
// was one. With no err it succeeds only if a boundary was recorded.
func (c *core) resolveCompact(err error) bool {
	c.mu.Lock()
	ch, b := c.compact, c.boundary
	c.compact, c.boundary = nil, nil
	if ch != nil {
		c.busy = false
	}
	c.mu.Unlock()
	if ch == nil {
		return false
	}
	switch {
	case err != nil:
		ch <- compactResult{err: err}
	case b == nil:
		ch <- compactResult{err: ErrNothingToCompact}
	default:
		ch <- compactResult{c: *b}
	}
	return true
}

func (c *core) waitCompact(ctx context.Context, ch <-chan compactResult) (Compaction, error) {
	select {
	case r := <-ch:
		return r.c, r.err
	case <-ctx.Done():
		c.mu.Lock()
		c.compact, c.boundary = nil, nil
		c.mu.Unlock()
		return Compaction{}, fmt.Errorf("compact: %w", ctx.Err())
	case <-c.done:
		return Compaction{}, ErrClosed
	}
}
