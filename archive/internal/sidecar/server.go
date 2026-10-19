package sidecar

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"time"
)

// Config holds sidecar configuration from environment
type Config struct {
	Port          int
	SleeveName    string
	WorkspacePath string
}

// Server is the sidecar HTTP server
type Server struct {
	cfg     *Config
	http    *http.Server
	dhf     *DHFDetector
	cstack  *CstackChecker
	process *ProcessStats
	auth    *AuthChecker
}

// StatusResponse is the response for GET /status
type StatusResponse struct {
	SleeveName string          `json:"sleeve_name"`
	DHF        *DHFInfo        `json:"dhf"`
	Workspace  *WorkspaceInfo  `json:"workspace"`
	Process    *ProcessInfo    `json:"process"`
	Auth       *AuthInfo       `json:"auth"`
}

// WorkspaceInfo contains workspace and cstack information
type WorkspaceInfo struct {
	Path   string       `json:"path"`
	Cstack *CstackStats `json:"cstack,omitempty"`
}

// NewServer creates a new sidecar server
func NewServer(cfg *Config) *Server {
	s := &Server{
		cfg:     cfg,
		dhf:     NewDHFDetector(),
		cstack:  NewCstackChecker(cfg.WorkspacePath),
		process: NewProcessStats(),
		auth:    NewAuthChecker(),
	}

	mux := http.NewServeMux()
	mux.HandleFunc("/health", s.handleHealth)
	mux.HandleFunc("/status", s.handleStatus)

	s.http = &http.Server{
		Addr:         fmt.Sprintf(":%d", cfg.Port),
		Handler:      mux,
		ReadTimeout:  10 * time.Second,
		WriteTimeout: 10 * time.Second,
	}

	return s
}

// Start begins listening for requests
func (s *Server) Start() error {
	return s.http.ListenAndServe()
}

// Shutdown gracefully stops the server
func (s *Server) Shutdown(ctx context.Context) error {
	return s.http.Shutdown(ctx)
}

func (s *Server) handleHealth(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]string{"status": "ok"})
}

func (s *Server) handleStatus(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	resp := StatusResponse{
		SleeveName: s.cfg.SleeveName,
		DHF:        s.dhf.Info(),
		Workspace: &WorkspaceInfo{
			Path:   s.cfg.WorkspacePath,
			Cstack: s.cstack.Stats(),
		},
		Process: s.process.Info(),
		Auth:    s.auth.Status(),
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(resp)
}
