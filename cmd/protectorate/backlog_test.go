package main

import (
	"bytes"
	"context"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/hotschmoe/protectorate/internal/hub"
)

func TestBacklogCLI(t *testing.T) {
	home := t.TempDir()
	t.Setenv("PROTECTORATE_HOME", home)
	tok, err := hub.EnsureToken()
	if err != nil {
		t.Fatal(err)
	}
	h := hub.New("test", tok)
	h.Heartbeat = 10 * time.Millisecond
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- h.Serve(ctx, l) }()
	t.Cleanup(func() { cancel(); <-done })
	url := "http://" + l.Addr().String()

	run := func(args ...string) (string, error) {
		var buf bytes.Buffer
		cmd := newRoot()
		cmd.SetOut(&buf)
		cmd.SetArgs(append(args, "--hub", url))
		err := cmd.Execute()
		return buf.String(), err
	}

	out, err := run("backlog", "add", "me/repo", "Fix thing", "--prio", "1", "--harness", "claude", "--json")
	if err != nil || !strings.Contains(out, `"title":"Fix thing"`) || !strings.Contains(out, `"priority":1`) {
		t.Fatalf("add: %q %v", out, err)
	}
	id := between(out, `"id":"`, `"`)
	if _, err := run("backlog", "add", "donate: o/r", "x"); err == nil {
		t.Error("donate without issue should fail")
	}
	if _, err := run("backlog", "add", "donate: o/r", "x", "--issue", "https://github.com/o/r/issues/1"); err != nil {
		t.Fatal(err)
	}

	tests := []struct {
		name    string
		args    []string
		want    []string
		wantErr bool
	}{
		{"ls", []string{"backlog", "ls"}, []string{"Fix thing", "todo", "p1", "claude", "donate: o/r"}, false},
		{"ls project", []string{"backlog", "ls", "--project", "me/repo", "--json"}, []string{`"title":"Fix thing"`}, false},
		{"ls bad state", []string{"backlog", "ls", "--state", "nope"}, nil, true},
		{"claim", []string{"backlog", "claim", id, "--by", "alice/claude"}, []string{"claimed", "alice/claude"}, false},
		{"claim taken", []string{"backlog", "claim", id, "--by", "bob"}, nil, true},
		{"claim needs by", []string{"backlog", "claim", id}, nil, true},
		{"ls claimed json", []string{"backlog", "ls", "--state", "claimed", "--json"}, []string{`"state":"ITEM_STATE_CLAIMED"`, `"by":"alice/claude"`}, false},
		{"done", []string{"backlog", "done", id}, []string{"done"}, false},
		{"done unknown", []string{"backlog", "done", "ffff"}, nil, true},
		{"edit json", []string{"backlog", "edit", "--json"}, []string{filepath.Join(home, "backlog.md")}, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			out, err := run(tt.args...)
			if (err != nil) != tt.wantErr {
				t.Fatalf("err = %v", err)
			}
			for _, w := range tt.want {
				if !strings.Contains(out, w) {
					t.Errorf("output %q lacks %q", out, w)
				}
			}
		})
	}

	t.Run("edit runs editor", func(t *testing.T) {
		sh, err := exec.LookPath("true")
		if err != nil {
			t.Skip("no true binary")
		}
		t.Setenv("EDITOR", sh)
		if _, err := run("backlog", "edit"); err != nil {
			t.Fatal(err)
		}
		if _, err := os.Stat(home); err != nil {
			t.Fatal(err)
		}
	})
}

func between(s, a, b string) string {
	_, rest, _ := strings.Cut(s, a)
	v, _, _ := strings.Cut(rest, b)
	return v
}
