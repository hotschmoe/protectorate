package auth

import (
	"errors"
	"fmt"
	"path"
	"regexp"
	"sort"
	"strings"
)

const (
	ScopeRead             = "read"
	ScopeEvents           = "events"
	ScopeTerminal         = "terminal"
	ScopeTerminalReadonly = "terminal:readonly"
	ScopeTerminalHost     = "terminal:host"
	// ScopeNodeAdminPrefix starts node_admin:<glob>, a path.Match glob over
	// node names: work on care nodes it matches, and admin jobs there.
	ScopeNodeAdminPrefix = "node_admin:"

	eventService = "EventService"
)

var nodeGlobRe = regexp.MustCompile(`^[a-z0-9.*?\[\]_-]{1,63}$`)

// nodeAdminGlob returns the node glob of a node_admin scope.
func nodeAdminGlob(s string) (string, bool) {
	g, ok := strings.CutPrefix(s, ScopeNodeAdminPrefix)
	return g, ok
}

func validNodeAdmin(s string) error {
	g, _ := nodeAdminGlob(s)
	if !nodeGlobRe.MatchString(g) {
		return fmt.Errorf("bad scope %q: want node_admin:<node or glob over node names>", s)
	}
	if _, err := path.Match(g, ""); err != nil {
		return fmt.Errorf("bad scope %q: %w", s, err)
	}
	return nil
}

// GrantScopes are the scopes of an admin grant for node (a node name or glob).
func GrantScopes(node string) ([]string, error) {
	scope := ScopeNodeAdminPrefix + node
	if err := validNodeAdmin(scope); err != nil {
		return nil, err
	}
	return validateScopes([]string{"job_*", scope})
}

// AdminOn reports whether scopes hold a node_admin grant matching node.
func AdminOn(scopes []string, node string) bool {
	for _, s := range scopes {
		if g, ok := nodeAdminGlob(s); ok {
			if m, _ := path.Match(g, node); m {
				return true
			}
		}
	}
	return false
}

var specialScopes = map[string]bool{
	ScopeFull: true,
	ScopeRead: true, ScopeEvents: true, ScopeTerminal: true, ScopeTerminalReadonly: true, ScopeTerminalHost: true,
}

// ValidateScopes normalizes scopes (sorted, deduplicated) and rejects an empty
// list, malformed globs and globs that match no grantable tool, which are
// almost always typos that would otherwise silently grant nothing. node_admin
// scopes are refused: they exist only on grants (Store.CreateGrant).
func ValidateScopes(scopes []string) ([]string, error) {
	for _, s := range scopes {
		if s == ScopeFull {
			return nil, errors.New("Full access is issued through device approval")
		}
		if _, ok := nodeAdminGlob(s); ok {
			return nil, fmt.Errorf("scope %q is only issued per task: protectorate grant admin --node <node> --task ...", s)
		}
	}
	return validateScopes(scopes)
}

func validateScopes(scopes []string) ([]string, error) {
	if len(scopes) == 0 {
		return nil, errors.New("at least one scope is required")
	}
	names, err := ToolNames()
	if err != nil {
		return nil, err
	}
	seen := map[string]bool{}
	var out []string
	for _, s := range scopes {
		if s == "" {
			return nil, errors.New("empty scope")
		}
		if seen[s] {
			continue
		}
		seen[s] = true
		if _, ok := nodeAdminGlob(s); ok {
			if err := validNodeAdmin(s); err != nil {
				return nil, err
			}
		} else if !specialScopes[s] {
			if _, err := path.Match(s, ""); err != nil {
				return nil, fmt.Errorf("bad scope %q: %w", s, err)
			}
			if !matchesAny(s, names) {
				return nil, fmt.Errorf("scope %q matches no tool; scopes are read, events, terminal, terminal:readonly, terminal:host, node_admin:<node> or globs over tool names such as session_*", s)
			}
		}
		out = append(out, s)
	}
	sort.Strings(out)
	return out, nil
}

func matchesAny(glob string, names []string) bool {
	for _, n := range names {
		if ok, _ := path.Match(glob, n); ok {
			return true
		}
	}
	return false
}

// needed names the scope that would admit m, for error messages.
func needed(m Method) string {
	if m.ReadOnly {
		return ScopeRead + " or " + m.Tool
	}
	if m.Service == eventService {
		return ScopeEvents + " or " + m.Tool
	}
	return m.Tool
}

func scopesAllow(scopes []string, m Method) bool {
	if m.OwnerOnly {
		return false
	}
	for _, s := range scopes {
		switch s {
		case ScopeFull:
			return true
		case ScopeRead:
			if m.ReadOnly {
				return true
			}
		case ScopeEvents:
			if m.Service == eventService {
				return true
			}
		case ScopeTerminal, ScopeTerminalReadonly, ScopeTerminalHost:
		default:
			if _, ok := nodeAdminGlob(s); ok {
				continue
			}
			if ok, _ := path.Match(s, m.Tool); ok {
				return true
			}
		}
	}
	return false
}

func scopesAllowTerminal(scopes []string, readOnly bool) bool {
	for _, s := range scopes {
		if s == ScopeFull || s == ScopeTerminal || s == ScopeTerminalHost || (readOnly && s == ScopeTerminalReadonly) {
			return true
		}
	}
	return false
}

// Grants reports whether a token holding scopes may call the named tool, with
// the same rules the hub applies. An unknown or owner-only tool is never
// granted.
func Grants(scopes []string, tool string) bool {
	ms, err := Methods()
	if err != nil {
		return false
	}
	for _, m := range ms {
		if m.Tool == tool {
			return scopesAllow(scopes, m)
		}
	}
	return false
}
