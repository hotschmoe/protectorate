package sidecar

import (
	"os"
	"path/filepath"
	"time"
)

// AuthInfo contains credential file status
type AuthInfo struct {
	ClaudeAuth bool `json:"claude_auth"`
	GeminiAuth bool `json:"gemini_auth"`
}

// AuthChecker checks credential files with caching
type AuthChecker struct {
	cache *CachedValue[*AuthInfo]
}

// NewAuthChecker creates a new auth checker
func NewAuthChecker() *AuthChecker {
	a := &AuthChecker{}
	a.cache = NewCachedValue(30*time.Second, a.check)
	return a
}

// Status returns auth status, using cache if fresh
func (a *AuthChecker) Status() *AuthInfo {
	return a.cache.Get()
}

func (a *AuthChecker) check() *AuthInfo {
	homeDir, err := os.UserHomeDir()
	if err != nil {
		return &AuthInfo{}
	}

	return &AuthInfo{
		ClaudeAuth: fileExists(filepath.Join(homeDir, ".creds", "claude", "credentials.json")),
		GeminiAuth: fileExists(filepath.Join(homeDir, ".creds", "gemini", "credentials.json")),
	}
}

func fileExists(path string) bool {
	info, err := os.Stat(path)
	return err == nil && !info.IsDir()
}
