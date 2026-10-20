package mcpbridge

import (
	"bufio"
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"mime"
	"net/http"
	"strings"
	"sync"
	"time"
)

const maxMessage = 16 << 20

var protocolVersions = []string{"2026-07-28", "2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"}

func (s *Server) capabilities() map[string]any {
	caps := map[string]any{"logging": map[string]any{}}
	if len(s.tools) > 0 {
		caps["tools"] = map[string]any{"listChanged": true}
	}
	return caps
}

type request struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params"`
}

type response struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Result  any             `json:"result,omitempty"`
	Error   *rpcError       `json:"error,omitempty"`
}

type rpcError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
	Data    any    `json:"data,omitempty"`
}

type session struct {
	mu          sync.Mutex
	initialized bool
	pending     map[string]context.CancelFunc
	legacyHTTP  bool
	notifyPeer  func(any)
}

func newSession() *session { return &session{pending: make(map[string]context.CancelFunc)} }

func requestKey(id json.RawMessage) string {
	var text string
	if json.Unmarshal(id, &text) == nil {
		return "string:" + text
	}
	return "number:" + string(id)
}

func parseRequest(raw []byte) (request, *response) {
	var req request
	if !json.Valid(raw) {
		return req, failure(nil, -32700, "parse error")
	}
	if err := json.Unmarshal(raw, &req); err != nil || req.JSONRPC != "2.0" || req.Method == "" {
		return req, failure(nil, -32600, "invalid request")
	}
	if len(req.ID) > 0 {
		var id any
		if err := json.Unmarshal(req.ID, &id); err != nil {
			return req, failure(nil, -32600, "invalid request id")
		}
		switch id.(type) {
		case string, float64, nil:
		default:
			return req, failure(nil, -32600, "invalid request id")
		}
	}
	return req, nil
}

func failure(id json.RawMessage, code int, message string) *response {
	if len(id) == 0 {
		id = json.RawMessage("null")
	}
	return &response{JSONRPC: "2.0", ID: id, Error: &rpcError{Code: code, Message: message}}
}

func (s *session) notify(req request) {
	if req.Method != "notifications/cancelled" {
		return
	}
	var params struct {
		RequestID json.RawMessage `json:"requestId"`
	}
	if json.Unmarshal(req.Params, &params) != nil {
		return
	}
	s.mu.Lock()
	cancel := s.pending[requestKey(params.RequestID)]
	s.mu.Unlock()
	if cancel != nil {
		cancel()
	}
}

const maxPendingRequests = 128

func (s *session) begin(ctx context.Context, req request) (context.Context, func(), *response) {
	ctx, cancel := context.WithCancel(ctx)
	key := requestKey(req.ID)
	s.mu.Lock()
	if _, ok := s.pending[key]; ok {
		s.mu.Unlock()
		cancel()
		return nil, nil, failure(req.ID, -32600, "duplicate request id")
	}
	if len(s.pending) >= maxPendingRequests {
		s.mu.Unlock()
		cancel()
		return nil, nil, failure(req.ID, -32000, "too many requests in progress")
	}
	s.pending[key] = cancel
	s.mu.Unlock()
	return ctx, func() {
		cancel()
		s.mu.Lock()
		delete(s.pending, key)
		s.mu.Unlock()
	}, nil
}

