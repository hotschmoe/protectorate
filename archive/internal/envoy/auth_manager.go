package envoy

import (
	"encoding/json"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"sync"
	"time"

	"github.com/hotschmoe/protectorate/internal/protocol"
)

const (
	credsBasePath = "/home/agent/.creds"
	authStatePath = "/home/agent/.creds/.auth-state.json"
	claudeExpiry  = 365 * 24 * time.Hour // 1 year
	warnThreshold = 24 * time.Hour
)

// AuthManager handles credential storage and authentication state
type AuthManager struct {
	mu    sync.RWMutex
	state *protocol.AuthState
}

// NewAuthManager creates a new auth manager
func NewAuthManager() *AuthManager {
	am := &AuthManager{
		state: &protocol.AuthState{
			Version:   1,
			Providers: make(map[string]protocol.ProviderAuthState),
		},
	}
	return am
}

// LoadState reads auth state from disk
func (m *AuthManager) LoadState() error {
	m.mu.Lock()
	defer m.mu.Unlock()

	data, err := os.ReadFile(authStatePath)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return fmt.Errorf("failed to read auth state: %w", err)
	}

	var state protocol.AuthState
	if err := json.Unmarshal(data, &state); err != nil {
		return fmt.Errorf("failed to parse auth state: %w", err)
	}

	m.state = &state
	return nil
}

// SaveState writes auth state to disk
func (m *AuthManager) SaveState() error {
	m.mu.Lock()
	defer m.mu.Unlock()

	return m.saveStateLocked()
}

func (m *AuthManager) saveStateLocked() error {
	if err := os.MkdirAll(filepath.Dir(authStatePath), 0700); err != nil {
		return fmt.Errorf("failed to create state directory: %w", err)
	}

	data, err := json.MarshalIndent(m.state, "", "  ")
	if err != nil {
		return fmt.Errorf("failed to marshal auth state: %w", err)
	}

	if err := os.WriteFile(authStatePath, data, 0600); err != nil {
		return fmt.Errorf("failed to write auth state: %w", err)
	}

	return nil
}

// RecordSync updates state after a successful sync
func (m *AuthManager) RecordSync(provider, method string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	return m.recordSyncLocked(provider, method)
}

// recordSyncLocked updates state without acquiring lock (caller must hold lock)
func (m *AuthManager) recordSyncLocked(provider, method string) error {
	now := time.Now()
	providerState := protocol.ProviderAuthState{
		SyncedAt: now,
		Method:   method,
	}

	// Set expiration based on provider
	switch protocol.AuthProvider(provider) {
	case protocol.AuthProviderClaude:
		providerState.ExpiresAt = now.Add(claudeExpiry)
	// Gemini, Git use API keys/SSH - no expiration
	// Codex uses auto-refresh OAuth - no warning needed
	}

	m.state.Providers[provider] = providerState
	return m.saveStateLocked()
}

// Check validates all providers and returns expiration status
func (m *AuthManager) Check() *protocol.AuthCheckResult {
	m.mu.RLock()
	defer m.mu.RUnlock()

	result := &protocol.AuthCheckResult{
		Valid:     true,
		Providers: make(map[protocol.AuthProvider]*protocol.AuthCheckInfo),
	}

	status := m.getStatusLocked()
	now := time.Now()

	for provider, providerStatus := range status.Providers {
		info := m.checkProvider(provider, providerStatus, now)
		// Only Claude is required - other providers are optional
		isRequired := provider == protocol.AuthProviderClaude
		if isRequired && (info.Status == "missing" || info.Status == "expired") {
			result.Valid = false
			result.Expired = true
		} else if isRequired && info.Status == "expiring_soon" {
			result.ExpiringSoon = true
		}
		result.Providers[provider] = info
	}

	return result
}

