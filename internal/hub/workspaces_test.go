//go:build linux

package hub

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"testing"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/session"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

func TestListWorkspaces(t *testing.T) {
	home := t.TempDir()
	repo := filepath.Join(home, "workspaces", "app")
	if err := os.MkdirAll(filepath.Join(repo, ".git"), 0o755); err != nil {
		t.Fatal(err)
	}
	for f, body := range map[string]string{
		"HEAD":   "ref: refs/heads/main\n",
		"config": "[remote \"origin\"]\n\turl = https://example.com/org/app.git\n",
	} {
		if err := os.WriteFile(filepath.Join(repo, ".git", f), []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	cfg, err := sleeve.DefaultConfig(home, nil)
	if err != nil {
		t.Fatal(err)
	}
	h := New("test", testToken)
	h.Sessions = session.New(fmt.Sprintf("prot-ws-test-%d", time.Now().UnixNano()%1000000))
	m, err := sleeve.Open(context.Background(), h.SleeveOptions(cfg))
	if err != nil {
		t.Fatal(err)
	}
	h.SetSleeves(m)

	res, err := h.ListWorkspaces(context.Background(), connect.NewRequest(&pb.ListWorkspacesRequest{}))
	if err != nil {
		t.Fatal(err)
	}
	if len(res.Msg.Workspaces) != 1 {
		t.Fatalf("got %d workspaces, want 1: %v", len(res.Msg.Workspaces), res.Msg.Workspaces)
	}
	w := res.Msg.Workspaces[0]
	if w.Name != "app" || w.GitBranch != "main" || w.GitRemote != "https://example.com/org/app.git" || w.RequiredName != "app" || w.Busy || w.Sleeve != "" {
		t.Errorf("unexpected workspace %v", w)
	}
}
