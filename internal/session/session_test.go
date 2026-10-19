package session

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"syscall"
	"testing"
	"time"
)

func TestValidName(t *testing.T) {
	tests := []struct {
		name string
		ok   bool
	}{
		{"a", true}, {"my-session_1", true}, {strings.Repeat("x", 64), true},
		{"", false}, {strings.Repeat("x", 65), false}, {"a:b", false}, {"a.b", false},
		{"a b", false}, {"=a", false}, {"../x", false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := ValidName(tt.name) == nil; got != tt.ok {
				t.Fatalf("ValidName(%q) = %v", tt.name, got)
			}
		})
	}
}

func TestParseList(t *testing.T) {
	tests := []struct {
		name    string
		in      string
		want    int
		wantErr bool
		check   func(t *testing.T, s []Session)
	}{
		{"empty", "", 0, false, nil},
		{"one with command", "a\t100\t2\t200\t80\t24\t/tmp\t0\t\t\t\t[\"sh\",\"-c\",\"x y\"]\n", 1, false, func(t *testing.T, s []Session) {
			if s[0].AttachedClients != 2 || s[0].Cols != 80 || s[0].Cwd != "/tmp" || len(s[0].Command) != 3 || s[0].Command[2] != "x y" || s[0].State != Running {
				t.Fatalf("%+v", s[0])
			}
		}},
		{"no command", "a\t100\t0\t200\t80\t24\t/tmp\t0\t\t\t\t\n", 1, false, func(t *testing.T, s []Session) {
			if s[0].Command != nil {
				t.Fatalf("%+v", s[0])
			}
		}},
		{"exited with status", "a\t100\t0\t200\t80\t24\t/tmp\t1\t3\t\t1700000000\t\n", 1, false, func(t *testing.T, s []Session) {
			if s[0].State != Exited || s[0].ExitCode != 3 || s[0].ExitSignal != 0 || s[0].Exited.Unix() != 1700000000 {
				t.Fatalf("%+v", s[0])
			}
		}},
		{"exited zero", "a\t100\t0\t200\t80\t24\t/tmp\t1\t0\t\t1700000000\t\n", 1, false, func(t *testing.T, s []Session) {
			if s[0].State != Exited || s[0].ExitCode != 0 {
				t.Fatalf("%+v", s[0])
			}
		}},
		{"exited by signal", "a\t100\t0\t200\t80\t24\t/tmp\t1\t\t9\t1700000000\t\n", 1, false, func(t *testing.T, s []Session) {
			if s[0].State != Exited || s[0].ExitSignal != 9 || s[0].ExitCode != 137 {
				t.Fatalf("%+v", s[0])
			}
		}},
		{"exited by named signal", "a\t100\t0\t200\t80\t24\t/tmp\t1\t\tKILL\t1700000000\t\n", 1, false, func(t *testing.T, s []Session) {
			if s[0].State != Exited || s[0].ExitSignal != int(syscall.SIGKILL) || s[0].ExitCode != 128+int(syscall.SIGKILL) {
				t.Fatalf("%+v", s[0])
			}
		}},
		{"unknown signal name", "a\t100\t0\t200\t80\t24\t/tmp\t1\t\tBOGUS\t1700000000\t\n", 0, true, nil},
		{"pty closed, status not yet known", "a\t100\t0\t200\t80\t24\t/tmp\t1\t\t\t\t\n", 1, false, func(t *testing.T, s []Session) {
			if s[0].State != Running {
				t.Fatalf("%+v", s[0])
			}
		}},
		{"bad exit status", "a\t100\t0\t200\t80\t24\t/tmp\t1\tx\t\t\t\n", 0, true, nil},
		{"too few fields", "a\t100\n", 0, true, nil},
		{"bad number", "a\tx\t0\t0\t80\t24\t/\t0\t\t\t\t\n", 0, true, nil},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := parseList(tt.in)
			if (err != nil) != tt.wantErr || len(got) != tt.want {
				t.Fatalf("got %v, %v", got, err)
			}
			if tt.check != nil {
				tt.check(t, got)
			}
		})
	}
}

func TestEscapeArg(t *testing.T) {
	tests := []struct{ in, want string }{
		{"plain", "plain"}, {"a;", `a\;`}, {";", `\;`}, {`a\;`, `a\\;`}, {"a;b", "a;b"},
	}
	for _, tt := range tests {
		if got := escapeArg(tt.in); got != tt.want {
			t.Errorf("escapeArg(%q) = %q, want %q", tt.in, got, tt.want)
		}
	}
}

