package driver

import (
	"fmt"
	"sync"
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