func (s *Server) dispatch(ctx context.Context, ss *session, req request) (res *response) {
	var params struct {
		Meta map[string]json.RawMessage `json:"_meta"`
	}
	if len(req.Params) > 0 && json.Unmarshal(req.Params, &params) != nil {
		return failure(req.ID, -32602, "invalid parameters")
	}
	var version string
	_ = json.Unmarshal(params.Meta["io.modelcontextprotocol/protocolVersion"], &version)
	modern := version >= "2026-07-28"
	if modern {
		var info map[string]any
		var caps map[string]any
		if json.Unmarshal(params.Meta["io.modelcontextprotocol/clientInfo"], &info) != nil || info == nil || json.Unmarshal(params.Meta["io.modelcontextprotocol/clientCapabilities"], &caps) != nil || caps == nil {
			return failure(req.ID, -32602, "missing per-request clientInfo or clientCapabilities")
		}
		if version != "2026-07-28" {
			res = failure(req.ID, -32022, "unsupported protocol version")
			res.Error.Data = map[string]any{"requested": version, "supported": protocolVersions}
			return res
		}
		switch req.Method {
		case "initialize", "ping", "logging/setLevel":
			return failure(req.ID, -32601, fmt.Sprintf("method not found: %q", req.Method))
		}
		defer func() {
			if res == nil || res.Error != nil {
				return
			}
			raw, err := json.Marshal(res.Result)
			if err != nil {
				res = failure(req.ID, -32603, "encode result")
				return
			}
			var result map[string]any
			if err := json.Unmarshal(raw, &result); err != nil {
				res = failure(req.ID, -32603, "encode result")
				return
			}
			result["resultType"] = "complete"
			meta, ok := result["_meta"].(map[string]any)
			if !ok {
				meta = map[string]any{}
			}
			meta["io.modelcontextprotocol/serverInfo"] = map[string]any{"name": "protectorate", "version": s.version}
			result["_meta"] = meta
			if req.Method == "server/discover" || req.Method == "tools/list" {
				result["ttlMs"] = 0
				result["cacheScope"] = "public"
			}
			res.Result = result
		}()
	}
	result := &response{JSONRPC: "2.0", ID: req.ID}
	switch req.Method {
	case "server/discover":
		if !modern {
			return failure(req.ID, -32601, "server/discover requires protocol version 2026-07-28")
		}
		versions := protocolVersions
		if ss.legacyHTTP {
			versions = versions[1:]
		}
		result.Result = map[string]any{"supportedVersions": versions, "capabilities": s.capabilities()}
		return result
	case "initialize":
		var params struct {
			ProtocolVersion string `json:"protocolVersion"`
		}
		if json.Unmarshal(req.Params, &params) != nil || params.ProtocolVersion == "" {
			return failure(req.ID, -32602, "invalid initialize parameters")
		}
		version := params.ProtocolVersion
		switch version {
		case "2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25":
		default:
			version = "2025-11-25"
		}
		ss.mu.Lock()
		ss.initialized = true
		ss.mu.Unlock()
		result.Result = map[string]any{"protocolVersion": version, "capabilities": s.capabilities(), "serverInfo": map[string]any{"name": "protectorate", "version": s.version}}
		return result
	case "ping":
		result.Result = struct{}{}
		return result
	}
	ss.mu.Lock()
	ready := ss.initialized
	ss.mu.Unlock()
	if !ready && !modern {
		return failure(req.ID, -32600, "session is not initialized")
	}
	switch req.Method {
	case "subscriptions/listen":
		if !modern || ss.notifyPeer == nil {
			return failure(req.ID, -32601, "subscriptions/listen unavailable on this transport")
		}
		var params struct {
			Notifications *struct {
				ToolsListChanged bool `json:"toolsListChanged"`
			} `json:"notifications"`
		}
		if json.Unmarshal(req.Params, &params) != nil || params.Notifications == nil {
			return failure(req.ID, -32602, "missing notifications")
		}
		agreed := map[string]any{}
		if params.Notifications.ToolsListChanged && len(s.tools) > 0 {
			agreed["toolsListChanged"] = true
		}
		meta := map[string]any{"io.modelcontextprotocol/subscriptionId": req.ID}
		ss.notifyPeer(map[string]any{"jsonrpc": "2.0", "method": "notifications/subscriptions/acknowledged", "params": map[string]any{"_meta": meta, "notifications": agreed}})
		if len(agreed) > 0 {
			<-ctx.Done()
		}
		result.Result = map[string]any{"_meta": meta}
	case "tools/list":
		var params struct {
			Cursor string `json:"cursor"`
		}
		if len(req.Params) > 0 && json.Unmarshal(req.Params, &params) != nil {
			return failure(req.ID, -32602, "invalid tools/list parameters")
		}
		if params.Cursor != "" {
			return failure(req.ID, -32602, "invalid cursor")
		}
		result.Result = struct {
			Tools []Tool `json:"tools"`
		}{s.tools}
	case "tools/call":
		var params struct {
			Name      string          `json:"name"`
			Arguments json.RawMessage `json:"arguments"`
		}
		if json.Unmarshal(req.Params, &params) != nil || params.Name == "" {
			return failure(req.ID, -32602, "invalid tools/call parameters")
		}
		handler, ok := s.handlers[params.Name]
		if !ok {
			return failure(req.ID, -32602, fmt.Sprintf("unknown tool %q", params.Name))
		}
		result.Result = handler(ctx, params.Arguments)
	case "logging/setLevel":
		var params struct {
			Level string `json:"level"`
		}
		if json.Unmarshal(req.Params, &params) != nil {
			return failure(req.ID, -32602, "invalid logging parameters")
		}
		switch params.Level {
		case "debug", "info", "notice", "warning", "error", "critical", "alert", "emergency":
		default:
			return failure(req.ID, -32602, "invalid log level")
		}
		result.Result = struct{}{}
	default:
		return failure(req.ID, -32601, fmt.Sprintf("method %q not found", req.Method))
	}
	return result
}

