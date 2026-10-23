package e2e

import (
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"github.com/hotschmoe/protectorate/internal/hub"
)

func (c *cluster) updateScenario(t *testing.T) {
	get := func(path string) []byte {
		t.Helper()
		req, err := http.NewRequestWithContext(c.ctx, "GET", c.url+path, nil)
		if err != nil {
			t.Fatal(err)
		}
		res, err := c.client.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		defer res.Body.Close()
		b, err := io.ReadAll(res.Body)
		if err != nil || res.StatusCode != 200 {
			t.Fatalf("download %s: %d %v", path, res.StatusCode, err)
		}
		return b
	}
	installer := filepath.Join(c.root, "install.sh")
	req, err := http.NewRequestWithContext(c.ctx, "GET", c.url+"/install.sh", nil)
	if err != nil {
		t.Fatal(err)
	}
	res, err := c.client.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	res.Body.Close()
	if res.StatusCode != 503 {
		t.Fatalf("installer without tailnet: %d", res.StatusCode)
	}
	if c.rust {
		for _, path := range []string{"/install.ps1", "/dl/protectorate-rs-linux-" + runtime.GOARCH} {
			res, err := c.client.Get(c.url + path)
			if err != nil {
				t.Fatal(err)
			}
			res.Body.Close()
			want := 503
			if strings.HasPrefix(path, "/dl/") {
				want = 404
			}
			if res.StatusCode != want {
				t.Fatalf("%s without active release: %d want %d", path, res.StatusCode, want)
			}
		}
		staged := filepath.Join(c.root, "staged-update")
		if err := os.WriteFile(staged, []byte("invalid update"), 0700); err != nil {
			t.Fatal(err)
		}
		c.cli("node", "put", "peer", staged, "candidate")
		var rejection map[string]any
		if status := c.rpc("NodeService", "ApplyUpdate", map[string]any{"name": "peer", "path": "candidate", "size": 14, "sha256": strings.Repeat("0", 64)}, &rejection); status != 400 || rejection["code"] != "failed_precondition" {
			t.Fatalf("bad update digest status=%d response=%v", status, rejection)
		}
		return
	}
	script, err := hub.InstallScript("sh", c.url, c.url)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(installer, script, 0600); err != nil {
		t.Fatal(err)
	}
	cmd := exec.CommandContext(c.ctx, "sh", installer, "--print-target")
	cmd.Env = c.env
	out, err := cmd.CombinedOutput()
	target := "protectorate-" + runtime.GOOS + "-" + runtime.GOARCH
	if err != nil || strings.TrimSpace(string(out)) != target {
		t.Fatalf("installer target: %s %v", out, err)
	}

	curl, err := exec.LookPath("curl")
	if err != nil {
		t.Fatal(err)
	}
	shim := filepath.Join(c.root, "checksum-peer")
	if err := os.Mkdir(shim, 0700); err != nil {
		t.Fatal(err)
	}
	peer := "#!/bin/sh\ncase \"$*\" in *.sha256*) printf '%064d  binary\\n' 0;; *) exec " + curl + " \"$@\";; esac\n"
	if err := os.WriteFile(filepath.Join(shim, "curl"), []byte(peer), 0700); err != nil {
		t.Fatal(err)
	}
	cmd = exec.CommandContext(c.ctx, "sh", installer, "--name", "must-not-install")
	cmd.Env = append(c.env, "PATH="+shim+":"+os.Getenv("PATH"), "PROTECTORATE_ENROLL_KEY=e2e-invalid")
	out, err = cmd.CombinedOutput()
	if err == nil || !strings.Contains(string(out), "not running it") {
		t.Fatalf("installer accepted corrupt digest: %v %s", err, out)
	}
	binary := get("/dl/" + target)
	digest := get("/dl/" + target + ".sha256")
	sum := sha256.Sum256(binary)
	if strings.Fields(string(digest))[0] != fmt.Sprintf("%x", sum) {
		t.Fatal("download checksum does not match binary")
	}
	staged := filepath.Join(c.root, "staged-binary")
	if err := os.WriteFile(staged, binary, 0700); err != nil {
		t.Fatal(err)
	}
	c.cli("node", "put", "peer", staged, "candidate")
	var rejection map[string]any
	if status := c.rpc("NodeService", "ApplyUpdate", map[string]any{"name": "peer", "path": "candidate", "size": len(binary), "sha256": strings.Repeat("0", 64)}, &rejection); status != 400 || rejection["code"] != "failed_precondition" {
		t.Fatalf("bad update digest status=%d", status)
	}
	c.cli("node", "update", "peer", "--force", "--no-wait")
	restart := func() {
		c.eventually(func() bool {
			select {
			case <-c.node.done:
				return true
			default:
				return false
			}
		})
		nodeCluster := *c
		nodeCluster.binary = filepath.Join(c.root, ".local", "bin", "node-protectorate")
		c.node = nodeCluster.start("node", append(c.env, "PROTECTORATE_HOME="+filepath.Join(c.root, "node"), "INVOCATION_ID=e2e"), "node", "serve", "--listen", strings.TrimPrefix(c.nodeURL, "http://"))
	}
	restart()
	c.eventually(func() bool {
		b, err := os.ReadFile(filepath.Join(c.root, ".local", "bin", "node-protectorate.update"))
		return err == nil && strings.Contains(string(b), `"confirmed"`)
	})
	marker := filepath.Join(c.root, ".local", "bin", "node-protectorate.update")
	b, err := os.ReadFile(marker)
	if err != nil {
		t.Fatal(err)
	}
	var state map[string]any
	if err := json.Unmarshal(b, &state); err != nil {
		t.Fatal(err)
	}
	c.stop(c.node)
	state["phase"] = "trial"
	state["attempts"] = 1
	state["deadline"] = time.Now().Add(-time.Minute).UTC().Format(time.RFC3339Nano)
	b, err = json.Marshal(state)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(marker, b, 0600); err != nil {
		t.Fatal(err)
	}
	restart()
	c.eventually(func() bool {
		b, err := os.ReadFile(marker)
		return err == nil && strings.Contains(string(b), `"rolledback"`)
	})
	restart()
	c.eventually(func() bool {
		return strings.Contains(fmt.Sprint(c.call("NodeService", "GetNode", map[string]any{"name": "peer"})), "NODE_FRESHNESS_FRESH")
	})
	job := c.call("JobService", "SubmitJob", map[string]any{"target": map[string]any{"node": "peer"}, "command": []string{"true"}})
	c.eventually(func() bool {
		return c.call("JobService", "GetJob", map[string]any{"id": job["id"]})["state"] == "JOB_STATE_SUCCEEDED"
	})
}
