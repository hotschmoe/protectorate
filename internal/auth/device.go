package auth

import (
	"context"
	"crypto/rand"
	"crypto/subtle"
	"errors"
	"fmt"
	"math/big"
	"slices"
	"strings"
	"time"
	"unicode"
)

const ScopeFull = "full"
const DeviceLifetime = 10 * time.Minute
const DeviceInterval = 5
const MaxPendingDevices = 100

var ErrDeviceCapacity = errors.New("device authorization capacity reached; try again later")
var ErrDeviceRequest = errors.New("no pending device request matches that code")

// DeviceAuth persists a request without its secret device code or token.
type DeviceAuth struct {
	CodeHash   string     `json:"code_hash"`
	UserCode   string     `json:"user_code"`
	Name       string     `json:"name"`
	Platform   string     `json:"platform"`
	IP         string     `json:"ip"`
	Expires    time.Time  `json:"expires"`
	Status     string     `json:"status"`
	Interval   int32      `json:"interval"`
	NextPoll   time.Time  `json:"next_poll"`
	ApprovedAt *time.Time `json:"approved_at,omitempty"`
	ApprovedBy string     `json:"approved_by,omitempty"`
}

func pairedToken(t Token) bool {
	if t.Task != "" || !strings.HasPrefix(t.Name, "pair: ") {
		return false
	}
	want := []string{"read", "terminal", "events", "session_*", "sleeve_*", "chat_*", "backlog_*"}
	if len(t.Scopes) != len(want) && len(t.Scopes) != len(want)+1 {
		return false
	}
	for _, scope := range want {
		if !slices.Contains(t.Scopes, scope) {
			return false
		}
	}
	return len(t.Scopes) == len(want) || slices.Contains(t.Scopes, ScopeTerminalHost)
}

func (t Token) IsDevice() bool { return t.Device }

func ValidDeviceLabel(label string, max int) bool {
	if label == "" || len(label) > max {
		return false
	}
	for _, c := range label {
		if unicode.IsControl(c) {
			return false
		}
	}
	return true
}

func validDeviceName(name string) bool {
	return ValidDeviceLabel(name, 100) && name != "envoy" && name != "orchestrator-envoy" && !strings.HasPrefix(name, "sleeve:")
}

func normalizeUserCode(code string) string {
	return strings.ToUpper(strings.ReplaceAll(strings.TrimSpace(code), "-", ""))
}

func (s *Store) sweepDevicesLocked(now time.Time) bool {
	n := 0
	for _, r := range s.requests {
		if now.Before(r.Expires) {
			s.requests[n] = r
			n++
		}
	}
	changed := n != len(s.requests)
	s.requests = s.requests[:n]
	return changed
}

func (s *Store) SweepDevices(now time.Time) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.sweepDevicesLocked(now) {
		return s.writeLocked()
	}
	return nil
}

