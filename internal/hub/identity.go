package hub

import (
	"context"
	"fmt"
	"log"
	"net"
	"path/filepath"
	"strings"
	"time"

	"connectrpc.com/connect"

	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/fsutil"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

const (
	sleeveTokenFile   = ".protectorate-token"
	sleeveTokenPrefix = "sleeve:"
	defaultSelfURL    = "http://127.0.0.1:7470"
)

// sleeveScopes are what every sleeve's own token grants: the read-only API,
// the mailbox, the directory, leases and the backlog. Nothing else.
var sleeveScopes = []string{auth.ScopeRead, "backlog_*", "directory_*", "lease_*", "message_*"}

func sleeveTokenName(name string) string { return sleeveTokenPrefix + name }

// LocalURL turns a listener address into a URL a sleeve on the same machine
// can dial.
func LocalURL(addr net.Addr) string {
	host, port, err := net.SplitHostPort(addr.String())
	if err != nil {
		return defaultSelfURL
	}
	if ip := net.ParseIP(host); host == "" || (ip != nil && ip.IsUnspecified()) {
		host = "127.0.0.1"
	}
	return "http://" + net.JoinHostPort(host, port)
}

// senderOf names the authenticated caller for messages: a sleeve token maps
// to its sleeve, the orchestrator's token to "poe", the owner to "owner".
// Any other token is not a sleeve and is labelled by its name.
func senderOf(p *auth.Principal) string {
	switch {
	case p == nil || p.Owner:
		return "owner"
	case p.Name == sleeve.OrchestratorTokenName:
		return sleeve.OrchestratorName
	case strings.HasPrefix(p.Name, sleeveTokenPrefix):
		return strings.TrimPrefix(p.Name, sleeveTokenPrefix)
	}
	return "token:" + p.Name
}

// callerSleeve returns the sleeve name when the authenticated caller is a
// sleeve's own token.
func callerSleeve(ctx context.Context) (string, bool) {
	p := auth.PrincipalFrom(ctx)
	if p == nil || p.Owner || !strings.HasPrefix(p.Name, sleeveTokenPrefix) {
		return "", false
	}
	return strings.TrimPrefix(p.Name, sleeveTokenPrefix), true
}

// ownSleeveOnly refuses a sleeve-identity caller that reaches for another
// sleeve's chat or session. Other callers are left to their scopes.
func ownSleeveOnly(ctx context.Context, target string) error {
	if self, ok := callerSleeve(ctx); ok && self != target {
		return connect.NewError(connect.CodePermissionDenied, fmt.Errorf("sleeve %s may only read its own sleeve, not %q", self, target))
	}
	return nil
}

// issueIdentity mints the scoped token a new sleeve will carry. The secret is
// returned once; install writes it into the sleeve's home.
func (h *Hub) issueIdentity(name string) (auth.Token, string, error) {
	t, secret, err := h.authz.Store().Create(sleeveTokenName(name), sleeveScopes, 0, time.Now())
	if err != nil {
		return auth.Token{}, "", fmt.Errorf("mint token for sleeve %s: %w", name, err)
	}
	return t, secret, nil
}

// identityEnv is the environment that points the sleeve's CLI at the hub and
// its token. The path is the one the sleeve sees.
func (h *Hub) identityEnv() map[string]string {
	url := h.SelfURL
	if url == "" {
		url = defaultSelfURL
	}
	return map[string]string{
		"PROTECTORATE_HUB":        url,
		"PROTECTORATE_TOKEN_FILE": filepath.Join(h.sleeves.UserHome(), sleeveTokenFile),
	}
}

// installIdentity writes the token into the sleeve's home, never the workspace.
func (h *Hub) installIdentity(name, secret string) error {
	path := filepath.Join(h.sleeves.HomeDir(name), sleeveTokenFile)
	if err := fsutil.WriteFile(path, []byte(secret+"\n"), 0o600); err != nil {
		return fmt.Errorf("write token for sleeve %s: %w", name, err)
	}
	return nil
}

// dropIdentity revokes every live token of a sleeve except keep.
func (h *Hub) dropIdentity(name, keep string) {
	st := h.authz.Store()
	want := sleeveTokenName(name)
	for _, t := range st.List() {
		if t.Name != want || t.RevokedAt != nil || t.ID == keep {
			continue
		}
		if _, err := st.Revoke(t.ID, time.Now()); err != nil {
			log.Printf("sleeve %s: revoke token %s: %v", name, t.ID, err)
		}
	}
}
