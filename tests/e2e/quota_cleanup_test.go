//go:build unix

package e2e

import (
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
	"testing"
)

func TestBinaryQuotaReapsClosedStdoutPeer(t *testing.T) {
	c := newCluster(t)
	c.stop(c.hub)
	proxy := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, "no external provider calls in this scenario", http.StatusBadGateway)
	}))
	defer proxy.Close()
	peer := `#!/bin/sh
trap '' TERM
printf '%s\n' "$$" > "$HOME/quota-peer.pid"
read request
printf '%s\n' '{"id":1,"result":{}}'
read request
read request
printf '%s\n' '{"id":2,"result":{"rateLimits":{"planType":"quota-cleanup","primary":{"usedPercent":25,"windowDurationMins":300}}}}'
exec 1>&-
while :; do sleep 1; done
`
	if err := os.WriteFile(filepath.Join(c.root, ".local", "bin", "codex"), []byte(peer), 0700); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(c.root, ".codex"), 0700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(c.root, ".codex", "auth.json"), []byte(`{"tokens":{"access_token":"fixture-not-a-provider-credential"}}`), 0600); err != nil {
		t.Fatal(err)
	}
	c.env = append(c.env, "PATH="+filepath.Join(c.root, ".local", "bin")+":"+os.Getenv("PATH"),
		"HTTP_PROXY="+proxy.URL, "HTTPS_PROXY="+proxy.URL, "NO_PROXY=127.0.0.1,localhost")
	t.Cleanup(func() {
		raw, err := os.ReadFile(filepath.Join(c.root, "quota-peer.pid"))
		if err == nil {
			if pid, err := strconv.Atoi(strings.TrimSpace(string(raw))); err == nil && pid > 0 {
				_ = syscall.Kill(-pid, syscall.SIGKILL)
			}
		}
	})
	c.boot()
	c.eventually(func() bool {
		return strings.Contains(fmt.Sprint(c.call("QuotaService", "GetQuota", map[string]any{})), "quota-cleanup")
	})
	raw, err := os.ReadFile(filepath.Join(c.root, "quota-peer.pid"))
	if err != nil {
		t.Fatal(err)
	}
	pid, err := strconv.Atoi(strings.TrimSpace(string(raw)))
	if err != nil || pid <= 0 {
		t.Fatalf("invalid quota peer PID: %q: %v", raw, err)
	}
	if err := syscall.Kill(pid, 0); err != syscall.ESRCH {
		t.Fatalf("quota was published with the peer still alive or unreaped: %v", err)
	}
}