// Run exchanges newline-delimited JSON-RPC messages until EOF or cancellation.
func (s *Server) Run(ctx context.Context, in io.ReadCloser, out io.Writer) error {
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	lines := make(chan []byte)
	readErr := make(chan error, 1)
	go func() {
		scanner := bufio.NewScanner(in)
		scanner.Buffer(make([]byte, 4096), maxMessage)
		for scanner.Scan() {
			line := append([]byte(nil), scanner.Bytes()...)
			select {
			case lines <- line:
			case <-ctx.Done():
				return
			}
		}
		readErr <- scanner.Err()
		close(lines)
	}()
	go func() { <-ctx.Done(); in.Close() }()
	ss := newSession()
	var workers sync.WaitGroup
	var writeMu sync.Mutex
	var writeErr error
	send := func(res any) {
		writeMu.Lock()
		defer writeMu.Unlock()
		if writeErr != nil {
			return
		}
		if err := json.NewEncoder(out).Encode(res); err != nil {
			writeErr = fmt.Errorf("write MCP response: %w", err)
			cancel()
		}
	}
	ss.notifyPeer = send
	defer workers.Wait()
	for {
		select {
		case <-ctx.Done():
			workers.Wait()
			if writeErr != nil {
				return writeErr
			}
			return nil
		case line, ok := <-lines:
			if !ok {
				cancel()
				workers.Wait()
				if writeErr != nil {
					return writeErr
				}
				if err := <-readErr; err != nil {
					return fmt.Errorf("read MCP message: %w", err)
				}
				return nil
			}
			if len(strings.TrimSpace(string(line))) == 0 {
				continue
			}
			req, invalid := parseRequest(line)
			if invalid != nil {
				send(invalid)
				continue
			}
			if len(req.ID) == 0 {
				ss.notify(req)
				continue
			}
			callCtx, finish, invalid := ss.begin(ctx, req)
			if invalid != nil {
				send(invalid)
				continue
			}
			if req.Method != "tools/call" && req.Method != "subscriptions/listen" {
				send(s.dispatch(callCtx, ss, req))
				finish()
				continue
			}
			workers.Add(1)
			go func() { defer workers.Done(); defer finish(); send(s.dispatch(callCtx, ss, req)) }()
		}
	}
}

type httpSession struct {
	*session
	ctx      context.Context
	cancel   context.CancelFunc
	lastUsed time.Time
	active   int
}

const maxHTTPSessions = 128
const httpSessionIdle = time.Hour

