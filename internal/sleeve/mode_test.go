//go:build linux

package sleeve

import (
	"context"
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestResolveMode(t *testing.T) {
	tests := []struct {
		name string
		h    Harness
		m    Mode
		args []string
		want Mode
		bad  bool
	}{
		{"agent defaults to chat", Claude, "", nil, ModeChat, false},
		{"shell defaults to terminal", Shell, "", nil, ModeTerminal, false},
		{"agent terminal", Pi, ModeTerminal, []string{"-x"}, ModeTerminal, false},
		{"shell chat", Shell, ModeChat, nil, "", true},
		{"chat with args", Grok, ModeChat, []string{"--model", "x"}, "", true},
		{"unknown", Codex, "tui", nil, "", true},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got, err := resolveMode(tc.h, tc.m, tc.args)
			if (err != nil) != tc.bad || (err != nil && !errors.Is(err, ErrInvalid)) || got != tc.want {
				t.Fatalf("got %q, %v", got, err)
			}
		})
	}
}

func TestTerminalArgs(t *testing.T) {
	if got := terminalArgs(Codex, []string{"-m", "o3"}); !reflect.DeepEqual(got, []string{"--disable", "daemon_auto_start", "-m", "o3"}) {
		t.Errorf("codex: %v", got)
	}
	if got := terminalArgs(Claude, []string{"-x"}); !reflect.DeepEqual(got, []string{"-x"}) {
		t.Errorf("claude: %v", got)
	}
}

func TestChatSleeveLifecycle(t *testing.T) {
	r := newRig(t)
	if _, err := r.cfg.Locate(Claude); err != nil {
		t.Skipf("claude not available: %v", err)
	}
	ws := r.workspace(t, "chat")
	name := r.pfx + "chat"
	if _, err := r.m.Spawn(context.Background(), SpawnRequest{Name: name, Workspace: ws, Harness: Shell, Mode: ModeChat}); !errors.Is(err, ErrInvalid) {
		t.Fatalf("shell chat spawn: %v", err)
	}
	info, err := r.m.Spawn(context.Background(), SpawnRequest{Name: name, Workspace: ws, Harness: Claude, Env: map[string]string{"K": "v"}})
	if err != nil {
		t.Fatal(err)
	}
	if info.Mode != ModeChat || info.Session != "" || info.State != Running || info.Chat == nil {
		t.Fatalf("chat sleeve = %+v", info)
	}
	if list, _ := r.sess.List(context.Background()); len(list) != 0 {
		t.Fatalf("chat sleeve created tmux sessions: %v", list)
	}
	if _, err := r.m.Spawn(context.Background(), SpawnRequest{Name: name, Workspace: ws, Harness: Claude}); !errors.Is(err, ErrExists) {
		t.Fatalf("duplicate spawn: %v", err)
	}
	if err := r.m.Reconcile(context.Background()); err != nil {
		t.Fatal(err)
	}
	if got, _ := r.m.Get(name); got.State != Running {
		t.Fatalf("reconcile exited a chat sleeve: %+v", got)
	}

	before, _ := r.m.Get(name)
	if _, err := r.m.SetChat(name, func(c *Chat) { c.State, c.SessionID, c.LastSeq = "idle", "sid-1", 7 }); err != nil {
		t.Fatal(err)
	}
	if before.Chat.SessionID != "" {
		t.Fatal("SetChat mutated a Chat shared with an earlier Info")
	}
	r.open(t)
	got, err := r.m.Get(name)
	if err != nil || got.Chat == nil || got.Chat.SessionID != "sid-1" || got.Chat.LastSeq != 7 || got.Mode != ModeChat {
		t.Fatalf("after reopen: %+v %v", got, err)
	}

	cmd, err := r.m.Command(name)
	if err != nil {
		t.Fatal(err)
	}
	argv, err := cmd("claude", []string{"-p"})
	if err != nil || !strings.Contains(strings.Join(argv, " "), "--args 3") {
		t.Fatalf("wrapper lost the spawn env: %v %v", argv, err)
	}
	if err := r.m.Kill(context.Background(), name); err != nil {
		t.Fatal(err)
	}
	if _, err := r.m.Get(name); !errors.Is(err, ErrNotFound) {
		t.Fatalf("after kill: %v", err)
	}
}
