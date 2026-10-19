package session

import (
	"context"
	"errors"
	"fmt"
	"os/exec"
	"strings"
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
		{"one with command", "a\t100\t2\t200\t80\t24\t/tmp\t[\"sh\",\"-c\",\"x y\"]\n", 1, false, func(t *testing.T, s []Session) {
			if s[0].AttachedClients != 2 || s[0].Cols != 80 || s[0].Cwd != "/tmp" || len(s[0].Command) != 3 || s[0].Command[2] != "x y" {
				t.Fatalf("%+v", s[0])
			}
		}},
		{"no command", "a\t100\t0\t200\t80\t24\t/tmp\t\n", 1, false, func(t *testing.T, s []Session) {
			if s[0].Command != nil {
				t.Fatalf("%+v", s[0])
			}
		}},
		{"too few fields", "a\t100\n", 0, true, nil},
		{"bad number", "a\tx\t0\t0\t80\t24\t/\t\n", 0, true, nil},
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
	if _, err := m.AttachCmd(ctx, "nope", false); !errors.Is(err, ErrNotFound) {
		t.Fatalf("attach missing: %v", err)
	}
	if err := m.Kill(ctx, "s"); err != nil {
		t.Fatal(err)
	}
	if err := m.Kill(ctx, "s"); !errors.Is(err, ErrNotFound) {
		t.Fatalf("kill twice: %v", err)
	}
}
