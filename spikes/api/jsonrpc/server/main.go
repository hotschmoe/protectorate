package main

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"sync"
	"time"
)

type Sleeve struct {
	ID      string `json:"id"`
	Name    string `json:"name"`
	Harness string `json:"harness"`
	State   string `json:"state"`
}

type ListParams struct{}

type ListResult struct {
	Sleeves []Sleeve `json:"sleeves"`
}

type SpawnParams struct {
	Name    string `json:"name" desc:"Human-readable sleeve name, unique per hub."`
	Harness string `json:"harness,omitempty" desc:"Agent harness to run: claude, gemini, opencode."`
}

type SpawnResult struct {
	Sleeve Sleeve `json:"sleeve"`
}

type Event struct {
	Seq      int64  `json:"seq"`
	Type     string `json:"type"`
	SleeveID string `json:"sleeve_id,omitempty"`
	Message  string `json:"message,omitempty"`
}

type hub struct {
	mu      sync.Mutex
	sleeves []Sleeve
	subs    map[chan Event]string
	seq     int64
}

func (h *hub) publish(typ, sleeveID, msg string) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.seq++
	for ch, filter := range h.subs {
		if filter != "" && filter != sleeveID {
			continue
		}
		select {
		case ch <- Event{Seq: h.seq, Type: typ, SleeveID: sleeveID, Message: msg}:
		default:
		}
	}
}

func (h *hub) list(_ context.Context, _ ListParams) (ListResult, error) {
	h.mu.Lock()
	defer h.mu.Unlock()
	return ListResult{Sleeves: append([]Sleeve{}, h.sleeves...)}, nil
}

func (h *hub) spawn(_ context.Context, p SpawnParams) (SpawnResult, error) {
	if p.Name == "" {
		return SpawnResult{}, &rpcError{codeInvalidParams, "name is required"}
	}
	h.mu.Lock()
	for _, s := range h.sleeves {
		if s.Name == p.Name {
			h.mu.Unlock()
			return SpawnResult{}, &rpcError{codeAlreadyExists, fmt.Sprintf("sleeve %q exists", p.Name)}
		}
	}
	s := Sleeve{ID: fmt.Sprintf("sl-%d", len(h.sleeves)+1), Name: p.Name, Harness: p.Harness, State: "running"}
	h.sleeves = append(h.sleeves, s)
	h.mu.Unlock()
	h.publish("sleeve.spawned", s.ID, s.Name)
	return SpawnResult{Sleeve: s}, nil
}

func (h *hub) events(w http.ResponseWriter, r *http.Request) {
	ch := make(chan Event, 16)
	h.mu.Lock()
	h.subs[ch] = r.URL.Query().Get("sleeve_id")
	h.mu.Unlock()
	defer func() {
		h.mu.Lock()
		delete(h.subs, ch)
		h.mu.Unlock()
	}()
	w.Header().Set("Content-Type", "application/x-ndjson")
	fl := w.(http.Flusher)
	enc := json.NewEncoder(w)
	fl.Flush()
	tick := time.NewTicker(time.Second)
	defer tick.Stop()
	for {
		select {
		case <-r.Context().Done():
			return
		case e := <-ch:
			enc.Encode(e)
		case <-tick.C:
			enc.Encode(Event{Type: "heartbeat"})
		}
		fl.Flush()
	}
}

func main() {
	h := &hub{subs: map[chan Event]string{}}
	var reg registry
	register(&reg, "sleeve.list", "List returns all sleeves.", h.list)
	register(&reg, "sleeve.spawn", "Spawn creates a sleeve running the given harness.", h.spawn)
	mux := http.NewServeMux()
	mux.HandleFunc("POST /rpc", reg.handler)
	mux.HandleFunc("GET /events", h.events)
	mux.HandleFunc("GET /mcp/tools", func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		enc := json.NewEncoder(w)
		enc.SetIndent("", "  ")
		enc.Encode(map[string]any{"tools": reg.tools()})
	})
	log.Fatal(http.ListenAndServe("127.0.0.1:7472", mux))
}
