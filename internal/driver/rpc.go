package driver

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"sync"
)

type rpcError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

func (e *rpcError) Error() string { return fmt.Sprintf("rpc error %d: %s", e.Code, e.Message) }

type rpcMsg struct {
	ID     json.RawMessage `json:"id,omitempty"`
	Method string          `json:"method,omitempty"`
	Params json.RawMessage `json:"params,omitempty"`
	Result json.RawMessage `json:"result,omitempty"`
	Error  *rpcError       `json:"error,omitempty"`
}

var errPeerDead = errors.New("driver: connection lost")

// rpcPeer routes JSON-RPC 2.0 messages on the harness's stdio transport.
type rpcPeer struct {
	send   func(any) error
	onNote func(method string, params, raw json.RawMessage)
	onReq  func(id json.RawMessage, method string, params, raw json.RawMessage)

	mu      sync.Mutex
	next    int64
	pending map[int64]chan rpcMsg
	dead    chan struct{}
	once    sync.Once
}

func newRPCPeer(send func(any) error) *rpcPeer {
	return &rpcPeer{send: send, pending: map[int64]chan rpcMsg{}, dead: make(chan struct{})}
}

// handle routes one inbound message; it runs on the transport read loop and
// must not block on a call.
func (p *rpcPeer) handle(raw []byte) {
	var m rpcMsg
	if json.Unmarshal(raw, &m) != nil {
		return
	}
	hasID := len(m.ID) > 0 && !bytes.Equal(m.ID, []byte("null"))
	switch {
	case m.Method != "" && hasID:
		if p.onReq != nil {
			p.onReq(m.ID, m.Method, m.Params, raw)
		}
	case m.Method != "":
		if p.onNote != nil {
			p.onNote(m.Method, m.Params, raw)
		}
	case hasID:
		var n int64
		if json.Unmarshal(m.ID, &n) != nil {
			return
		}
		p.mu.Lock()
		ch := p.pending[n]
		delete(p.pending, n)
		p.mu.Unlock()
		if ch != nil {
			ch <- m
		}
	}
}

func (p *rpcPeer) close() { p.once.Do(func() { close(p.dead) }) }

func (p *rpcPeer) call(ctx context.Context, method string, params any) (json.RawMessage, error) {
	wait, err := p.start(method, params)
	if err != nil {
		return nil, err
	}
	return wait(ctx)
}

// start writes the request before returning, so callers can order it
// against later messages, and hands back the function that awaits the reply.
func (p *rpcPeer) start(method string, params any) (func(context.Context) (json.RawMessage, error), error) {
	ch := make(chan rpcMsg, 1)
	p.mu.Lock()
	p.next++
	id := p.next
	p.pending[id] = ch
	p.mu.Unlock()
	drop := func() {
		p.mu.Lock()
		delete(p.pending, id)
		p.mu.Unlock()
	}
	if err := p.send(map[string]any{"jsonrpc": "2.0", "id": id, "method": method, "params": params}); err != nil {
		drop()
		return nil, fmt.Errorf("send %s: %w", method, err)
	}
	return func(ctx context.Context) (json.RawMessage, error) {
		select {
		case m := <-ch:
			if m.Error != nil {
				return nil, fmt.Errorf("%s: %w", method, m.Error)
			}
			return m.Result, nil
		case <-ctx.Done():
			drop()
			return nil, ctx.Err()
		case <-p.dead:
			drop()
			return nil, errPeerDead
		}
	}, nil
}

func (p *rpcPeer) notify(method string, params any) error {
	m := map[string]any{"jsonrpc": "2.0", "method": method}
	if params != nil {
		m["params"] = params
	}
	return p.send(m)
}

func (p *rpcPeer) reply(id json.RawMessage, result any) error {
	return p.send(map[string]any{"jsonrpc": "2.0", "id": id, "result": result})
}

func (p *rpcPeer) replyError(id json.RawMessage, code int, msg string) error {
	return p.send(map[string]any{"jsonrpc": "2.0", "id": id, "error": map[string]any{"code": code, "message": msg}})
}
