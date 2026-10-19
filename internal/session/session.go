// Package session wraps the tmux CLI on a dedicated socket. Harnesses run
// untouched inside tmux sessions; nothing here injects input.
package session

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"os/exec"
	"regexp"
	"slices"
	"strconv"
	"strings"
	"syscall"
	"time"
)

const DefaultSocket = "protectorate"

// The command is stored base64-encoded: tmux 3.4 backslash-escapes "$" in
// option values, which breaks JSON.
const commandOption = "@protectorate_command"

var (
	ErrNotFound    = errors.New("session not found")
	ErrExists      = errors.New("session already exists")
	ErrInvalidName = errors.New("invalid session name (want 1-64 of A-Z a-z 0-9 _ -)")
	ErrNotExited   = errors.New("session is still running")
)

var nameRE = regexp.MustCompile(`^[A-Za-z0-9_-]{1,64}$`)

type State string

const (
	Running State = "running"
	Exited  State = "exited"
)

// deadMarker starts the line tmux draws at the bottom of a dead pane.
const deadMarker = "Pane is dead ("

type Session struct {
	Name            string
	Cwd             string
	Command         []string
	Created         time.Time
	AttachedClients int
	LastActivity    time.Time
	Cols, Rows      int

	State State
	// ExitCode is the command's exit status, or 128+signal when a signal
	// ended it. Only meaningful when State is Exited.
	ExitCode   int
	ExitSignal int
	Exited     time.Time

	// unreaped marks a dead pane whose exit status tmux has not recorded yet.
	unreaped bool
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

// MaxCaptureLines caps the scrollback Capture reads; larger requests are
// clamped to it.
const MaxCaptureLines = 10000

// maxOutputBytes bounds the stdout run buffers from tmux.
const maxOutputBytes = 8 << 20

var errOutputTooLarge = errors.New("tmux output too large")

// limitWriter buffers up to max bytes and, on the first byte beyond, stops
// the command through cancel.
type limitWriter struct {
	buf    bytes.Buffer
	max    int
	cancel context.CancelFunc
	over   bool
}

func (w *limitWriter) Write(p []byte) (int, error) {
	if w.over || w.buf.Len()+len(p) > w.max {
		if !w.over {
			w.over = true
			w.cancel()
		}
		return 0, errOutputTooLarge
	}
	return w.buf.Write(p)
}

func (m *Manager) run(ctx context.Context, args ...string) (string, error) {
	return m.runLimited(ctx, maxOutputBytes, args...)
}

func (m *Manager) runLimited(ctx context.Context, max int, args ...string) (string, error) {
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	stdout := &limitWriter{max: max, cancel: cancel}
	var stderr bytes.Buffer
	c := m.cmd(ctx, args...)
	c.Stdout, c.Stderr = stdout, &stderr
	if err := c.Run(); err != nil {
		if stdout.over {
			return "", fmt.Errorf("%w: more than %d bytes", errOutputTooLarge, max)
		}
		return "", classify(strings.TrimSpace(stderr.String()), err)
	}
	return stdout.buf.String(), nil
}

type noServerError struct{}

func (noServerError) Error() string { return "no tmux server running" }

// noSessions reports that no session can exist: no tmux server is running, or
// tmux is not installed on this host.
func noSessions(err error) bool {
	return errors.As(err, new(noServerError)) || errors.Is(err, exec.ErrNotFound)
}

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

const listFormat = "#{session_name}\t#{session_created}\t#{session_attached}\t#{session_activity}\t#{window_width}\t#{window_height}\t#{pane_current_path}" +
	"\t#{pane_dead}\t#{pane_dead_status}\t#{pane_dead_signal}\t#{pane_dead_time}\t#{" + commandOption + "}"

const listFields = 12

func (m *Manager) List(ctx context.Context) ([]Session, error) {
	out, err := m.run(ctx, "list-sessions", "-F", listFormat)
	if noSessions(err) {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("list sessions: %w", err)
	}
	res, err := parseList(out)
	if err != nil || !slices.ContainsFunc(res, func(s Session) bool { return s.unreaped }) {
		return res, err
	}
	m.nudgeReaper(ctx)
	time.Sleep(30 * time.Millisecond)
	if out, err = m.run(ctx, "list-sessions", "-F", listFormat); err != nil {
		return res, nil
	}
	return parseList(out)
}

// nudgeReaper sends SIGCHLD to the tmux server. A command that dies within
// milliseconds of its spawn can be missed by tmux's child reaper, leaving a
// dead pane without exit status until some other child exits; the signal
// makes tmux reap again.
func (m *Manager) nudgeReaper(ctx context.Context) {
	out, err := m.run(ctx, "display-message", "-p", "#{pid}")
	if err != nil {
		return
	}
	if pid := strings.TrimSpace(out); pid != "" {
		_ = exec.CommandContext(ctx, "kill", "-CHLD", pid).Run()
	}
}

func parseList(out string) ([]Session, error) {
	var res []Session
	for _, line := range strings.Split(strings.TrimRight(out, "\n"), "\n") {
		if line == "" {
			continue
		}
		f := strings.SplitN(line, "\t", listFields)
		if len(f) != listFields {
			return nil, fmt.Errorf("parse tmux line %q: want %d fields", line, listFields)
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
			State:           Running,
		}
		if f[7] == "1" {
			if f[8] == "" && f[9] == "" {
				s.unreaped = true
			} else if err := s.parseExit(f[8], f[9], f[10]); err != nil {
				return nil, fmt.Errorf("parse tmux line %q: %w", line, err)
			}
		}
		if f[11] != "" {
			if raw, err := base64.RawURLEncoding.DecodeString(f[11]); err == nil {
				if err := json.Unmarshal(raw, &s.Command); err != nil {
					s.Command = nil
				}
			}
		}
		res = append(res, s)
	}
	return res, nil
}

var signalNames = map[string]syscall.Signal{
	"HUP": syscall.SIGHUP, "INT": syscall.SIGINT, "QUIT": syscall.SIGQUIT, "ILL": syscall.SIGILL,
	"TRAP": syscall.SIGTRAP, "ABRT": syscall.SIGABRT, "BUS": syscall.SIGBUS, "FPE": syscall.SIGFPE,
	"KILL": syscall.SIGKILL, "SEGV": syscall.SIGSEGV, "PIPE": syscall.SIGPIPE, "ALRM": syscall.SIGALRM,
	"TERM": syscall.SIGTERM,
}

// parseSignal reads #{pane_dead_signal}, which is a number on glibc builds of
// tmux but a name such as KILL on Android's bionic.
func parseSignal(s string) (int, error) {
	if n, err := strconv.Atoi(s); err == nil {
		return n, nil
	}
	if sig, ok := signalNames[strings.TrimPrefix(strings.ToUpper(s), "SIG")]; ok {
		return int(sig), nil
	}
	return 0, fmt.Errorf("unknown signal %q", s)
}

// parseExit fills the exit fields. tmux reports a pane dead the moment its pty
// closes, a little before it reaps the process; until then the session still
// counts as running.
func (s *Session) parseExit(status, signal, at string) error {
	s.State = Exited
	if signal != "" {
		n, err := parseSignal(signal)
		if err != nil {
			return err
		}
		s.ExitSignal = n
	}
	switch {
	case status != "":
		n, err := strconv.Atoi(status)
		if err != nil {
			return err
		}
		s.ExitCode = n
	case s.ExitSignal > 0:
		s.ExitCode = 128 + s.ExitSignal
	}
	if at != "" {
		n, err := strconv.ParseInt(at, 10, 64)
		if err != nil {
			return err
		}
		s.Exited = time.Unix(n, 0)
	}
	return nil
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

// escapeArg protects a trailing ";" from tmux's command-sequence parsing.
func escapeArg(a string) string {
	if strings.HasSuffix(a, ";") {
		return strings.TrimSuffix(a, ";") + `\;`
	}
	return a
}

// Create starts a detached session. Its window gets remain-on-exit in the
// same tmux invocation, so even a command that dies instantly keeps its pane:
// crashes leave their output and exit status until Kill or Respawn.
func (m *Manager) Create(ctx context.Context, o CreateOptions) (Session, error) {
	if err := ValidName(o.Name); err != nil {
		return Session{}, err
	}
	args := []string{"new-session", "-d", "-s", o.Name}
	if o.Cwd != "" {
		args = append(args, "-c", escapeArg(o.Cwd))
	}
	for k, v := range o.Env {
		if k == "" || strings.Contains(k, "=") {
			return Session{}, fmt.Errorf("invalid env name %q", k)
		}
		args = append(args, "-e", escapeArg(k+"="+v))
	}
	if len(o.Command) > 0 {
		args = append(args, "--")
		for _, a := range o.Command {
			args = append(args, escapeArg(a))
		}
	}
	args = append(args, ";", "set-option", "-w", "-t", target(o.Name), "remain-on-exit", "on")
	if len(o.Command) > 0 {
		raw, _ := json.Marshal(o.Command)
		args = append(args, ";", "set-option", "-t", target(o.Name), commandOption, base64.RawURLEncoding.EncodeToString(raw))
	}
	if _, err := m.run(ctx, args...); err != nil {
		return Session{}, fmt.Errorf("create session %q: %w", o.Name, err)
	}
	return m.Get(ctx, o.Name)
}

// Respawn restarts an exited session's pane with its original command and
// working directory, keeping the scrollback.
func (m *Manager) Respawn(ctx context.Context, name string) (Session, error) {
	s, err := m.Get(ctx, name)
	if err != nil {
		return Session{}, err
	}
	if s.State != Exited {
		return Session{}, fmt.Errorf("respawn %q: %w", name, ErrNotExited)
	}
	if _, err := m.run(ctx, "respawn-pane", "-k", "-t", target(name)); err != nil {
		return Session{}, fmt.Errorf("respawn %q: %w", name, err)
	}
	return m.Get(ctx, name)
}

func (m *Manager) Kill(ctx context.Context, name string) error {
	if err := ValidName(name); err != nil {
		return err
	}
	_, err := m.run(ctx, "kill-session", "-t", "="+name)
	if noSessions(err) {
		return ErrNotFound
	}
	if err != nil {
		return fmt.Errorf("kill session %q: %w", name, err)
	}
	return nil
}

// Capture returns the visible pane plus lines of scrollback, with ANSI
// escapes preserved and wrapped lines joined. It works on exited sessions
// with tmux's "Pane is dead" banner removed. tmux scrolls the screen up one
// line to draw that banner, so the first screen line of a dead pane's output
// moves into scrollback: pass lines >= 1 to see it. lines is clamped to
// MaxCaptureLines.
func (m *Manager) Capture(ctx context.Context, name string, lines int) (string, error) {
	if err := ValidName(name); err != nil {
		return "", err
	}
	lines = min(max(lines, 0), MaxCaptureLines)
	out, err := m.run(ctx, "capture-pane", "-p", "-e", "-J", "-S", strconv.Itoa(-lines), "-t", target(name))
	if noSessions(err) {
		return "", ErrNotFound
	}
	if err != nil {
		return "", fmt.Errorf("capture %q: %w", name, err)
	}
	return stripDeadBanner(out), nil
}

var sgrRE = regexp.MustCompile(`\x1b\[[0-9;]*m`)

func stripDeadBanner(out string) string {
	body := strings.TrimRight(out, "\n")
	i := strings.LastIndex(body, "\n") + 1
	if !strings.HasPrefix(sgrRE.ReplaceAllString(body[i:], ""), deadMarker) {
		return out
	}
	return strings.TrimRight(body[:i], "\n") + "\n"
}

// Tail returns the last n lines of captured pane text as plain text: ANSI
// colors and tabs are removed, and so are trailing blank lines.
func Tail(text string, n int) string {
	text = strings.ReplaceAll(sgrRE.ReplaceAllString(text, ""), "\t", "    ")
	lines := strings.Split(strings.TrimRight(text, " \t\r\n"), "\n")
	if len(lines) > n {
		lines = lines[len(lines)-n:]
	}
	return strings.Join(lines, "\n")
}

// AttachCmd builds the tmux client command; the caller supplies a PTY. An
// exited session is always attached read-only, since its pane takes no input;
// the returned Session lets the caller tell the user.
func (m *Manager) AttachCmd(ctx context.Context, name string, readOnly bool) (*exec.Cmd, Session, error) {
	if err := ValidName(name); err != nil {
		return nil, Session{}, err
	}
	s, err := m.Get(ctx, name)
	if err != nil {
		return nil, Session{}, err
	}
	args := []string{"attach-session", "-t", "=" + name}
	if readOnly || s.State == Exited {
		args = append(args, "-r")
	}
	c := exec.Command(m.Bin, append([]string{"-L", m.Socket}, args...)...)
	c.Env = append(c.Environ(), "TERM=xterm-256color")
	return c, s, nil
}
