package hub

import (
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// Home returns the state directory, honoring PROTECTORATE_HOME.
func Home() (string, error) {
	if h := os.Getenv("PROTECTORATE_HOME"); h != "" {
		return h, nil
	}
	u, err := os.UserHomeDir()
	if err != nil {
		return "", fmt.Errorf("locate home dir: %w", err)
	}
	return filepath.Join(u, ".protectorate"), nil
}

func tokenPath() (string, error) {
	h, err := Home()
	if err != nil {
		return "", err
	}
	return filepath.Join(h, "token"), nil
}

// ReadToken loads the existing hub token.
func ReadToken() (string, error) {
	p, err := tokenPath()
	if err != nil {
		return "", err
	}
	b, err := os.ReadFile(p)
	if err != nil {
		return "", fmt.Errorf("read token (has `hub serve` run on this machine?): %w", err)
	}
	return strings.TrimSpace(string(b)), nil
}

// EnsureToken returns the hub token, creating it on first use.
func EnsureToken() (string, error) {
	p, err := tokenPath()
	if err != nil {
		return "", err
	}
	if err := os.MkdirAll(filepath.Dir(p), 0o700); err != nil {
		return "", fmt.Errorf("create %s: %w", filepath.Dir(p), err)
	}
	raw := make([]byte, 32)
	if _, err := rand.Read(raw); err != nil {
		return "", fmt.Errorf("generate token: %w", err)
	}
	tok := hex.EncodeToString(raw)
	f, err := os.OpenFile(p, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o600)
	if errors.Is(err, os.ErrExist) {
		return ReadToken()
	}
	if err != nil {
		return "", fmt.Errorf("create token: %w", err)
	}
	defer f.Close()
	if _, err := f.WriteString(tok + "\n"); err != nil {
		return "", fmt.Errorf("write token: %w", err)
	}
	return tok, nil
}
