package envoy

import (
	"fmt"
	"io"
	"net/http"
	"strings"
	"sync"
	"time"
)

// SSEHub manages SSE client connections and broadcasts messages
type SSEHub struct {
	clients    map[*SSEClient]bool
	broadcast  chan *SSEMessage
	register   chan *SSEClient
	unregister chan *SSEClient
	mu         sync.RWMutex
}

// SSEClient represents a connected SSE client
type SSEClient struct {
	send chan *SSEMessage
}

// SSEMessage represents a server-sent event
type SSEMessage struct {
	Event string
	Data  string
}

// NewSSEHub creates a new SSE hub
func NewSSEHub() *SSEHub {
	return &SSEHub{
		clients:    make(map[*SSEClient]bool),
		broadcast:  make(chan *SSEMessage, 256),
		register:   make(chan *SSEClient),
		unregister: make(chan *SSEClient),
	}
}

// Run starts the SSE hub event loop
func (h *SSEHub) Run() {
	keepaliveTicker := time.NewTicker(15 * time.Second)
	defer keepaliveTicker.Stop()

	for {
		select {
		case client := <-h.register:
			h.mu.Lock()
			h.clients[client] = true
			h.mu.Unlock()

		case client := <-h.unregister:
			h.mu.Lock()
			if _, ok := h.clients[client]; ok {
				delete(h.clients, client)
				close(client.send)
			}
			h.mu.Unlock()

		case msg := <-h.broadcast:
			h.mu.RLock()
			for client := range h.clients {
				select {
				case client.send <- msg:
				default:
					// Client buffer full, skip this message
				}
			}
			h.mu.RUnlock()

		case <-keepaliveTicker.C:
			// Send keepalive comment to all clients
			h.mu.RLock()
			for client := range h.clients {
				select {
				case client.send <- &SSEMessage{Event: "keepalive", Data: ""}:
				default:
				}
			}
			h.mu.RUnlock()
		}
	}
}

// Broadcast sends a message to all connected clients
func (h *SSEHub) Broadcast(event, data string) {
	h.broadcast <- &SSEMessage{Event: event, Data: data}
}

// ClientCount returns the number of connected clients
func (h *SSEHub) ClientCount() int {
	h.mu.RLock()
	defer h.mu.RUnlock()
	return len(h.clients)
}

// handleSSE handles SSE connections from clients
func (s *Server) handleSSE(w http.ResponseWriter, r *http.Request) {
	// Set SSE headers
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "keep-alive")
	w.Header().Set("Access-Control-Allow-Origin", "*")
	w.Header().Set("X-Accel-Buffering", "no")

	// Check for SSE support
	flusher, ok := w.(http.Flusher)
	if !ok {
		http.Error(w, "SSE not supported", http.StatusInternalServerError)
		return
	}

	// Disable write deadline for SSE (long-lived connection)
	rc := http.NewResponseController(w)
	rc.SetWriteDeadline(time.Time{})

	// Create client
	client := &SSEClient{
		send: make(chan *SSEMessage, 64),
	}

	// Register client
	s.sseHub.register <- client

	// Cleanup on disconnect
	defer func() {
		s.sseHub.unregister <- client
	}()

	// Send immediate connected event so client knows SSE is working
	fmt.Fprintf(w, "event: connected\n")
	fmt.Fprintf(w, "data: {}\n\n")
	flusher.Flush()

	// Request initial state from broadcaster
	s.broadcaster.RequestInit()

	// Send events to client
	for {
		select {
		case msg, ok := <-client.send:
			if !ok {
				return
			}

			if msg.Event == "keepalive" {
				// SSE comment format for keepalive
				fmt.Fprintf(w, ": keepalive\n\n")
			} else {
				fmt.Fprintf(w, "event: %s\n", msg.Event)
				// SSE requires each line of data to be prefixed with "data: "
				writeSSEData(w, msg.Data)
			}
			flusher.Flush()

		case <-r.Context().Done():
			return
		}
	}
}

// writeSSEData writes data in proper SSE format, handling multi-line content
func writeSSEData(w io.Writer, data string) {
	lines := strings.Split(data, "\n")
	for _, line := range lines {
		fmt.Fprintf(w, "data: %s\n", line)
	}
	fmt.Fprintf(w, "\n")
}