func TestStripDeadBanner(t *testing.T) {
	tests := []struct{ name, in, want string }{
		{"plain", "a\nb\n", "a\nb\n"},
		{"banner", "a\nb\n\n\nPane is dead (status 3, Mon Oct 19 16:44:13 2026)\n", "a\nb\n"},
		{"colored banner", "a\n\x1b[31mPane is dead (signal 9)\x1b[0m\n", "a\n"},
		{"banner text not last", "Pane is dead (x)\nmore\n", "Pane is dead (x)\nmore\n"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := stripDeadBanner(tt.in); got != tt.want {
				t.Fatalf("got %q, want %q", got, tt.want)
			}
		})
	}
}

func TestTail(t *testing.T) {
	tests := []struct {
		in   string
		n    int
		want string
	}{
		{"a\nb\nc\n\n\n", 2, "b\nc"},
		{"a\nb", 5, "a\nb"},
		{"\x1b[31mred\x1b[0m\tx\n", 3, "red    x"},
		{"", 3, ""},
	}
	for _, tt := range tests {
		if got := Tail(tt.in, tt.n); got != tt.want {
			t.Errorf("Tail(%q, %d) = %q, want %q", tt.in, tt.n, got, tt.want)
		}
	}
}

func TestManagerLifecycle(t *testing.T) {
	if _, err := exec.LookPath("tmux"); err != nil {
		t.Skip("tmux not installed")
	}
	sock := fmt.Sprintf("protectorate-test-%d", time.Now().UnixNano())
	m := New(sock)
	t.Cleanup(func() { exec.Command("tmux", "-L", sock, "kill-server").Run() })
	ctx := context.Background()

	if got, err := m.List(ctx); err != nil || len(got) != 0 {
		t.Fatalf("no server: %v, %v", got, err)
	}
	if err := m.Kill(ctx, "x"); !errors.Is(err, ErrNotFound) {
		t.Fatalf("kill without server: %v", err)
	}
	if _, err := m.Create(ctx, CreateOptions{Name: "bad=name"}); !errors.Is(err, ErrInvalidName) {
		t.Fatalf("invalid name: %v", err)
	}
	if _, err := m.Create(ctx, CreateOptions{Name: "e", Env: map[string]string{"A=B": "x"}}); err == nil {
		t.Fatal("expected invalid env error")
	}
	s, err := m.Create(ctx, CreateOptions{Name: "s", Cwd: t.TempDir(), Command: []string{"sleep", "60"}})
	if err != nil {
		t.Fatal(err)
	}
	if len(s.Command) != 2 || s.Command[0] != "sleep" {
		t.Fatalf("%+v", s)
	}
	if _, err := m.Create(ctx, CreateOptions{Name: "s"}); !errors.Is(err, ErrExists) {
		t.Fatalf("duplicate: %v", err)
	}
	if _, _, err := m.AttachCmd(ctx, "nope", false); !errors.Is(err, ErrNotFound) {
		t.Fatalf("attach missing: %v", err)
	}
	if err := m.Kill(ctx, "s"); err != nil {
		t.Fatal(err)
	}
	if err := m.Kill(ctx, "s"); !errors.Is(err, ErrNotFound) {
		t.Fatalf("kill twice: %v", err)
	}
}

func newTestManager(t *testing.T) *Manager {
	t.Helper()
	if _, err := exec.LookPath("tmux"); err != nil {
		t.Skip("tmux not installed")
	}
	sock := fmt.Sprintf("protectorate-test-%d", time.Now().UnixNano())
	t.Cleanup(func() { exec.Command("tmux", "-L", sock, "kill-server").Run() })
	return New(sock)
}

func waitExited(t *testing.T, m *Manager, name string) Session {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for {
		s, err := m.Get(context.Background(), name)
		if err != nil {
			t.Fatalf("get %s: %v", name, err)
		}
		if s.State == Exited {
			return s
		}
		if time.Now().After(deadline) {
			raw, _ := m.run(context.Background(), "list-sessions", "-F", "#{session_name} dead=#{pane_dead} st=#{pane_dead_status} sig=#{pane_dead_signal} t=#{pane_dead_time}")
			t.Fatalf("%s did not exit: %s", name, raw)
		}
		time.Sleep(10 * time.Millisecond)
	}
}

