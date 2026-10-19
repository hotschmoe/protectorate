package hub

import (
	"context"
	"errors"
	"fmt"
	"log"
	"regexp"
	"sync"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/keylog"
)

const (
	requestIDWindow = time.Hour
	maxRequestIDs   = 10000
)

var requestIDRe = regexp.MustCompile(`^[A-Za-z0-9._-]{8,128}$`)

// keyEntry is what the hub remembers about one idempotency key. done closes
// when the first submit with the key has finished; id and err are valid then.
type keyEntry struct {
	done     chan struct{}
	id       string
	err      error
	canceled bool
	expires  time.Time
}

// submitKeys dedupes SubmitJob by request id for a bounded window. With a log
// it survives a hub restart.
type submitKeys struct {
	now func() time.Time
	log *keylog.Log

	mu      sync.Mutex
	entries map[string]*keyEntry
}

func newSubmitKeys(now func() time.Time) *submitKeys {
	return &submitKeys{now: now, entries: map[string]*keyEntry{}}
}

// sweepLocked drops finished entries past their window.
func (k *submitKeys) sweepLocked() {
	now := k.now()
	for key, e := range k.entries {
		select {
		case <-e.done:
			if !e.expires.After(now) {
				delete(k.entries, key)
			}
		default:
		}
	}
}

// restore adopts the entries a previous hub persisted and records the rest in
// log.
func (k *submitKeys) restore(log *keylog.Log, entries []keylog.Entry) {
	k.log = log
	for _, p := range entries {
		e := &keyEntry{done: make(chan struct{}), id: p.Value, expires: p.Expires}
		if p.Err != "" {
			e.err, e.canceled = connect.NewError(connect.Code(p.Code), errors.New(p.Err)), true
		}
		close(e.done)
		k.entries[p.Key] = e
	}
}

func (k *submitKeys) persist(key string, e *keyEntry) {
	if k.log == nil {
		return
	}
	p := keylog.Entry{Key: key, Value: e.id, Expires: e.expires}
	if e.err != nil {
		p.Code, p.Err = uint32(connect.CodeOf(e.err)), e.err.Error()
	}
	if err := k.log.Append(p); err != nil {
		log.Printf("jobs: persist request id: %v", err)
	}
}

func (k *submitKeys) tombstoneLocked(key string, err error) {
	e := &keyEntry{done: make(chan struct{}), err: err, canceled: true, expires: k.now().Add(requestIDWindow)}
	close(e.done)
	k.entries[key] = e
	k.persist(key, e)
}

func wait(ctx context.Context, e *keyEntry) error {
	select {
	case <-e.done:
		return nil
	case <-ctx.Done():
		return connect.NewError(connect.CodeCanceled, ctx.Err())
	}
}

func checkRequestID(key string) error {
	if !requestIDRe.MatchString(key) {
		return invalid("request_id must be 8 to 128 characters of letters, digits, '.', '_' and '-'")
	}
	return nil
}

var errCanceledBeforeSubmit = connect.NewError(connect.CodeAborted, fmt.Errorf("the submit was canceled by its request_id before it reached the hub"))

// submit runs create once per key. A repeat returns the job of the first call
// as get reports it now, waiting for the first call if it is still in flight.
func (k *submitKeys) submit(ctx context.Context, key string, create func() (*pb.Job, error), get func(id string) (*pb.Job, error)) (*pb.Job, error) {
	if err := checkRequestID(key); err != nil {
		return nil, err
	}
	k.mu.Lock()
	k.sweepLocked()
	if e := k.entries[key]; e != nil {
		k.mu.Unlock()
		if err := wait(ctx, e); err != nil {
			return nil, err
		}
		if e.err != nil {
			return nil, e.err
		}
		return get(e.id)
	}
	if len(k.entries) >= maxRequestIDs {
		k.mu.Unlock()
		return nil, connect.NewError(connect.CodeResourceExhausted, fmt.Errorf("too many request ids in use; retry later"))
	}
	e := &keyEntry{done: make(chan struct{})}
	k.entries[key] = e
	k.mu.Unlock()

	j, err := create()

	k.mu.Lock()
	switch {
	case err == nil:
		e.id = j.Id
		e.expires = k.now().Add(requestIDWindow)
		k.persist(key, e)
	case e.canceled:
		e.err = err
		e.expires = k.now().Add(requestIDWindow)
		k.persist(key, e)
	default:
		delete(k.entries, key)
		e.err = err
	}
	close(e.done)
	k.mu.Unlock()
	return j, err
}

// cancel cancels the job a key refers to. A submit still in flight is
// awaited, so the job exists by the time it is canceled. A key the hub has not
// seen is remembered as canceled, so a submit that is still on its way is
// refused.
func (k *submitKeys) cancel(ctx context.Context, key string, cancel func(id string) (*pb.Job, error)) (*pb.Job, error) {
	if err := checkRequestID(key); err != nil {
		return nil, err
	}
	k.mu.Lock()
	k.sweepLocked()
	e := k.entries[key]
	if e == nil {
		k.tombstoneLocked(key, errCanceledBeforeSubmit)
		k.mu.Unlock()
		return nil, connect.NewError(connect.CodeNotFound, fmt.Errorf("no job for request_id %q; a later submit with it is refused", key))
	}
	e.canceled = true
	k.mu.Unlock()
	if err := wait(ctx, e); err != nil {
		return nil, err
	}
	if e.err != nil {
		return nil, connect.NewError(connect.CodeNotFound, fmt.Errorf("no job for request_id %q: %w", key, e.err))
	}
	return cancel(e.id)
}

func (m *JobManager) snapshotOf(id string) (*pb.Job, error) {
	e, err := m.get(id)
	if err != nil {
		return nil, err
	}
	j, _ := e.snapshot()
	return j, nil
}
