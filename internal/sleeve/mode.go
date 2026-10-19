package sleeve

import (
	"context"
	"crypto/rand"
	"errors"
	"fmt"
	"log"
	"slices"
	"strings"
	"time"

	"github.com/hotschmoe/protectorate/internal/session"
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

// tuiArgs builds the TUI arguments for h. A known session id resumes it with
// the harness's native resume. A new Claude TUI gets a session id of our own
// so a later switch to chat can resume it; the returned id is the one in use.
func tuiArgs(h Harness, extra []string, sessionID string) ([]string, string, error) {
	switch h {
	case Shell:
		return extra, "", nil
	case Claude:
		if sessionID != "" {
			return append(slices.Clone(extra), "--resume", sessionID), sessionID, nil
		}
		for _, a := range extra {
			switch a {
			case "--resume", "-r", "--continue", "-c", "--session-id":
				return extra, "", nil
			}
		}
		var b [16]byte
		if _, err := rand.Read(b[:]); err != nil {
			return nil, "", fmt.Errorf("session id: %w", err)
		}
		b[6], b[8] = b[6]&0x0f|0x40, b[8]&0x3f|0x80
		id := fmt.Sprintf("%x-%x-%x-%x-%x", b[0:4], b[4:6], b[6:8], b[8:10], b[10:])
		return append(slices.Clone(extra), "--session-id", id), id, nil
	case Codex:
		// The TUI would otherwise install and run a managed daemon per sleeve.
		args := append([]string{"--disable", "daemon_auto_start"}, extra...)
		if sessionID != "" {
			args = append(args, "resume", sessionID)
		}
		return args, sessionID, nil
	case Grok:
		if sessionID != "" {
			return append(slices.Clone(extra), "--resume", sessionID), sessionID, nil
		}
	case Pi:
		if sessionID != "" {
			return append(slices.Clone(extra), "--session", sessionID), sessionID, nil
		}
	}
	return extra, sessionID, nil
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

// SwitchMode moves a sleeve between chat and terminal. The caller has closed
// the chat driver before switching to terminal, and opens it after switching
// to chat, so only one owner holds the native session at a time.
func (m *Manager) SwitchMode(ctx context.Context, name string, to Mode) (Info, error) {
	if to != ModeChat && to != ModeTerminal {
		return Info{}, fmt.Errorf("%w: unknown mode %q", ErrInvalid, to)
	}
	if err := m.reserve(ctx, name); err != nil {
		return Info{}, err
	}
	defer m.release(name)
	m.mu.Lock()
	e := m.sleeves[name]
	var info Info
	if e != nil {
		info = e.Info
	}
	m.mu.Unlock()
	switch {
	case e == nil:
		return Info{}, ErrNotFound
	case info.Harness == Shell:
		return Info{}, fmt.Errorf("%w: a shell sleeve has no chat mode", ErrInvalid)
	case info.Mode == to && info.State == Running:
		return info, nil
	}
	if info.Session != "" {
		if err := m.opts.Sessions.Kill(ctx, info.Session); err != nil && !errors.Is(err, session.ErrNotFound) {
			return Info{}, fmt.Errorf("kill session: %w", err)
		}
	}
	if err := m.cfg.killScope(ctx, name); err != nil {
		return Info{}, err
	}
	var sessionID string
	if info.Chat != nil {
		sessionID = info.Chat.SessionID
	}
	if to == ModeTerminal {
		args, id, err := tuiArgs(info.Harness, info.Args, sessionID)
		if err != nil {
			return Info{}, err
		}
		launch, err := m.cfg.launch(info, args)
		if err != nil {
			return Info{}, err
		}
		info.Session, info.Mode, info.State = SessionName(name), ModeTerminal, Running
		info.Exited, info.ExitCode, info.ExitSignal, info.OOMKilled = time.Time{}, nil, 0, false
		if err := m.start(ctx, launch, info); err != nil {
			return Info{}, err
		}
		if id != "" && info.Chat != nil && info.Chat.SessionID == "" {
			c := *info.Chat
			c.SessionID = id
			info.Chat = &c
		}
	} else {
		info.Session, info.Mode, info.State = "", ModeChat, Running
	}
	m.mu.Lock()
	if cur := m.sleeves[name]; cur == e {
		e.Info = info
		e.cpuAt = time.Time{}
		if err := m.saveLocked(); err != nil {
			log.Printf("sleeve %s: persist mode: %v", name, err)
		}
		info = e.Info
	}
	m.mu.Unlock()
	m.opts.Emit("sleeve.mode", name, map[string]any{"mode": strings.ToLower(string(to))})
	return info, nil
}
