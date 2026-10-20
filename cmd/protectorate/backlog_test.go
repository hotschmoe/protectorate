package main

import (
	"bytes"
	"context"
	"net"
	"os"
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

	out, err := run("backlog", "add", "Fix thing", "--project", "me/repo", "--body", "line one", "--json")
	if err != nil || !strings.Contains(out, `"title":"Fix thing"`) || !strings.Contains(out, `"source":"owner"`) {
		t.Fatalf("add: %q %v", out, err)
	}
	id := between(out, `"id":"`, `"`)
	if _, err := run("backlog", "add"); err == nil {
		t.Error("an empty note should fail")
	}
	if _, err := run("backlog", "add", "Second", "--pin"); err != nil {
		t.Fatal(err)
	}

	tests := []struct {
		name    string
		args    []string
		want    []string
		notWant []string
		wantErr bool
	}{
		{"ls", []string{"backlog", "ls"}, []string{"Fix thing", "me/repo", "Second", "pin"}, nil, false},
		{"ls project", []string{"backlog", "ls", "--project", "me/repo", "--json"}, []string{`"title":"Fix thing"`}, []string{"Second"}, false},
		{"ls query", []string{"backlog", "ls", "--query", "LINE ONE"}, []string{"Fix thing"}, []string{"Second"}, false},
		{"show", []string{"backlog", "show", id}, []string{"Fix thing", "me/repo", "owner", "line one"}, nil, false},
		{"show unknown", []string{"backlog", "show", "ffff"}, nil, nil, true},
		{"edit title", []string{"backlog", "edit", id, "--title", "Fixed thing"}, []string{"Fixed thing"}, nil, false},
		{"edit append", []string{"backlog", "edit", id, "--append", "line two"}, []string{"Fixed thing"}, nil, false},
		{"show appended", []string{"backlog", "show", id}, []string{"line one", "line two"}, nil, false},
		{"edit body and append", []string{"backlog", "edit", id, "--body", "x", "--append", "y"}, nil, nil, true},
		{"edit clears project", []string{"backlog", "edit", id, "--project", ""}, nil, []string{"me/repo"}, false},
		{"edit json needs a change", []string{"backlog", "edit", id, "--json"}, nil, nil, true},
		{"pin", []string{"backlog", "pin", id, "--json"}, []string{`"pinned":true`}, nil, false},
		{"unpin", []string{"backlog", "pin", id, "--off", "--json"}, nil, []string{`"pinned":true`}, false},
		{"rm", []string{"backlog", "rm", id}, []string{"deleted"}, nil, false},
		{"rm again", []string{"backlog", "rm", id}, nil, nil, true},
		{"ls after rm", []string{"backlog", "ls"}, []string{"Second"}, []string{"Fixed thing"}, false},
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
			for _, w := range tt.notWant {
				if strings.Contains(out, w) {
					t.Errorf("output %q has %q", out, w)
				}
			}
		})
	}

	t.Run("edit runs editor", func(t *testing.T) {
		script := filepath.Join(t.TempDir(), "ed.sh")
		if err := os.WriteFile(script, []byte("#!/bin/sh\nprintf 'Edited title\\n\\nEdited body\\n' > \"$1\"\n"), 0o755); err != nil {
			t.Fatal(err)
		}
		t.Setenv("EDITOR", script)
		add, err := run("backlog", "add", "Before", "--json")
		if err != nil {
			t.Fatal(err)
		}
		nid := between(add, `"id":"`, `"`)
		if _, err := run("backlog", "edit", nid); err != nil {
			t.Fatal(err)
		}
		out, err := run("backlog", "show", nid)
		if err != nil || !strings.Contains(out, "Edited title") || !strings.Contains(out, "Edited body") {
			t.Fatalf("show = %q err=%v", out, err)
		}
	})

	t.Run("body from stdin", func(t *testing.T) {
		var buf bytes.Buffer
		cmd := newRoot()
		cmd.SetOut(&buf)
		cmd.SetIn(strings.NewReader("piped body"))
		cmd.SetArgs([]string{"backlog", "add", "From stdin", "--body-file", "-", "--hub", url, "--json"})
		if err := cmd.Execute(); err != nil || !strings.Contains(buf.String(), `"body":"piped body"`) {
			t.Fatalf("out = %q err=%v", buf.String(), err)
		}
	})
}

func between(s, a, b string) string {
	_, rest, _ := strings.Cut(s, a)
	v, _, _ := strings.Cut(rest, b)
	return v
}