func TestSessionExit(t *testing.T) {
	m := newTestManager(t)
	ctx := context.Background()
	// A second session keeps the tmux server alive and must be unaffected.
	if _, err := m.Create(ctx, CreateOptions{Name: "keep", Command: []string{"sleep", "60"}}); err != nil {
		t.Fatal(err)
	}

	tests := []struct {
		name       string
		command    []string
		wantCode   int
		wantSignal int
		wantOut    string
	}{
		{"exit 3", []string{"sh", "-c", "echo last-words; exit 3"}, 3, 0, "last-words"},
		{"exit 0", []string{"sh", "-c", "echo fine"}, 0, 0, "fine"},
		{"killed by signal", []string{"sh", "-c", "echo dying; kill -9 $$"}, 137, 9, "dying"},
		{"arg ending in semicolon", []string{"sh", "-c", "echo \"$0\"; exit 1", "end;"}, 1, 0, "end;"},
	}
	for i, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			name := fmt.Sprintf("x%d", i)
			if _, err := m.Create(ctx, CreateOptions{Name: name, Command: tt.command}); err != nil {
				t.Fatal(err)
			}
			s := waitExited(t, m, name)
			if s.ExitCode != tt.wantCode || s.ExitSignal != tt.wantSignal || s.Exited.IsZero() {
				t.Fatalf("%+v", s)
			}
			if len(s.Command) != len(tt.command) {
				t.Fatalf("command not preserved: %v", s.Command)
			}
			out, err := m.Capture(ctx, name, 50)
			if err != nil || !strings.Contains(out, tt.wantOut) || strings.Contains(out, "Pane is dead") {
				t.Fatalf("capture = %q, %v", out, err)
			}
			if err := m.Kill(ctx, name); err != nil {
				t.Fatalf("kill dead session: %v", err)
			}
			if _, err := m.Get(ctx, name); !errors.Is(err, ErrNotFound) {
				t.Fatalf("get after kill: %v", err)
			}
		})
	}
	if s, err := m.Get(ctx, "keep"); err != nil || s.State != Running {
		t.Fatalf("keep = %+v, %v", s, err)
	}
}

func TestRespawn(t *testing.T) {
	m := newTestManager(t)
	ctx := context.Background()
	dir := t.TempDir()
	runs := filepath.Join(dir, "runs")
	if _, err := m.Create(ctx, CreateOptions{Name: "r", Cwd: dir, Command: []string{"sh", "-c", "echo run >> runs; exit 4"}}); err != nil {
		t.Fatal(err)
	}
	waitExited(t, m, "r")
	if _, err := m.Create(ctx, CreateOptions{Name: "r"}); !errors.Is(err, ErrExists) {
		t.Fatalf("create over dead session: %v", err)
	}
	if _, err := m.Respawn(ctx, "r"); err != nil {
		t.Fatal(err)
	}
	waitExited(t, m, "r")
	if b, _ := os.ReadFile(runs); strings.Count(string(b), "run") != 2 {
		t.Fatalf("runs file = %q, want two runs in the original cwd", b)
	}

	if _, err := m.Create(ctx, CreateOptions{Name: "live", Command: []string{"sleep", "60"}}); err != nil {
		t.Fatal(err)
	}
	if _, err := m.Respawn(ctx, "live"); !errors.Is(err, ErrNotExited) {
		t.Fatalf("respawn running: %v", err)
	}
	if _, err := m.Respawn(ctx, "ghost"); !errors.Is(err, ErrNotFound) {
		t.Fatalf("respawn missing: %v", err)
	}
}

func TestAttachCmdExitedIsReadOnly(t *testing.T) {
	m := newTestManager(t)
	ctx := context.Background()
	if _, err := m.Create(ctx, CreateOptions{Name: "k", Command: []string{"sleep", "60"}}); err != nil {
		t.Fatal(err)
	}
	if _, err := m.Create(ctx, CreateOptions{Name: "d", Command: []string{"true"}}); err != nil {
		t.Fatal(err)
	}
	waitExited(t, m, "d")
	for name, wantRO := range map[string]bool{"k": false, "d": true} {
		cmd, s, err := m.AttachCmd(ctx, name, false)
		if err != nil {
			t.Fatal(err)
		}
		if got := slices.Contains(cmd.Args, "-r"); got != wantRO || (s.State == Exited) != wantRO {
			t.Errorf("%s: read-only = %v, state %s", name, got, s.State)
		}
	}
}
