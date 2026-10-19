package auth

import (
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/hex"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"

	"gopkg.in/yaml.v3"

	"github.com/hotschmoe/protectorate/internal/fsutil"
)

// SecretPrefix starts every issued token so leaked secrets are recognizable.
const SecretPrefix = "ptk_"

const lastUsedFlush = time.Minute

var (
	ErrUnknownToken = errors.New("unknown token")
	ErrExpired      = errors.New("token expired")
	ErrRevoked      = errors.New("token revoked")
)

// Token is one issued token. Only the SHA-256 hash of the secret is kept.
type Token struct {
	ID        string     `yaml:"id"`
	Name      string     `yaml:"name"`
	Hash      string     `yaml:"hash"`
	Scopes    []string   `yaml:"scopes"`
	Created   time.Time  `yaml:"created"`
	Expires   *time.Time `yaml:"expires,omitempty"`
	LastUsed  *time.Time `yaml:"last_used,omitempty"`
	RevokedAt *time.Time `yaml:"revoked,omitempty"`
}

type tokenFile struct {
	Tokens []Token `yaml:"tokens"`
}

// Store holds issued tokens in a YAML file, rewritten atomically with mode
// 0600. The hub process is its only writer. An empty path keeps tokens in
// memory.
type Store struct {
	path string

	mu        sync.Mutex
	tokens    []Token
	lastFlush time.Time
}

// OpenStore loads path, treating a missing file as empty.
func OpenStore(path string) (*Store, error) {
	s := &Store{path: path}
	b, err := os.ReadFile(path)
	switch {
	case errors.Is(err, os.ErrNotExist):
		return s, nil
	case err != nil:
		return nil, fmt.Errorf("read tokens: %w", err)
	}
	var f tokenFile
	if err := yaml.Unmarshal(b, &f); err != nil {
		return nil, fmt.Errorf("parse %s: %w", path, err)
	}
	s.tokens = f.Tokens
	return s, nil
}

// NewMemoryStore returns a store that never touches disk.
func NewMemoryStore() *Store { return &Store{} }

func hashSecret(secret string) string {
	sum := sha256.Sum256([]byte(secret))
	return hex.EncodeToString(sum[:])
}

func randHex(n int) (string, error) {
	b := make([]byte, n)
	if _, err := rand.Read(b); err != nil {
		return "", fmt.Errorf("random: %w", err)
	}
	return hex.EncodeToString(b), nil
}

// Create issues a token. ttl zero means it never expires. The secret is
// returned once and not retained.
func (s *Store) Create(name string, scopes []string, ttl time.Duration, now time.Time) (Token, string, error) {
	if name == "" {
		return Token{}, "", errors.New("name is required")
	}
	if ttl < 0 {
		return Token{}, "", errors.New("ttl must not be negative")
	}
	scopes, err := ValidateScopes(scopes)
	if err != nil {
		return Token{}, "", err
	}
	id, err := randHex(4)
	if err != nil {
		return Token{}, "", err
	}
	raw, err := randHex(32)
	if err != nil {
		return Token{}, "", err
	}
	secret := SecretPrefix + raw
	t := Token{ID: id, Name: name, Hash: hashSecret(secret), Scopes: scopes, Created: now.UTC()}
	if ttl > 0 {
		exp := now.Add(ttl).UTC()
		t.Expires = &exp
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	s.tokens = append(s.tokens, t)
	if err := s.writeLocked(); err != nil {
		s.tokens = s.tokens[:len(s.tokens)-1]
		return Token{}, "", err
	}
	return t, secret, nil
}

// Lookup authenticates a secret. Every stored hash is compared in constant
// time so the response time does not depend on which token matched.
func (s *Store) Lookup(secret string, now time.Time) (Token, error) {
	got := []byte(hashSecret(secret))
	s.mu.Lock()
	defer s.mu.Unlock()
	idx := -1
	for i := range s.tokens {
		if subtle.ConstantTimeCompare(got, []byte(s.tokens[i].Hash)) == 1 {
			idx = i
		}
	}
	if idx < 0 {
		return Token{}, ErrUnknownToken
	}
	t := s.tokens[idx]
	if t.RevokedAt != nil {
		return Token{}, ErrRevoked
	}
	if t.Expires != nil && !now.Before(*t.Expires) {
		return Token{}, ErrExpired
	}
	used := now.UTC()
	s.tokens[idx].LastUsed = &used
	if now.Sub(s.lastFlush) >= lastUsedFlush {
		if s.writeLocked() == nil {
			s.lastFlush = now
		}
	}
	return s.tokens[idx].clone(), nil
}

// Revoke disables a token. Revoking twice keeps the first revocation time.
func (s *Store) Revoke(id string, now time.Time) (Token, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	for i := range s.tokens {
		if s.tokens[i].ID != id {
			continue
		}
		if s.tokens[i].RevokedAt != nil {
			return s.tokens[i].clone(), nil
		}
		at := now.UTC()
		s.tokens[i].RevokedAt = &at
		if err := s.writeLocked(); err != nil {
			s.tokens[i].RevokedAt = nil
			return Token{}, err
		}
		return s.tokens[i].clone(), nil
	}
	return Token{}, ErrUnknownToken
}

// List returns all tokens in creation order.
func (s *Store) List() []Token {
	s.mu.Lock()
	defer s.mu.Unlock()
	out := make([]Token, len(s.tokens))
	for i, t := range s.tokens {
		out[i] = t.clone()
	}
	return out
}

// Flush persists pending last_used updates.
func (s *Store) Flush() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.writeLocked()
}

func (t Token) clone() Token {
	t.Scopes = append([]string(nil), t.Scopes...)
	return t
}

func (s *Store) writeLocked() error {
	if s.path == "" {
		return nil
	}
	b, err := yaml.Marshal(tokenFile{Tokens: s.tokens})
	if err != nil {
		return fmt.Errorf("encode tokens: %w", err)
	}
	dir := filepath.Dir(s.path)
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return fmt.Errorf("create %s: %w", dir, err)
	}
	if err := fsutil.WriteFile(s.path, b, 0o600); err != nil {
		return fmt.Errorf("write tokens: %w", err)
	}
	return nil
}
