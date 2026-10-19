package sleeve

import (
	"fmt"
	"path/filepath"
	"time"
)

// Mode is how a sleeve runs its harness. In chat mode the hub drives the
// harness's headless protocol inside the sandbox (no tmux session); in
// terminal mode the TUI runs in a tmux session.
type Mode string

const (
	ModeChat     Mode = "chat"
	ModeTerminal Mode = "terminal"
)

// Chat is the persisted chat summary of a sleeve. State holds the hub's
// chat state name; the sleeve layer does not interpret it.
type Chat struct {
	State        string    `yaml:"state,omitempty"`
	SessionID    string    `yaml:"session_id,omitempty"`
	Model        string    `yaml:"model,omitempty"`
	Provider     string    `yaml:"provider,omitempty"`
	LocalModel   bool      `yaml:"local_model,omitempty"`
	Context      uint64    `yaml:"context_tokens,omitempty"`
	Window       uint64    `yaml:"context_window,omitempty"`
	CacheExpires time.Time `yaml:"cache_expires,omitempty"`
	LastSeq      uint64    `yaml:"last_seq,omitempty"`
	LastActivity time.Time `yaml:"last_activity,omitempty"`
}

func resolveMode(h Harness, m Mode, args []string) (Mode, error) {
	switch {
	case m == "" && h == Shell:
		return ModeTerminal, nil
	case m == "":
		m = ModeChat
	case m != ModeChat && m != ModeTerminal:
		return "", fmt.Errorf("%w: unknown mode %q", ErrInvalid, m)
	}
	if m == ModeChat && h == Shell {
		return "", fmt.Errorf("%w: a shell sleeve has no chat mode", ErrInvalid)
	}
	if m == ModeChat && len(args) > 0 {
		return "", fmt.Errorf("%w: args apply to terminal mode; a chat sleeve runs the harness's headless protocol", ErrInvalid)
	}
	return m, nil
}

// terminalArgs builds the TUI arguments for h. A Codex TUI would otherwise
// install and run a managed daemon inside every sleeve.
func terminalArgs(h Harness, extra []string) []string {
	if h == Codex {
		return append([]string{"--disable", "daemon_auto_start"}, extra...)
	}
	return extra
}

// SetChat updates the persisted chat summary of a sleeve and returns it.
func (m *Manager) SetChat(name string, f func(*Chat)) (Info, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	e := m.sleeves[name]
	if e == nil {
		return Info{}, ErrNotFound
	}
	// copy on write: readers hold Info values that share the pointer
	c := Chat{}
	if e.Chat != nil {
		c = *e.Chat
	}
	f(&c)
	e.Chat = &c
	if err := m.saveLocked(); err != nil {
		return e.Info, fmt.Errorf("persist sleeves: %w", err)
	}
	return e.Info, nil
}

// ChatLogPath is where the hub keeps the sleeve's chat event log. It lives in
// the sleeve's state directory, outside the sandbox.
func (m *Manager) ChatLogPath(name string) string {
	return filepath.Join(m.cfg.sleeveDir(name), "chat.jsonl")
}

// HomeDir is the host path of the sleeve's home directory, bound at UserHome
// inside the sandbox. Files the hub leaves there are visible to the harness
// but never land in the workspace.
func (m *Manager) HomeDir(name string) string { return m.cfg.sleeveHome(name) }

// UserHome is the path the sleeve's home has inside the sandbox.
func (m *Manager) UserHome() string { return m.cfg.UserHome }
