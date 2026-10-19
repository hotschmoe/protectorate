package tailnet

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strconv"
	"strings"
	"time"
)

// Admin talks to headscale's REST API on this machine using an API key
// created through headscale's local unix socket at setup.
type Admin struct {
	home string
	cfg  Config
	hc   *http.Client
	base string

	key    string
	userID uint64
}

// NewAdmin builds a client that always dials headscale's listen address
// directly, so it works with TLS and without DNS or hairpin routing.
func NewAdmin(home string, cfg Config) *Admin {
	u, _ := url.Parse(cfg.ServerURL)
	scheme := "http"
	var tr http.Transport
	dial := loopback(cfg.Listen)
	tr.DialContext = func(ctx context.Context, network, _ string) (net.Conn, error) {
		var d net.Dialer
		return d.DialContext(ctx, network, dial)
	}
	if cfg.TLSCert != "" {
		scheme = "https"
	}
	return &Admin{home: home, cfg: cfg, base: scheme + "://" + u.Host,
		hc: &http.Client{Transport: &tr, Timeout: 30 * time.Second}}
}

func apiKeyPath(home string) string { return filepath.Join(Dir(home), "apikey") }

// WaitReady polls headscale's /health until it answers or ctx ends.
func (a *Admin) WaitReady(ctx context.Context) error {
	for {
		req, err := http.NewRequestWithContext(ctx, http.MethodGet, a.base+"/health", nil)
		if err != nil {
			return err
		}
		if res, err := a.hc.Do(req); err == nil {
			res.Body.Close()
			if res.StatusCode == http.StatusOK {
				return nil
			}
		}
		select {
		case <-ctx.Done():
			return fmt.Errorf("headscale not ready: %w", ctx.Err())
		case <-time.After(200 * time.Millisecond):
		}
	}
}

// Ensure loads or creates the API key and the policy user. It needs a running
// headscale.
func (a *Admin) Ensure(ctx context.Context) error {
	if b, err := os.ReadFile(apiKeyPath(a.home)); err == nil {
		a.key = strings.TrimSpace(string(b))
	}
	if a.key == "" || !a.keyWorks(ctx) {
		if err := a.createAPIKey(ctx); err != nil {
			return err
		}
	}
	return a.ensureUser(ctx)
}

func (a *Admin) keyWorks(ctx context.Context) bool {
	err := a.call(ctx, http.MethodGet, "/api/v1/user", nil, nil)
	return err == nil
}

func (a *Admin) createAPIKey(ctx context.Context) error {
	bin := BinaryPath(a.home, HeadscaleVersion)
	cfgPath := filepath.Join(Dir(a.home), "headscale.yaml")
	out, err := exec.CommandContext(ctx, bin, "-c", cfgPath, "apikeys", "create", "--expiration", "365d").Output()
	if err != nil {
		var ee *exec.ExitError
		if errors.As(err, &ee) {
			err = fmt.Errorf("%w: %s", err, bytes.TrimSpace(ee.Stderr))
		}
		return fmt.Errorf("create headscale API key: %w", err)
	}
	lines := strings.Fields(string(out))
	if len(lines) == 0 {
		return errors.New("create headscale API key: empty output")
	}
	a.key = strings.Trim(lines[len(lines)-1], `"`)
	return writeFile(apiKeyPath(a.home), []byte(a.key+"\n"), 0o600)
}

type apiError struct {
	Status int
	Body   string
}

func (e *apiError) Error() string { return fmt.Sprintf("headscale API: %d %s", e.Status, e.Body) }