func (m *AuthManager) checkProvider(provider protocol.AuthProvider, providerStatus *protocol.ProviderAuthStatus, now time.Time) *protocol.AuthCheckInfo {
	if !providerStatus.Authenticated {
		return &protocol.AuthCheckInfo{Status: "missing", Message: "not authenticated"}
	}

	stateInfo, hasState := m.state.Providers[string(provider)]
	if !hasState || stateInfo.ExpiresAt.IsZero() {
		return &protocol.AuthCheckInfo{Status: "valid", Message: "authenticated"}
	}

	timeLeft := stateInfo.ExpiresAt.Sub(now)
	info := &protocol.AuthCheckInfo{ExpiresAt: stateInfo.ExpiresAt}

	if timeLeft <= 0 {
		info.Status = "expired"
		info.Message = "credentials have expired"
	} else if timeLeft <= warnThreshold {
		info.Status = "expiring_soon"
		info.ExpiresIn = formatDuration(timeLeft)
		info.Message = fmt.Sprintf("expires in %s", info.ExpiresIn)
	} else {
		info.Status = "valid"
		info.ExpiresIn = formatDuration(timeLeft)
		info.Message = fmt.Sprintf("expires in %s", info.ExpiresIn)
	}

	return info
}

func formatDuration(d time.Duration) string {
	switch {
	case d < time.Hour:
		return fmt.Sprintf("%d minutes", int(d.Minutes()))
	case d < 24*time.Hour:
		return fmt.Sprintf("%d hours", int(d.Hours()))
	default:
		days := int(d.Hours() / 24)
		if days == 1 {
			return "1 day"
		}
		return fmt.Sprintf("%d days", days)
	}
}


// StartupCheck performs a non-blocking startup validation and logs warnings
func (m *AuthManager) StartupCheck() {
	go func() {
		result := m.Check()
		if result.Expired {
			log.Printf("WARNING: Authentication tokens expired - run 'envoy auth check' for details")
		} else if result.ExpiringSoon {
			log.Printf("WARNING: Authentication tokens expiring soon - run 'envoy auth check' for details")
		}
	}()
}

// GetStatus returns the authentication status for all providers
func (m *AuthManager) GetStatus() *protocol.AuthStatus {
	m.mu.RLock()
	defer m.mu.RUnlock()

	return m.getStatusLocked()
}

// getStatusLocked returns status without acquiring lock (caller must hold lock)
func (m *AuthManager) getStatusLocked() *protocol.AuthStatus {
	status := &protocol.AuthStatus{
		Providers: make(map[protocol.AuthProvider]*protocol.ProviderAuthStatus),
	}

	status.Providers[protocol.AuthProviderClaude] = m.getClaudeStatus()
	status.Providers[protocol.AuthProviderGemini] = m.getGeminiStatus()
	status.Providers[protocol.AuthProviderCodex] = m.getCodexStatus()
	status.Providers[protocol.AuthProviderGit] = m.getGitStatus()

	return status
}

func (m *AuthManager) getClaudeStatus() *protocol.ProviderAuthStatus {
	// Check for synced credentials (.credentials.json with dot prefix - Claude's format)
	credPath := filepath.Join(credsBasePath, "claude", ".credentials.json")
	if _, err := os.Stat(credPath); err == nil {
		return &protocol.ProviderAuthStatus{
			Authenticated: true,
			Method:        "oauth",
		}
	}

	// Fallback: check old format (credentials.json without dot)
	credPath = filepath.Join(credsBasePath, "claude", "credentials.json")
	if _, err := os.Stat(credPath); err != nil {
		return &protocol.ProviderAuthStatus{Authenticated: false}
	}

	data, err := os.ReadFile(credPath)
	if err != nil {
		return &protocol.ProviderAuthStatus{Authenticated: false}
	}

	var creds struct {
		AccessToken string `json:"accessToken"`
	}
	if err := json.Unmarshal(data, &creds); err != nil {
		return &protocol.ProviderAuthStatus{Authenticated: false}
	}

	return &protocol.ProviderAuthStatus{
		Authenticated: creds.AccessToken != "",
		Method:        "token",
	}
}

func (m *AuthManager) getGeminiStatus() *protocol.ProviderAuthStatus {
	if os.Getenv("GEMINI_API_KEY") != "" {
		return &protocol.ProviderAuthStatus{
			Authenticated: true,
			Method:        "api_key",
		}
	}

	credPath := filepath.Join(credsBasePath, "gemini", "credentials.json")
	if _, err := os.Stat(credPath); err == nil {
		return &protocol.ProviderAuthStatus{
			Authenticated: true,
			Method:        "oauth",
		}
	}

	return &protocol.ProviderAuthStatus{Authenticated: false}
}

