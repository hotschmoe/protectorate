package hub

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"time"
)

func (h *Hub) authorizeDebug(r *http.Request) error {
	p, err := h.authz.Authenticate(bearerToken(r.Header))
	if err != nil || !p.Owner {
		return errors.New("owner token required")
	}
	return nil
}

func (h *Hub) handleDebugNode(w http.ResponseWriter, r *http.Request) {
	path := r.PathValue("path")
	switch path {
	case "metrics", "pprof/heap", "pprof/goroutine", "pprof/profile":
	default:
		http.NotFound(w, r)
		return
	}
	if h.nodes == nil {
		http.Error(w, "node monitor unavailable", http.StatusServiceUnavailable)
		return
	}
	n, err := h.nodes.info(r.PathValue("name"))
	if err != nil {
		http.Error(w, err.Error(), http.StatusNotFound)
		return
	}
	if n.Local {
		http.Error(w, "use the hub's debug endpoint for its local node", http.StatusBadRequest)
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), 2*time.Minute)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, AgentURL(n.Address)+"/debug/"+path, nil)
	if err != nil {
		http.Error(w, fmt.Errorf("build node profile request: %w", err).Error(), http.StatusBadGateway)
		return
	}
	req.URL.RawQuery = r.URL.RawQuery
	client := *h.nodes.opts.Client
	client.CheckRedirect = func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }
	if transport, ok := client.Transport.(*http.Transport); ok {
		transport = transport.Clone()
		transport.ResponseHeaderTimeout = 2 * time.Minute
		client.Transport = transport
		defer transport.CloseIdleConnections()
	}
	res, err := client.Do(req)
	if err != nil {
		http.Error(w, fmt.Errorf("profile node %q: %w", n.Name, err).Error(), http.StatusBadGateway)
		return
	}
	defer res.Body.Close()
	w.Header().Set("Content-Type", res.Header.Get("Content-Type"))
	w.Header().Set("X-Content-Type-Options", "nosniff")
	w.WriteHeader(res.StatusCode)
	_, _ = io.Copy(w, res.Body)
}