func (s *Store) StartDevice(name, platform, ip string, now time.Time) (DeviceAuth, string, error) {
	if !validDeviceName(name) {
		return DeviceAuth{}, "", errors.New("device name must be printable, 1-100 bytes, and cannot use a reserved sleeve or orchestrator identity")
	}
	if !ValidDeviceLabel(platform, 40) {
		return DeviceAuth{}, "", errors.New("device platform must be printable and 1-40 bytes")
	}
	code, err := randHex(32)
	if err != nil {
		return DeviceAuth{}, "", fmt.Errorf("generate device code: %w", err)
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	s.sweepDevicesLocked(now)
	pending := 0
	for _, r := range s.requests {
		if r.Status == "pending" || r.Status == "approved" {
			pending++
		}
	}
	if pending >= MaxPendingDevices || len(s.requests) >= 1000 {
		return DeviceAuth{}, "", ErrDeviceCapacity
	}
	const alphabet = "23456789BCDFGHJKLMNPQRSTVWXYZ"
	var userCode string
	for {
		b := make([]byte, 8)
		for i := range b {
			n, err := rand.Int(rand.Reader, big.NewInt(int64(len(alphabet))))
			if err != nil {
				return DeviceAuth{}, "", fmt.Errorf("generate user code: %w", err)
			}
			b[i] = alphabet[n.Int64()]
		}
		userCode = string(b[:4]) + "-" + string(b[4:])
		duplicate := false
		for _, r := range s.requests {
			if subtle.ConstantTimeCompare([]byte(userCode), []byte(r.UserCode)) == 1 {
				duplicate = true
			}
		}
		if !duplicate {
			break
		}
	}
	r := DeviceAuth{CodeHash: hashSecret(code), UserCode: userCode, Name: name, Platform: platform, IP: ip, Expires: now.Add(DeviceLifetime).UTC(), Status: "pending", Interval: DeviceInterval}
	s.requests = append(s.requests, r)
	if err := s.writeLocked(); err != nil {
		s.requests = s.requests[:len(s.requests)-1]
		return DeviceAuth{}, "", fmt.Errorf("save device request: %w", err)
	}
	return r, code, nil
}

func (s *Store) PendingDevices(now time.Time) []DeviceAuth {
	s.mu.Lock()
	defer s.mu.Unlock()
	var out []DeviceAuth
	for _, r := range s.requests {
		if now.Before(r.Expires) && r.Status == "pending" {
			out = append(out, r)
		}
	}
	return out
}

func (s *Store) DecideDevice(code, actor string, approve bool, now time.Time) (DeviceAuth, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	hash := []byte(hashSecret(normalizeUserCode(code)))
	idx := -1
	for i, r := range s.requests {
		if subtle.ConstantTimeCompare(hash, []byte(hashSecret(normalizeUserCode(r.UserCode)))) == 1 {
			idx = i
		}
	}
	if idx < 0 || s.requests[idx].Status != "pending" || !now.Before(s.requests[idx].Expires) {
		return DeviceAuth{}, ErrDeviceRequest
	}
	before := s.requests[idx]
	if approve {
		at := now.UTC()
		s.requests[idx].Status = "approved"
		s.requests[idx].ApprovedAt = &at
		s.requests[idx].ApprovedBy = actor
	} else {
		s.requests[idx].Status = "denied"
	}
	if err := s.writeLocked(); err != nil {
		s.requests[idx] = before
		return DeviceAuth{}, fmt.Errorf("save device decision: %w", err)
	}
	return s.requests[idx], nil
}

func (s *Store) PollDevice(code string, now time.Time) (string, int32, Token, string, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	hash := []byte(hashSecret(code))
	idx := -1
	for i, r := range s.requests {
		if subtle.ConstantTimeCompare(hash, []byte(r.CodeHash)) == 1 {
			idx = i
		}
	}
	if idx < 0 || !now.Before(s.requests[idx].Expires) {
		return "expired", DeviceInterval, Token{}, "", nil
	}
	r := &s.requests[idx]
	if r.Status == "consumed" || r.Status == "denied" {
		return r.Status, r.Interval, Token{}, "", nil
	}
	before := *r
	if now.Before(r.NextPoll) {
		r.Interval += DeviceInterval
		if r.Interval > 300 {
			r.Interval = 300
		}
		r.NextPoll = now.Add(time.Duration(r.Interval) * time.Second)
		if err := s.writeLocked(); err != nil {
			*r = before
			return "", 0, Token{}, "", fmt.Errorf("save poll interval: %w", err)
		}
		return "slow_down", r.Interval, Token{}, "", nil
	}
	r.NextPoll = now.Add(time.Duration(r.Interval) * time.Second)
	if r.Status != "approved" {
		if err := s.writeLocked(); err != nil {
			*r = before
			return "", 0, Token{}, "", fmt.Errorf("save device poll: %w", err)
		}
		return "pending", r.Interval, Token{}, "", nil
	}
	id, err := randHex(4)
	if err != nil {
		*r = before
		return "", 0, Token{}, "", fmt.Errorf("generate token id: %w", err)
	}
	raw, err := randHex(32)
	if err != nil {
		*r = before
		return "", 0, Token{}, "", fmt.Errorf("generate token secret: %w", err)
	}
	secret := SecretPrefix + raw
	t := Token{ID: id, Name: r.Name, Hash: hashSecret(secret), Scopes: []string{ScopeFull}, Created: now.UTC(), Device: true, Platform: r.Platform, ApprovedAt: r.ApprovedAt, ApprovedBy: r.ApprovedBy}
	r.Status = "consumed"
	s.tokens = append(s.tokens, t)
	if err := s.writeLocked(); err != nil {
		*r = before
		s.tokens = s.tokens[:len(s.tokens)-1]
		return "", 0, Token{}, "", fmt.Errorf("deliver device token: %w", err)
	}
	return "approved", r.Interval, t.clone(), secret, nil
}

func (s *Store) RenameDevice(id, name string) (Token, error) {
	if !validDeviceName(name) {
		return Token{}, errors.New("device name must be printable, 1-100 bytes, and cannot use a reserved sleeve or orchestrator identity")
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	for i, t := range s.tokens {
		if t.ID != id || !t.IsDevice() {
			continue
		}
		s.tokens[i].Name = name
		if err := s.writeLocked(); err != nil {
			s.tokens[i] = t
			return Token{}, fmt.Errorf("rename device: %w", err)
		}
		return s.tokens[i].clone(), nil
	}
	return Token{}, ErrUnknownToken
}

// WatchToken cancels a stream when its token is revoked, including a revoke
// racing admission. The returned stop releases the registration.
func (s *Store) WatchToken(parent context.Context, id string) (context.Context, func()) {
	ctx, cancel := context.WithCancel(parent)
	if id == "" {
		return ctx, cancel
	}
	s.mu.Lock()
	active := false
	for _, t := range s.tokens {
		if t.ID == id && t.RevokedAt == nil {
			active = true
		}
	}
	key := new(int)
	if active {
		if s.streams == nil {
			s.streams = map[string]map[*int]context.CancelFunc{}
		}
		if s.streams[id] == nil {
			s.streams[id] = map[*int]context.CancelFunc{}
		}
		s.streams[id][key] = cancel
	} else {
		cancel()
	}
	s.mu.Unlock()
	return ctx, func() {
		cancel()
		s.mu.Lock()
		defer s.mu.Unlock()
		delete(s.streams[id], key)
		if len(s.streams[id]) == 0 {
			delete(s.streams, id)
		}
	}
}