func (a *Admin) call(ctx context.Context, method, path string, in, out any) error {
	var body io.Reader
	if in != nil {
		b, err := json.Marshal(in)
		if err != nil {
			return err
		}
		body = bytes.NewReader(b)
	}
	req, err := http.NewRequestWithContext(ctx, method, a.base+path, body)
	if err != nil {
		return err
	}
	req.Header.Set("Authorization", "Bearer "+a.key)
	if in != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	res, err := a.hc.Do(req)
	if err != nil {
		return fmt.Errorf("headscale API %s %s (is `hub serve` running?): %w", method, path, err)
	}
	defer res.Body.Close()
	raw, err := io.ReadAll(io.LimitReader(res.Body, 8<<20))
	if err != nil {
		return err
	}
	if res.StatusCode != http.StatusOK {
		return &apiError{Status: res.StatusCode, Body: strings.TrimSpace(string(raw))}
	}
	if out == nil {
		return nil
	}
	return json.Unmarshal(raw, out)
}

type apiUser struct {
	ID   uint64 `json:"id,string"`
	Name string `json:"name"`
}

func (a *Admin) ensureUser(ctx context.Context) error {
	var list struct {
		Users []apiUser `json:"users"`
	}
	if err := a.call(ctx, http.MethodGet, "/api/v1/user?name="+policyUser, nil, &list); err != nil {
		return err
	}
	for _, u := range list.Users {
		if u.Name == policyUser {
			a.userID = u.ID
			return nil
		}
	}
	var created struct {
		User apiUser `json:"user"`
	}
	if err := a.call(ctx, http.MethodPost, "/api/v1/user", map[string]string{"name": policyUser}, &created); err != nil {
		return err
	}
	a.userID = created.User.ID
	return nil
}

// CreateKey issues a single-use preauth key that enrolls a node with tags.
func (a *Admin) CreateKey(ctx context.Context, tags []string, ttl time.Duration) (string, error) {
	req := map[string]any{
		"user":       strconv.FormatUint(a.userID, 10),
		"reusable":   false,
		"ephemeral":  false,
		"expiration": time.Now().Add(ttl).UTC().Format(time.RFC3339),
		"aclTags":    tags,
	}
	var res struct {
		PreAuthKey struct {
			Key string `json:"key"`
		} `json:"preAuthKey"`
	}
	if err := a.call(ctx, http.MethodPost, "/api/v1/preauthkey", req, &res); err != nil {
		return "", fmt.Errorf("create preauth key: %w", err)
	}
	if res.PreAuthKey.Key == "" {
		return "", errors.New("create preauth key: empty key in response")
	}
	return res.PreAuthKey.Key, nil
}

// Node is a machine registered with headscale.
type Node struct {
	ID        uint64   `json:"id,string"`
	Name      string   `json:"name"`
	GivenName string   `json:"givenName"`
	Tags      []string `json:"tags"`
	Online    bool     `json:"online"`
	Expiry    string   `json:"expiry"`
}

func (a *Admin) Nodes(ctx context.Context) ([]Node, error) {
	var res struct {
		Nodes []Node `json:"nodes"`
	}
	if err := a.call(ctx, http.MethodGet, "/api/v1/node", nil, &res); err != nil {
		return nil, err
	}
	return res.Nodes, nil
}

func (a *Admin) find(ctx context.Context, name string) (Node, error) {
	nodes, err := a.Nodes(ctx)
	if err != nil {
		return Node{}, err
	}
	for _, n := range nodes {
		if n.GivenName == name || n.Name == name {
			return n, nil
		}
	}
	return Node{}, fmt.Errorf("no node named %q", name)
}

// Revoke expires and deletes the node so peers drop it from their netmap.
func (a *Admin) Revoke(ctx context.Context, name string) error {
	n, err := a.find(ctx, name)
	if err != nil {
		return err
	}
	id := strconv.FormatUint(n.ID, 10)
	if err := a.call(ctx, http.MethodPost, "/api/v1/node/"+id+"/expire", map[string]string{}, nil); err != nil {
		return fmt.Errorf("expire node %s: %w", name, err)
	}
	if err := a.call(ctx, http.MethodDelete, "/api/v1/node/"+id, nil, nil); err != nil {
		return fmt.Errorf("delete node %s: %w", name, err)
	}
	return nil
}

