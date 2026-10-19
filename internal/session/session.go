// Package session wraps the tmux CLI on a dedicated socket. Harnesses run
// untouched inside tmux sessions; nothing here injects input.
package session

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os/exec"
	"regexp"
	"strconv"
	"strings"
	"time"
)

const DefaultSocket = "protectorate"

const commandOption = "@protectorate_command"

var (
	ErrNotFound    = errors.New("session not found")
	ErrExists      = errors.New("session already exists")
	ErrInvalidName = errors.New("invalid session name (want 1-64 of A-Z a-z 0-9 _ -)")
)

var nameRE = regexp.MustCompile(`^[A-Za-z0-9_-]{1,64}$`)

type Session struct {
	Name            string
	Cwd             string
	Command         []string
	Created         time.Time
	AttachedClients int
	LastActivity    time.Time
	Cols, Rows      int
}

// Manager drives one tmux server, selected by socket name (tmux -L).
type Manager struct {
	Socket string
	Bin    string
}

func New(socket string) *Manager {
	if socket == "" {
		socket = DefaultSocket
	}
	return &Manager{Socket: socket, Bin: "tmux"}
}

func ValidName(name string) error {
	if !nameRE.MatchString(name) {
		return ErrInvalidName
	}
	return nil
}

func (m *Manager) cmd(ctx context.Context, args ...string) *exec.Cmd {
	return exec.CommandContext(ctx, m.Bin, append([]string{"-L", m.Socket}, args...)...)
}

func (m *Manager) run(ctx context.Context, args ...string) (string, error) {
	var stdout, stderr bytes.Buffer
	c := m.cmd(ctx, args...)
	c.Stdout, c.Stderr = &stdout, &stderr
	if err := c.Run(); err != nil {
		return "", classify(strings.TrimSpace(stderr.String()), err)
	}
	return stdout.String(), nil
}

type noServerError struct{}

func (noServerError) Error() string { return "no tmux server running" }

func classify(stderr string, err error) error {
	switch {
	case strings.Contains(stderr, "no server running"),
		strings.Contains(stderr, "error connecting to"):
		return noServerError{}
	case strings.Contains(stderr, "can't find session"),
		strings.Contains(stderr, "no such session"):
		return ErrNotFound
	case strings.Contains(stderr, "duplicate session"):
		return ErrExists
	}
	if stderr == "" {
		return err
	}
	return fmt.Errorf("tmux: %s: %w", stderr, err)
}

func target(name string) string { return "=" + name + ":" }

const listFormat = "#{session_name}\t#{session_created}\t#{session_attached}\t#{session_activity}\t#{window_width}\t#{window_height}\t#{pane_current_path}\t#{" + commandOption + "}"

func (m *Manager) List(ctx context.Context) ([]Session, error) {
	out, err := m.run(ctx, "list-sessions", "-F", listFormat)
	if errors.As(err, new(noServerError)) {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("list sessions: %w", err)
	}
	return parseList(out)
}

func parseList(out string) ([]Session, error) {
	var res []Session
	for _, line := range strings.Split(strings.TrimRight(out, "\n"), "\n") {
		if line == "" {
			continue
		}
		f := strings.SplitN(line, "\t", 8)
		if len(f) != 8 {
			return nil, fmt.Errorf("parse tmux line %q: want 8 fields", line)
		}
		var nums [4]int64
		for i, s := range f[2:6] {
			n, err := strconv.ParseInt(s, 10, 64)
			if err != nil {
				return nil, fmt.Errorf("parse tmux line %q: %w", line, err)
			}
			nums[i] = n
		}
		created, err := strconv.ParseInt(f[1], 10, 64)
		if err != nil {
			return nil, fmt.Errorf("parse tmux line %q: %w", line, err)
		}
		s := Session{
			Name:            f[0],
			Created:         time.Unix(created, 0),
			AttachedClients: int(nums[0]),
			LastActivity:    time.Unix(nums[1], 0),
			Cols:            int(nums[2]),
			Rows:            int(nums[3]),
			Cwd:             f[6],
		}
		if f[7] != "" {
			if err := json.Unmarshal([]byte(f[7]), &s.Command); err != nil {
				s.Command = nil
			}
		}
		res = append(res, s)
	}
	return res, nil
}

func (m *Manager) Get(ctx context.Context, name string) (Session, error) {
	all, err := m.List(ctx)
	if err != nil {
		return Session{}, err
	}
	for _, s := range all {
		if s.Name == name {
			return s, nil
		}
	}
	return Session{}, ErrNotFound
}

type CreateOptions struct {
	Name    string
	Cwd     string
	Command []string
	Env     map[string]string
}

func (m *Manager) Create(ctx context.Context, o CreateOptions) (Session, error) {
	if err := ValidName(o.Name); err != nil {
		return Session{}, err
	}
	args := []string{"new-session", "-d", "-s", o.Name}
	if o.Cwd != "" {
		args = append(args, "-c", o.Cwd)
	}
	for k, v := range o.Env {
		if k == "" || strings.Contains(k, "=") {
			return Session{}, fmt.Errorf("invalid env name %q", k)
		}
		args = append(args, "-e", k+"="+v)
	}
	if len(o.Command) > 0 {
		args = append(args, "--")
		args = append(args, o.Command...)
	}
	if _, err := m.run(ctx, args...); err != nil {
		return Session{}, fmt.Errorf("create session %q: %w", o.Name, err)
	}
	if len(o.Command) > 0 {
		raw, _ := json.Marshal(o.Command)
		if _, err := m.run(ctx, "set-option", "-t", target(o.Name), commandOption, string(raw)); err != nil {
			return Session{}, fmt.Errorf("record command for %q: %w", o.Name, err)
		}
	}
	return m.Get(ctx, o.Name)
}

func (m *Manager) Kill(ctx context.Context, name string) error {
	if err := ValidName(name); err != nil {
		return err
	}
	_, err := m.run(ctx, "kill-session", "-t", "="+name)
	if errors.As(err, new(noServerError)) {
		return ErrNotFound
	}
	if err != nil {
		return fmt.Errorf("kill session %q: %w", name, err)
	}
	return nil
}

// Capture returns the visible pane plus lines of scrollback, with ANSI
// escapes preserved and wrapped lines joined.
func (m *Manager) Capture(ctx context.Context, name string, lines int) (string, error) {
	if err := ValidName(name); err != nil {
		return "", err
	}
	if lines < 0 {
		lines = 0
	}
	out, err := m.run(ctx, "capture-pane", "-p", "-e", "-J", "-S", strconv.Itoa(-lines), "-t", target(name))
	if errors.As(err, new(noServerError)) {
		return "", ErrNotFound
	}
	if err != nil {
		return "", fmt.Errorf("capture %q: %w", name, err)
	}
	return out, nil
}

// AttachCmd builds the tmux client command; the caller supplies a PTY.
func (m *Manager) AttachCmd(ctx context.Context, name string, readOnly bool) (*exec.Cmd, error) {
	if err := ValidName(name); err != nil {
		return nil, err
	}
	if _, err := m.Get(ctx, name); err != nil {
		return nil, err
	}
	args := []string{"attach-session", "-t", "=" + name}
	if readOnly {
		args = append(args, "-r")
	}
	c := exec.Command(m.Bin, append([]string{"-L", m.Socket}, args...)...)
	c.Env = append(c.Environ(), "TERM=xterm-256color")
	return c, nil
}
