package protocol

import "time"

// AuthProvider represents a supported authentication provider
type AuthProvider string

const (
	AuthProviderClaude AuthProvider = "claude"
	AuthProviderGemini AuthProvider = "gemini"
	AuthProviderCodex  AuthProvider = "codex"
	AuthProviderGit    AuthProvider = "git"
)

// AuthStatus represents the authentication status for all providers
type AuthStatus struct {
	Providers map[AuthProvider]*ProviderAuthStatus `json:"providers"`
}

// ProviderAuthStatus represents the authentication status for a single provider
type ProviderAuthStatus struct {
	Authenticated bool      `json:"authenticated"`
	Method        string    `json:"method,omitempty"` // "token", "oauth", "api_key", "ssh"
	ExpiresAt     time.Time `json:"expires_at,omitempty"`
	Username      string    `json:"username,omitempty"`
}

// AuthLoginRequest is the request body for logging in to a provider
type AuthLoginRequest struct {
	Provider AuthProvider `json:"provider"`
	Token    string       `json:"token,omitempty"`
}

// AuthLoginResult is the result of a login operation
type AuthLoginResult struct {
	Success  bool   `json:"success"`
	Provider string `json:"provider"`
	Method   string `json:"method,omitempty"`
	Message  string `json:"message,omitempty"`
	Error    string `json:"error,omitempty"`
}

// AuthRevokeResult is the result of a revoke operation
type AuthRevokeResult struct {
	Success  bool   `json:"success"`
	Provider string `json:"provider"`
	Message  string `json:"message,omitempty"`
	Error    string `json:"error,omitempty"`
}

// AuthState tracks expiration info persisted to .auth-state.json
type AuthState struct {
	Version   int                          `json:"version"`
	Providers map[string]ProviderAuthState `json:"providers"`
}

// ProviderAuthState tracks sync and expiration times for a provider
type ProviderAuthState struct {
	SyncedAt  time.Time `json:"synced_at"`
	ExpiresAt time.Time `json:"expires_at,omitempty"`
	Method    string    `json:"method"`
}

// AuthCheckResult is the result of an auth check operation
type AuthCheckResult struct {
	Valid        bool                           `json:"valid"`
	ExpiringSoon bool                           `json:"expiring_soon"`
	Expired      bool                           `json:"expired"`
	Providers    map[AuthProvider]*AuthCheckInfo `json:"providers"`
}

// AuthCheckInfo provides status details for a single provider
type AuthCheckInfo struct {
	Status    string    `json:"status"` // "valid", "expiring_soon", "expired", "missing"
	ExpiresAt time.Time `json:"expires_at,omitempty"`
	ExpiresIn string    `json:"expires_in,omitempty"`
	Message   string    `json:"message,omitempty"`
}
