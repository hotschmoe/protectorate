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
	"strings"
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
	// Task and MintedBy are set on an admin grant.
	Task     string `yaml:"task,omitempty"`
	MintedBy string `yaml:"minted_by,omitempty"`
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
	return s.create(Token{Name: name, Scopes: scopes}, ttl, now)
}

// create stores t, with validated scopes, under a new id and secret.
func (s *Store) create(t Token, ttl time.Duration, now time.Time) (Token, string, error) {
	id, err := randHex(4)
	if err != nil {
		return Token{}, "", err
	}
	raw, err := randHex(32)
	if err != nil {
		return Token{}, "", err
	}
	secret := SecretPrefix + raw
	t.ID, t.Hash, t.Created = id, hashSecret(secret), now.UTC()
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

// Grant is a request for an admin grant: job_* and node_admin:<Node>.
type Grant struct {
	Node, Task, MintedBy string
	TTL                  time.Duration
}

// Renew issues a new secret for an existing token's grants, including an empty set.
func (s *Store) Renew(t Token, ttl time.Duration, now time.Time) (Token, string, error) {
	if t.Name == "" {
		return Token{}, "", errors.New("name is required")
	}
	if ttl < 0 {
		return Token{}, "", errors.New("ttl must not be negative")
	}
	if len(t.Scopes) > 0 {
		scopes, err := validateScopes(t.Scopes)
		if err != nil {
			return Token{}, "", fmt.Errorf("renew token scopes: %w", err)
		}
		t.Scopes = scopes
	}
	return s.create(Token{Name: t.Name, Scopes: append([]string(nil), t.Scopes...), Task: t.Task, MintedBy: t.MintedBy}, ttl, now)
}

// MaxGrantTTL bounds an admin grant's lifetime.
const MaxGrantTTL = 24 * time.Hour

// CreateGrant issues an admin grant, named "grant:<node>". The secret is
// returned once and not retained.
func (s *Store) CreateGrant(g Grant, now time.Time) (Token, string, error) {
	switch {
	case strings.TrimSpace(g.Task) == "":
		return Token{}, "", errors.New("a grant needs a task saying what it is for")
	case len(g.Task) > 500:
		return Token{}, "", errors.New("task is longer than 500 bytes")
	case g.TTL <= 0 || g.TTL > MaxGrantTTL:
		return Token{}, "", fmt.Errorf("grant ttl must be positive and at most %s", MaxGrantTTL)
	case g.MintedBy == "":
		return Token{}, "", errors.New("a grant needs its minter")
	}
	scopes, err := GrantScopes(g.Node)
	if err != nil {
		return Token{}, "", err
	}
	return s.create(Token{Name: "grant:" + g.Node, Scopes: scopes, Task: strings.TrimSpace(g.Task), MintedBy: g.MintedBy}, g.TTL, now)
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

// UpdateScopes changes grants atomically, keeping the token's identity and secret.
func (s *Store) UpdateScopes(id string, add, remove []string, now time.Time) (Token, []string, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	idx := -1
	for i, t := range s.tokens {
		if t.ID == id {
			idx = i
			break
		}
	}
	if idx < 0 {
		for i, t := range s.tokens {
			if t.Name != id || t.RevokedAt != nil || t.Expires != nil && !now.Before(*t.Expires) {
				continue
			}
			if idx >= 0 {
				return Token{}, nil, fmt.Errorf("token name %q is ambiguous; use an ID", id)
			}
			idx = i
		}
	}
	if idx < 0 {
		return Token{}, nil, ErrUnknownToken
	}
	t := s.tokens[idx]
	if t.RevokedAt != nil {
		return Token{}, nil, ErrRevoked
	}
	if t.Expires != nil && !now.Before(*t.Expires) {
		return Token{}, nil, ErrExpired
	}
	for i, scopes := range [][]string{add, remove} {
		if len(scopes) > 0 {
			validate := ValidateScopes
			if i == 1 {
				validate = validateScopes
			}
			if _, err := validate(scopes); err != nil {
				return Token{}, nil, err
			}
		}
	}
	scopes := append([]string(nil), t.Scopes...)
	scopes = append(scopes, add...)
	var next []string
	for _, scope := range scopes {
		drop := false
		for _, removed := range remove {
			if scope == removed {
				drop = true
				break
			}
		}
		if !drop {
			next = append(next, scope)
		}
	}
	if len(next) > 0 {
		var err error
		next, err = validateScopes(next)
		if err != nil {
			return Token{}, nil, err
		}
	}
	s.tokens[idx].Scopes = next
	if err := s.writeLocked(); err != nil {
		s.tokens[idx] = t
		return Token{}, nil, err
	}
	return s.tokens[idx].clone(), t.Scopes, nil
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
