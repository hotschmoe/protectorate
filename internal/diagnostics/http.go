// Package diagnostics exposes authenticated runtime diagnostics on an existing listener.
package diagnostics

import (
	"encoding/json"
	"net/http"
	"net/http/pprof"
	"runtime/metrics"
)

// Handler requires authorization before dispatching even unknown debug paths.
func Handler(authorize func(*http.Request) error) http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /debug/pprof/", pprof.Index)
	mux.HandleFunc("GET /debug/pprof/heap", pprof.Handler("heap").ServeHTTP)
	mux.HandleFunc("GET /debug/pprof/goroutine", pprof.Handler("goroutine").ServeHTTP)
	mux.HandleFunc("GET /debug/pprof/profile", pprof.Profile)
	mux.HandleFunc("GET /debug/metrics", func(w http.ResponseWriter, r *http.Request) {
		samples := []metrics.Sample{
			{Name: "/memory/classes/total:bytes"},
			{Name: "/memory/classes/heap/released:bytes"},
			{Name: "/memory/classes/heap/objects:bytes"},
			{Name: "/memory/classes/heap/stacks:bytes"},
			{Name: "/gc/heap/live:bytes"},
			{Name: "/gc/heap/goal:bytes"},
			{Name: "/gc/gomemlimit:bytes"},
			{Name: "/gc/cycles/total:gc-cycles"},
			{Name: "/sched/goroutines:goroutines"},
		}
		metrics.Read(samples)
		values := make(map[string]uint64, len(samples))
		for _, s := range samples {
			if s.Value.Kind() == metrics.KindUint64 {
				values[s.Name] = s.Value.Uint64()
			}
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(values)
	})
	return Protect(authorize, mux)
}

// Protect applies the same access and cache policy to remote diagnostics.
func Protect(authorize func(*http.Request) error, next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Cache-Control", "no-store")
		if authorize == nil || authorize(r) != nil {
			http.Error(w, "forbidden", http.StatusForbidden)
			return
		}
		next.ServeHTTP(w, r)
	})
}