// SanitizeLabel lowercases s into a DNS label, replacing other runes with '-'.
func SanitizeLabel(s string) string {
	var b strings.Builder
	for _, r := range strings.ToLower(s) {
		if r >= 'a' && r <= 'z' || r >= '0' && r <= '9' || r == '-' {
			b.WriteRune(r)
		} else {
			b.WriteByte('-')
		}
	}
	out := strings.Trim(b.String(), "-")
	if len(out) > 63 {
		out = strings.Trim(out[:63], "-")
	}
	return out
}

// Invite creates a single-use key for a machine called name, refusing names
// already registered.
func (a *Admin) Invite(ctx context.Context, name string, tags []string, ttl time.Duration) (string, error) {
	if !validLabel.MatchString(name) {
		return "", fmt.Errorf("name %q must be a lowercase DNS label (a-z, 0-9, -)", name)
	}
	if ttl <= 0 {
		return "", errors.New("expiry must be positive")
	}
	nodes, err := a.Nodes(ctx)
	if err != nil {
		return "", err
	}
	for _, n := range nodes {
		if n.GivenName == name || n.Name == name || name == a.cfg.HubName {
			return "", fmt.Errorf("name %q is already in use (revoke it first)", name)
		}
	}
	return a.CreateKey(ctx, tags, ttl)
}

// RegisterClient approves a device that signed in through the stock app and
// tags it tag:client.
func (a *Admin) RegisterClient(ctx context.Context, regKey string) error {
	q := url.Values{"user": {policyUser}, "key": {regKey}}
	var res struct {
		Node Node `json:"node"`
	}
	if err := a.call(ctx, http.MethodPost, "/api/v1/node/register?"+q.Encode(), nil, &res); err != nil {
		return fmt.Errorf("register device: %w", err)
	}
	id := strconv.FormatUint(res.Node.ID, 10)
	if err := a.call(ctx, http.MethodPost, "/api/v1/node/"+id+"/tags", map[string][]string{"tags": {TagClient}}, nil); err != nil {
		return fmt.Errorf("tag device %s: %w", res.Node.GivenName, err)
	}
	return nil
}

func (a *Admin) setExpiry(ctx context.Context, id uint64, at time.Time) error {
	q := url.Values{"expiry": {at.UTC().Format(time.RFC3339)}}
	return a.call(ctx, http.MethodPost, "/api/v1/node/"+strconv.FormatUint(id, 10)+"/expire?"+q.Encode(), nil, nil)
}

// expires reports whether policy puts a lifetime on n. The hub itself never
// expires.
func expires(n Node) bool {
	return slices.Contains(n.Tags, TagNode) || slices.Contains(n.Tags, TagClient)
}

// EnforceExpiry gives every enrolled node and client without an expiry the
// configured lifetime. Headscale leaves tagged nodes unexpiring.
func (a *Admin) EnforceExpiry(ctx context.Context) error {
	nodes, err := a.Nodes(ctx)
	if err != nil {
		return err
	}
	at := time.Now().AddDate(0, 0, a.cfg.KeyExpiryDays)
	var errs []error
	for _, n := range nodes {
		if expires(n) && n.Expiry == "" {
			if err := a.setExpiry(ctx, n.ID, at); err != nil {
				errs = append(errs, fmt.Errorf("set expiry of %s: %w", n.GivenName, err))
			}
		}
	}
	return errors.Join(errs...)
}

// Renew restarts name's key lifetime from now.
func (a *Admin) Renew(ctx context.Context, name string) (time.Time, error) {
	n, err := a.find(ctx, name)
	if err != nil {
		return time.Time{}, err
	}
	if !expires(n) {
		return time.Time{}, fmt.Errorf("%s has no expiry to renew", name)
	}
	at := time.Now().AddDate(0, 0, a.cfg.KeyExpiryDays)
	return at, a.setExpiry(ctx, n.ID, at)
}
