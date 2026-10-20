package sleeve

import (
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/hotschmoe/protectorate/internal/driver"
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
	State          string           `json:"state,omitempty"`
	SessionID      string           `json:"session_id,omitempty"`
	Model          string           `json:"model,omitempty"`
	Effort         string           `json:"effort,omitempty"`
	Requested      driver.Selection `json:"requested,omitempty"`
	Defaults       driver.Selection `json:"native_defaults,omitempty"`
	EffortFallback bool             `json:"effort_fallback,omitempty"`
	Provider       string           `json:"provider,omitempty"`
	LocalModel     bool             `json:"local_model,omitempty"`
	Context        uint64           `json:"context_tokens,omitempty"`
	Window         uint64           `json:"context_window,omitempty"`
	CacheExpires   time.Time        `json:"cache_expires,omitempty"`
	LastSeq        uint64           `json:"last_seq,omitempty"`
	LastActivity   time.Time        `json:"last_activity,omitempty"`
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
	previous := e.Chat
	e.Chat = &c
	if err := m.saveLocked(); err != nil {
		e.Chat = previous
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

// Handoff records the resleeve that put a sleeve on its harness. The sleeve
// layer stores it for the hub and does not interpret it.
type Handoff struct {
	From               string    `json:"from"`
	FromSession        string    `json:"from_session,omitempty"`
	To                 string    `json:"to"`
	Summarizer         string    `json:"summarizer,omitempty"`
	SummarizerModel    string    `json:"summarizer_model,omitempty"`
	SummarizerProvider string    `json:"summarizer_provider,omitempty"`
	SummarizerLocal    bool      `json:"summarizer_local,omitempty"`
	SummaryError       string    `json:"summary_error,omitempty"`
	Path               string    `json:"path"`
	Archive            string    `json:"archive,omitempty"`
	At                 time.Time `json:"at"`
}

// SetHandoff records how a sleeve got its harness.
func (m *Manager) SetHandoff(name string, h Handoff) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	e := m.sleeves[name]
	if e == nil {
		return ErrNotFound
	}
	e.Handoff = &h
	if err := m.saveLocked(); err != nil {
		return fmt.Errorf("persist sleeves: %w", err)
	}
	return nil
}

// Env returns the caller environment a sleeve was spawned with (including
// what the hub added), so a replacement harness can be started with it.
func (m *Manager) Env(name string) (map[string]string, error) {
	if _, err := m.Get(name); err != nil {
		return nil, err
	}
	return m.cfg.readEnvFile(name)
}

// ScratchWorkspace creates, if needed, the empty workspace a hub-owned
// sleeve of that name may use, inside the hub's workspaces directory.
func (m *Manager) ScratchWorkspace(name string) (string, error) {
	if err := ValidName(name); err != nil {
		return "", err
	}
	d := filepath.Join(m.cfg.workspacesDir(), name)
	if err := os.MkdirAll(d, 0o700); err != nil {
		return "", fmt.Errorf("create scratch workspace: %w", err)
	}
	return filepath.EvalSymlinks(d)
}

// StateDir is the hub's state directory the sleeves live in.
func (m *Manager) StateDir() string { return m.cfg.Home }