func (m *AuthManager) getCodexStatus() *protocol.ProviderAuthStatus {
	if os.Getenv("OPENAI_API_KEY") != "" {
		return &protocol.ProviderAuthStatus{
			Authenticated: true,
			Method:        "api_key",
		}
	}

	credPath := filepath.Join(credsBasePath, "codex", "auth.json")
	if _, err := os.Stat(credPath); err == nil {
		return &protocol.ProviderAuthStatus{
			Authenticated: true,
			Method:        "oauth",
		}
	}

	return &protocol.ProviderAuthStatus{Authenticated: false}
}

func (m *AuthManager) getGitStatus() *protocol.ProviderAuthStatus {
	sshKeyPath := filepath.Join(credsBasePath, "git", "id_ed25519")
	if _, err := os.Stat(sshKeyPath); err == nil {
		return &protocol.ProviderAuthStatus{
			Authenticated: true,
			Method:        "ssh",
		}
	}

	sshKeyPath = filepath.Join(credsBasePath, "git", "id_rsa")
	if _, err := os.Stat(sshKeyPath); err == nil {
		return &protocol.ProviderAuthStatus{
			Authenticated: true,
			Method:        "ssh",
		}
	}

	return &protocol.ProviderAuthStatus{Authenticated: false}
}

// Login stores credentials for a provider
func (m *AuthManager) Login(provider protocol.AuthProvider, token string) (*protocol.AuthLoginResult, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	switch provider {
	case protocol.AuthProviderClaude:
		return m.loginClaude(token)
	case protocol.AuthProviderGemini:
		return m.loginGemini(token)
	case protocol.AuthProviderCodex:
		return m.loginCodex(token)
	default:
		return nil, fmt.Errorf("unsupported provider: %s", provider)
	}
}

// storeCredentials is a helper that stores credentials for a provider
func (m *AuthManager) storeCredentials(provider protocol.AuthProvider, subdir, filename, tokenKey, method, missingMsg string, token string) (*protocol.AuthLoginResult, error) {
	if token == "" {
		return &protocol.AuthLoginResult{
			Success:  false,
			Provider: string(provider),
			Error:    missingMsg,
		}, nil
	}

	credDir := filepath.Join(credsBasePath, subdir)
	if err := os.MkdirAll(credDir, 0700); err != nil {
		return nil, fmt.Errorf("failed to create credentials directory: %w", err)
	}

	creds := map[string]string{tokenKey: token}
	data, err := json.MarshalIndent(creds, "", "  ")
	if err != nil {
		return nil, fmt.Errorf("failed to marshal credentials: %w", err)
	}

	credPath := filepath.Join(credDir, filename)
	if err := os.WriteFile(credPath, data, 0600); err != nil {
		return nil, fmt.Errorf("failed to write credentials: %w", err)
	}

	return &protocol.AuthLoginResult{
		Success:  true,
		Provider: string(provider),
		Method:   method,
		Message:  "credentials stored successfully",
	}, nil
}

func (m *AuthManager) loginClaude(token string) (*protocol.AuthLoginResult, error) {
	return m.storeCredentials(
		protocol.AuthProviderClaude,
		"claude",
		"credentials.json",
		"accessToken",
		"token",
		"token required for Claude authentication",
		token,
	)
}

func (m *AuthManager) loginGemini(token string) (*protocol.AuthLoginResult, error) {
	return m.storeCredentials(
		protocol.AuthProviderGemini,
		"gemini",
		"credentials.json",
		"api_key",
		"api_key",
		"API key required for Gemini authentication",
		token,
	)
}

func (m *AuthManager) loginCodex(token string) (*protocol.AuthLoginResult, error) {
	return m.storeCredentials(
		protocol.AuthProviderCodex,
		"codex",
		"auth.json",
		"api_key",
		"api_key",
		"API key required for Codex authentication",
		token,
	)
}

