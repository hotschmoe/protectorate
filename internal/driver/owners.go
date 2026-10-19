package driver

import (
	"context"
	"fmt"
	"sync"
)

type OwnerKind string

const (
	OwnerDriver OwnerKind = "driver"
	OwnerTUI    OwnerKind = "tui"
)

type Owner struct {
	Kind   OwnerKind
	Holder string
}

// ConflictError reports that a (harness, session id) already has an owner.
type ConflictError struct {
	Harness   Harness
	SessionID string
	Current   Owner
}

func (e *ConflictError) Error() string {
	return fmt.Sprintf("session %s/%s is already owned by %s %q", e.Harness, e.SessionID, e.Current.Kind, e.Current.Holder)
}

type ownerKey struct {
	h  Harness
	id string
}

// Owners enforces at most one live driver or attached TUI per
// (harness, session id). Only Codex guards this itself; the others corrupt
// the conversation when two writers interleave.
type Owners struct {
	mu sync.Mutex
	m  map[ownerKey]Owner
}

func NewOwners() *Owners { return &Owners{m: map[ownerKey]Owner{}} }

// Acquire claims the session. The returned release is idempotent.
func (o *Owners) Acquire(h Harness, sessionID string, kind OwnerKind, holder string) (release func(), err error) {
	if sessionID == "" {
		return nil, fmt.Errorf("driver: acquire %s: empty session id", h)
	}
	k := ownerKey{h, sessionID}
	o.mu.Lock()
	defer o.mu.Unlock()
	if cur, ok := o.m[k]; ok {
		return nil, &ConflictError{Harness: h, SessionID: sessionID, Current: cur}
	}
	me := Owner{Kind: kind, Holder: holder}
	o.m[k] = me
	var once sync.Once
	return func() {
		once.Do(func() {
			o.mu.Lock()
			defer o.mu.Unlock()
			if o.m[k] == me {
				delete(o.m, k)
			}
		})
	}, nil
}

// Holder returns the current owner of a session, if any.
func (o *Owners) Holder(h Harness, sessionID string) (Owner, bool) {
	o.mu.Lock()
	defer o.mu.Unlock()
	cur, ok := o.m[ownerKey{h, sessionID}]
	return cur, ok
}

// lateRelease runs a release func that may be armed after it was triggered.
type lateRelease struct {
	mu    sync.Mutex
	fn    func()
	fired bool
}

func (l *lateRelease) arm(fn func()) {
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.fired {
		fn()
		return
	}
	l.fn = fn
}

func (l *lateRelease) fire() {
	l.mu.Lock()
	defer l.mu.Unlock()
	l.fired = true
	if l.fn != nil {
		l.fn()
	}
}

// OpenDriver acquires the session for a driver and opens it. The claim is
// released when the harness exits or the session is closed. A new session
// is claimed under the id the harness assigns.
func (o *Owners) OpenDriver(ctx context.Context, h Harness, cwd, sessionID, holder string, opts Options) (Session, error) {
	var late lateRelease
	if sessionID != "" {
		r, err := o.Acquire(h, sessionID, OwnerDriver, holder)
		if err != nil {
			return nil, err
		}
		late.arm(r)
	}
	opts.release = late.fire
	s, err := Open(ctx, h, cwd, sessionID, opts)
	if err != nil {
		late.fire()
		return nil, err
	}
	if sessionID == "" {
		r, err := o.Acquire(h, s.SessionID(), OwnerDriver, holder)
		if err != nil {
			_ = s.Close()
			return nil, err
		}
		late.arm(r)
	}
	return s, nil
}
