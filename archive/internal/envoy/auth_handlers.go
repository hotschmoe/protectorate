package envoy

import (
	"encoding/json"
	"net/http"
	"strings"

	"github.com/hotschmoe/protectorate/internal/protocol"
)

func (s *Server) handleAuthStatus(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	status := s.auth.GetStatus()
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(status)
}

func (s *Server) handleAuthCheck(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	result := s.auth.Check()
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(result)
}

func (s *Server) handleAuthProvider(w http.ResponseWriter, r *http.Request) {
	path := strings.TrimPrefix(r.URL.Path, "/api/auth/")
	parts := strings.Split(path, "/")

	if len(parts) == 0 || parts[0] == "" {
		http.Error(w, "provider required", http.StatusBadRequest)
		return
	}

	providerStr := parts[0]
	provider := protocol.AuthProvider(providerStr)

	switch {
	case len(parts) == 2 && parts[1] == "login":
		s.handleAuthLogin(w, r, provider)
	case len(parts) == 1:
		switch r.Method {
		case http.MethodGet:
			s.handleAuthProviderStatus(w, r, provider)
		case http.MethodDelete:
			s.handleAuthRevoke(w, r, provider)
		default:
			http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		}
	default:
		http.Error(w, "invalid path", http.StatusBadRequest)
	}
}

func (s *Server) handleAuthProviderStatus(w http.ResponseWriter, r *http.Request, provider protocol.AuthProvider) {
	status := s.auth.GetStatus()
	providerStatus, ok := status.Providers[provider]
	if !ok {
		http.Error(w, "unknown provider", http.StatusNotFound)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(providerStatus)
}

func (s *Server) handleAuthLogin(w http.ResponseWriter, r *http.Request, provider protocol.AuthProvider) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	var req protocol.AuthLoginRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		req = protocol.AuthLoginRequest{}
	}

	result, err := s.auth.Login(provider, req.Token)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	if !result.Success {
		w.WriteHeader(http.StatusBadRequest)
	}
	json.NewEncoder(w).Encode(result)
}

func (s *Server) handleAuthRevoke(w http.ResponseWriter, r *http.Request, provider protocol.AuthProvider) {
	result, err := s.auth.Revoke(provider)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(result)
}

func (s *Server) handleAuthSync(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	var req struct {
		Provider string `json:"provider"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		req.Provider = "all"
	}

	result, err := s.auth.SyncFromCLI(req.Provider)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(result)
}