// Revoke removes credentials for a provider
func (m *AuthManager) Revoke(provider protocol.AuthProvider) (*protocol.AuthRevokeResult, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	var credDir string
	switch provider {
	case protocol.AuthProviderClaude:
		credDir = filepath.Join(credsBasePath, "claude")
	case protocol.AuthProviderGemini:
		credDir = filepath.Join(credsBasePath, "gemini")
	case protocol.AuthProviderCodex:
		credDir = filepath.Join(credsBasePath, "codex")
	case protocol.AuthProviderGit:
		credDir = filepath.Join(credsBasePath, "git")
	default:
		return nil, fmt.Errorf("unsupported provider: %s", provider)
	}

	if _, err := os.Stat(credDir); os.IsNotExist(err) {
		return &protocol.AuthRevokeResult{
			Success:  true,
			Provider: string(provider),
			Message:  "no credentials found",
		}, nil
	}

	entries, err := os.ReadDir(credDir)
	if err != nil {
		return nil, fmt.Errorf("failed to read credentials directory: %w", err)
	}

	for _, entry := range entries {
		if !entry.IsDir() {
			path := filepath.Join(credDir, entry.Name())
			if err := os.Remove(path); err != nil {
				return nil, fmt.Errorf("failed to remove %s: %w", path, err)
			}
		}
	}

	return &protocol.AuthRevokeResult{
		Success:  true,
		Provider: string(provider),
		Message:  "credentials revoked successfully",
	}, nil
}

// SyncFromCLI copies credentials from CLI tool locations to the shared volume
func (m *AuthManager) SyncFromCLI(provider string) (map[string]interface{}, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	result := map[string]interface{}{
		"synced": []string{},
		"errors": []string{},
	}

	synced := []string{}
	errors := []string{}

	if provider == "all" || provider == "claude" {
		if err := m.syncClaude(); err != nil {
			errors = append(errors, fmt.Sprintf("claude: %v", err))
		} else {
			synced = append(synced, "claude")
		}
	}

	result["synced"] = synced
	result["errors"] = errors

	if len(synced) > 0 {
		result["message"] = fmt.Sprintf("Synced: %v", synced)
	} else if len(errors) > 0 {
		result["message"] = fmt.Sprintf("Sync failed: %v", errors)
	} else {
		result["message"] = "Nothing to sync"
	}

	return result, nil
}

func (m *AuthManager) syncClaude() error {
	homeDir := "/home/agent"

	// Source locations (where Claude CLI stores credentials)
	claudeCredsPath := filepath.Join(homeDir, ".claude", ".credentials.json")
	claudeSettingsPath := filepath.Join(homeDir, ".claude.json")

	// Destination in volume
	destDir := filepath.Join(credsBasePath, "claude")
	if err := os.MkdirAll(destDir, 0700); err != nil {
		return fmt.Errorf("failed to create destination directory: %w", err)
	}

	// Copy credentials
	if _, err := os.Stat(claudeCredsPath); err == nil {
		data, err := os.ReadFile(claudeCredsPath)
		if err != nil {
			return fmt.Errorf("failed to read credentials: %w", err)
		}
		destPath := filepath.Join(destDir, ".credentials.json")
		if err := os.WriteFile(destPath, data, 0600); err != nil {
			return fmt.Errorf("failed to write credentials: %w", err)
		}
	} else {
		return fmt.Errorf("no Claude credentials found at %s", claudeCredsPath)
	}

	// Copy settings
	if _, err := os.Stat(claudeSettingsPath); err == nil {
		data, err := os.ReadFile(claudeSettingsPath)
		if err != nil {
			return fmt.Errorf("failed to read settings: %w", err)
		}
		destPath := filepath.Join(destDir, "settings.json")
		if err := os.WriteFile(destPath, data, 0600); err != nil {
			return fmt.Errorf("failed to write settings: %w", err)
		}
	}

	// Record sync with expiration tracking
	if err := m.recordSyncLocked("claude", "oauth"); err != nil {
		log.Printf("Warning: failed to record sync state: %v", err)
	}

	return nil
}
