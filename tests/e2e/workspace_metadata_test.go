//go:build unix

package e2e

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
	"testing"
	"time"
)

func TestBinaryWorkspaceMetadataIsBounded(t *testing.T) {
	c := newCluster(t)
	c.stop(c.hub)
	root := filepath.Join(c.root, "github")
	var records []any
	for _, name := range []string{"head-fifo", "config-fifo", "oversized-head", "escaped-head"} {
		dir := filepath.Join(root, name, ".git")
		if err := os.MkdirAll(dir, 0700); err != nil {
			t.Fatal(err)
		}
		head := "ref: refs/heads/normal\n"
		if name == "oversized-head" {
			head = "ref: refs/heads/" + strings.Repeat("a", 2<<20)
		}
		if name == "head-fifo" {
			if err := syscall.Mkfifo(filepath.Join(dir, "HEAD"), 0600); err != nil {
				t.Fatal(err)
			}
		} else if name == "escaped-head" {
			outside := filepath.Join(c.root, "private-head")
			if err := os.WriteFile(outside, []byte("ref: refs/heads/private\n"), 0600); err != nil {
				t.Fatal(err)
			}
			if err := os.Symlink(outside, filepath.Join(dir, "HEAD")); err != nil {
				t.Fatal(err)
			}
		} else if err := os.WriteFile(filepath.Join(dir, "HEAD"), []byte(head), 0600); err != nil {
			t.Fatal(err)
		}
		if name == "config-fifo" {
			if err := syscall.Mkfifo(filepath.Join(dir, "config"), 0600); err != nil {
				t.Fatal(err)
			}
		} else if err := os.WriteFile(filepath.Join(dir, "config"), []byte("[remote \"origin\"]\nurl=https://example.invalid/repo\n"), 0600); err != nil {
			t.Fatal(err)
		}
		row := map[string]any{"name": name, "workspace": filepath.Dir(dir), "harness": "shell", "state": "exited", "created": "2026-01-01T00:00:00Z"}
		if c.rust {
			row["mode"] = "terminal"
			row["session"] = "sleeve-" + name
			row["createdAt"] = row["created"]
			row["exitedAt"] = "2026-01-01T00:01:00Z"
			delete(row, "created")
		}
		records = append(records, row)
	}
	registry := map[string]any{"sleeves": records}
	path := filepath.Join(c.home, "sleeves.json")
	if c.rust {
		registry["version"] = 1
		path = filepath.Join(c.home, "sleeves-rust.json")
	}
	record, err := json.Marshal(registry)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, record, 0600); err != nil {
		t.Fatal(err)
	}
	c.boot()
	for _, tc := range []struct {
		service, method string
		body            string
	}{
		{"SleeveService", "ListSleeves", `{}`},
		{"SleeveService", "BrowseWorkspaces", `{"path":` + strconv.Quote(root) + `}`},
		{"SleeveService", "ListWorkspaces", `{}`},
	} {
		ctx, cancel := context.WithTimeout(c.ctx, 2*time.Second)
		req, err := http.NewRequestWithContext(ctx, "POST", c.url+"/protectorate.v1."+tc.service+"/"+tc.method, bytes.NewBufferString(tc.body))
		if err != nil {
			cancel()
			t.Fatal(err)
		}
		req.Header.Set("Content-Type", "application/json")
		req.Header.Set("Authorization", "Bearer "+c.token)
		res, err := c.client.Do(req)
		if err != nil {
			cancel()
			t.Fatalf("%s blocked on untrusted git metadata: %v", tc.method, err)
		}
		body, err := io.ReadAll(res.Body)
		res.Body.Close()
		cancel()
		if err != nil || res.StatusCode != 200 || len(body) > 64<<10 {
			t.Fatalf("%s: status=%d bytes=%d err=%v", tc.method, res.StatusCode, len(body), err)
		}
		if strings.Contains(string(body), `"private"`) || strings.Contains(string(body), strings.Repeat("a", 1024)) {
			t.Fatalf("%s returned escaped or oversized metadata", tc.method)
		}
	}
}