func (s *Server) httpHandler() http.Handler {
	var mu sync.Mutex
	sessions := map[string]*httpSession{}
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		id := r.Header.Get("Mcp-Session-Id")
		mu.Lock()
		now := time.Now()
		for key, candidate := range sessions {
			if candidate.active == 0 && now.Sub(candidate.lastUsed) >= httpSessionIdle {
				delete(sessions, key)
				candidate.cancel()
			}
		}
		ss := sessions[id]
		if ss != nil {
			ss.active++
		}
		mu.Unlock()
		if ss != nil {
			defer func(existing *httpSession) {
				mu.Lock()
				existing.active--
				existing.lastUsed = time.Now()
				mu.Unlock()
			}(ss)
		}
		if id != "" && ss == nil {
			http.Error(w, "session not found", http.StatusNotFound)
			return
		}
		if r.Method == http.MethodDelete {
			if ss == nil {
				http.Error(w, "session id required", http.StatusBadRequest)
				return
			}
			mu.Lock()
			delete(sessions, id)
			mu.Unlock()
			ss.cancel()
			w.WriteHeader(http.StatusNoContent)
			return
		}
		if r.Method == http.MethodGet {
			if ss == nil {
				http.Error(w, "session id required", http.StatusBadRequest)
				return
			}
			w.Header().Set("Content-Type", "text/event-stream")
			w.Header().Set("Cache-Control", "no-cache")
			w.WriteHeader(http.StatusOK)
			if f, ok := w.(http.Flusher); ok {
				f.Flush()
			}
			select {
			case <-r.Context().Done():
			case <-ss.ctx.Done():
			}
			return
		}
		if r.Method != http.MethodPost {
			w.Header().Set("Allow", "GET, POST, DELETE")
			w.WriteHeader(http.StatusMethodNotAllowed)
			return
		}
		media, _, err := mime.ParseMediaType(r.Header.Get("Content-Type"))
		if err != nil || media != "application/json" {
			http.Error(w, "expected application/json", http.StatusUnsupportedMediaType)
			return
		}
		raw, err := io.ReadAll(http.MaxBytesReader(w, r.Body, maxMessage))
		if err != nil {
			http.Error(w, "read MCP request", http.StatusBadRequest)
			return
		}
		req, res := parseRequest(raw)
		created := false
		retained := false
		if res == nil && ss == nil {
			if req.Method != "initialize" && req.Method != "server/discover" {
				http.Error(w, "session id required", http.StatusBadRequest)
				return
			}
			sessionCtx, cancel := context.WithCancel(context.Background())
			ss = &httpSession{session: newSession(), ctx: sessionCtx, cancel: cancel}
			created = true
			defer func() {
				if !retained {
					cancel()
				}
			}()
			ss.legacyHTTP = true
			var secret [24]byte
			if _, err := rand.Read(secret[:]); err != nil {
				cancel()
				http.Error(w, "create session", http.StatusInternalServerError)
				return
			}
			id = hex.EncodeToString(secret[:])
		}
		if res == nil && len(req.ID) == 0 {
			ss.notify(req)
			w.WriteHeader(http.StatusAccepted)
			return
		}
		if res == nil {
			ctx, cancel := context.WithCancel(r.Context())
			stop := context.AfterFunc(ss.ctx, cancel)
			defer stop()
			defer cancel()
			callCtx, finish, invalid := ss.begin(ctx, req)
			if invalid != nil {
				res = invalid
			} else {
				res = s.dispatch(callCtx, ss.session, req)
				finish()
			}
			if req.Method == "initialize" && res.Error == nil {
				if created {
					mu.Lock()
					if len(sessions) >= maxHTTPSessions {
						mu.Unlock()
						http.Error(w, "too many MCP sessions; close an existing session and retry", http.StatusServiceUnavailable)
						return
					}
					ss.lastUsed = time.Now()
					sessions[id] = ss
					retained = true
					mu.Unlock()
				}
				w.Header().Set("Mcp-Session-Id", id)
			}
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(res)
	})
}
