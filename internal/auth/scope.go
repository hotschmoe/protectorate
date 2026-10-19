package auth

import (
	"errors"
	"fmt"
	"path"
	"sort"
)

const (
	ScopeRead             = "read"
	ScopeEvents           = "events"
	ScopeTerminal         = "terminal"
	ScopeTerminalReadonly = "terminal:readonly"

	eventService = "EventService"
)

var specialScopes = map[string]bool{
	ScopeRead: true, ScopeEvents: true, ScopeTerminal: true, ScopeTerminalReadonly: true,
}

// ValidateScopes normalizes scopes (sorted, deduplicated) and rejects an empty
// list, malformed globs and globs that match no grantable tool, which are
// almost always typos that would otherwise silently grant nothing.
func ValidateScopes(scopes []string) ([]string, error) {
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
		if !specialScopes[s] {
			if _, err := path.Match(s, ""); err != nil {
				return nil, fmt.Errorf("bad scope %q: %w", s, err)
			}
			if !matchesAny(s, names) {
				return nil, fmt.Errorf("scope %q matches no tool; scopes are read, events, terminal, terminal:readonly or globs over tool names such as session_*", s)
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
		case ScopeRead:
			if m.ReadOnly {
				return true
			}
		case ScopeEvents:
			if m.Service == eventService {
				return true
			}
		case ScopeTerminal, ScopeTerminalReadonly:
		default:
			if ok, _ := path.Match(s, m.Tool); ok {
				return true
			}
		}
	}
	return false
}

func scopesAllowTerminal(scopes []string, readOnly bool) bool {
	for _, s := range scopes {
		if s == ScopeTerminal || (readOnly && s == ScopeTerminalReadonly) {
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
