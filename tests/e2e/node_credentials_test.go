package e2e

import (
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
)

func TestBinaryNodeCredentialBoundaries(t *testing.T) {
	c := newCluster(t)
	var redirected, leaked atomic.Int32
	var authenticated atomic.Bool
	sink := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		leaked.Add(1)
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer sink.Close()
	redirector := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") == "Bearer e2e-node-secret" {
			authenticated.Store(true)
		}
		redirected.Add(1)
		http.Redirect(w, r, sink.URL+r.URL.Path, http.StatusTemporaryRedirect)
	}))
	defer redirector.Close()
	c.cli("node", "add", "redirecting", "--addr", strings.TrimPrefix(redirector.URL, "http://"))
	c.eventually(func() bool { return redirected.Load() >= 2 })
	if !authenticated.Load() || leaked.Load() != 0 {
		t.Fatalf("node polling forwarded credentials to a redirect: original authenticated=%t destination requests=%d", authenticated.Load(), leaked.Load())
	}
	c.call("NodeService", "SetNodeTier", map[string]any{"name": "peer", "tier": "care"})
	input := filepath.Join(c.root, "private-input")
	if err := os.WriteFile(input, []byte("care file must stay private"), 0600); err != nil {
		t.Fatal(err)
	}
	c.eventually(func() bool {
		return strings.Contains(fmt.Sprint(c.call("NodeService", "GetNode", map[string]any{"name": "peer"})), "NODE_FRESHNESS_FRESH")
	})
	c.cli("node", "put", "peer", input, "care-private")
	owner := c.token
	plain := c.call("TokenService", "CreateToken", map[string]any{"name": "file-worker", "scopes": []string{"node_get_file", "node_put_file"}})["secret"].(string)
	credentials := filepath.Join(c.root, "file-worker-token")
	if err := os.WriteFile(credentials, []byte(plain), 0600); err != nil {
		t.Fatal(err)
	}
	download := filepath.Join(c.root, "care-download")
	careError := "tier care"
	if c.rust {
		careError = "care node requires an owner or node admin grant"
	}
	c.rejectedCLI(careError, "--token-file", credentials, "node", "get", "peer", "care-private", download)
	c.rejectedCLI(careError, "--token-file", credentials, "node", "put", "peer", input, "forbidden-write")
	for _, path := range []string{download, filepath.Join(c.root, "node", "node-files", "forbidden-write")} {
		if _, err := os.Stat(path); !os.IsNotExist(err) {
			t.Fatalf("denied care access produced a file: %s: %v", path, err)
		}
	}
	grant := c.call("GrantService", "Admin", map[string]any{"node": "peer", "task": "read a care node file"})
	c.call("TokenService", "UpdateTokenScopes", map[string]any{"id": grant["token"].(map[string]any)["id"], "add": []string{"node_get_file"}})
	if err := os.WriteFile(credentials, []byte(grant["secret"].(string)), 0600); err != nil {
		t.Fatal(err)
	}
	c.cli("--token-file", credentials, "node", "get", "peer", "care-private", download)
	if b, err := os.ReadFile(download); err != nil || string(b) != "care file must stay private" {
		t.Fatalf("explicit care grant did not read the file intact: %q %v", b, err)
	}
	c.token = owner
	c.cli("node", "rm", "redirecting")
	if leaked.Load() != 0 {
		t.Fatalf("redirect destination received %d requests", leaked.Load())
	}
}
