package e2e

import (
	"archive/tar"
	"bytes"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"debug/buildinfo"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sort"
	"strings"
	"sync"
	"syscall"
	"testing"
	"time"

	"github.com/hotschmoe/protectorate/internal/testenv"
)

type releaseFixture struct {
	version string
	assets  map[string][]byte
}

type releaseServer struct {
	mu          sync.Mutex
	server      *httptest.Server
	releases    map[string]releaseFixture
	stable, dev string
	corrupt     bool
	requests    map[string]int
}

func newReleaseServer(t *testing.T) *releaseServer {
	t.Helper()
	s := &releaseServer{releases: make(map[string]releaseFixture), requests: make(map[string]int)}
	s.server = httptest.NewServer(http.HandlerFunc(s.serveHTTP))
	t.Cleanup(s.server.Close)
	return s
}

// set changes the served releases under the lock the handler holds.
func (s *releaseServer) set(change func()) {
	s.mu.Lock()
	defer s.mu.Unlock()
	change()
}

func (s *releaseServer) serveHTTP(w http.ResponseWriter, r *http.Request) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.requests[r.URL.Path]++
	const prefix = "/repos/e2e/protectorate/releases/"
	if strings.HasPrefix(r.URL.Path, prefix) {
		tag := strings.TrimPrefix(r.URL.Path, prefix)
		switch tag {
		case "latest":
			tag = s.stable
		case "tags/dev":
			tag = s.dev
		default:
			tag = strings.TrimPrefix(tag, "tags/")
		}
		release, ok := s.releases[tag]
		if !ok {
			http.NotFound(w, r)
			return
		}
		etag := `"` + release.version + `"`
		w.Header().Set("ETag", etag)
		if r.Header.Get("If-None-Match") == etag {
			s.requests["conditional:"+r.URL.Path]++
			w.WriteHeader(http.StatusNotModified)
			return
		}
		assets := make([]map[string]string, 0, len(release.assets))
		for name := range release.assets {
			assets = append(assets, map[string]string{"name": name, "browser_download_url": s.server.URL + "/assets/" + tag + "/" + name})
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{"tag_name": tag, "name": release.version, "html_url": s.server.URL + "/notes/" + tag, "prerelease": tag == s.dev, "assets": assets})
		return
	}
	parts := strings.SplitN(strings.TrimPrefix(r.URL.Path, "/assets/"), "/", 2)
	if len(parts) != 2 {
		http.NotFound(w, r)
		return
	}
	asset, ok := s.releases[parts[0]].assets[parts[1]]
	if !ok {
		http.NotFound(w, r)
		return
	}
	if s.corrupt && strings.HasPrefix(parts[1], "protectorate-") && !strings.HasSuffix(parts[1], ".tar.gz") {
		_, _ = w.Write([]byte("checksum mismatch must never become an executable"))
		return
	}
	_, _ = w.Write(asset)
}

func (s *releaseServer) add(t *testing.T, tag, version string, binary []byte) {
	t.Helper()
	assets := map[string][]byte{"VERSION": []byte(version + "\n")}
	for _, name := range []string{"protectorate-linux-amd64", "protectorate-linux-arm64", "protectorate-windows-amd64.exe", "protectorate-windows-arm64.exe", "protectorate-android-arm64"} {
		assets[name] = binary
	}
	var archive bytes.Buffer
	gz := gzip.NewWriter(&archive)
	tw := tar.NewWriter(gz)
	index := []byte("release " + version)
	if err := tw.WriteHeader(&tar.Header{Name: "index.html", Mode: 0644, Size: int64(len(index))}); err != nil {
		t.Fatal(err)
	}
	if _, err := tw.Write(index); err != nil {
		t.Fatal(err)
	}
	if err := tw.Close(); err != nil {
		t.Fatal(err)
	}
	if err := gz.Close(); err != nil {
		t.Fatal(err)
	}
	assets["protectorate-web-"+version+".tar.gz"] = archive.Bytes()
	var sums strings.Builder
	for name, data := range assets {
		fmt.Fprintf(&sums, "%x  %s\n", sha256.Sum256(data), name)
	}
	assets["SHA256SUMS"] = []byte(sums.String())
	s.mu.Lock()
	s.releases[tag] = releaseFixture{version: version, assets: assets}
	s.mu.Unlock()
}

func buildReleaseCandidate(t *testing.T, selected, version string) []byte {
	t.Helper()
	ctx, cancel := context.WithTimeout(t.Context(), 3*time.Minute)
	defer cancel()
	target := filepath.Join(t.TempDir(), "release-candidate")
	args := []string{"build", "-p", "2", "-trimpath", "-ldflags", "-s -w -X main.version=" + version, "-o", target}
	buildEnv := os.Environ()
	if selected != "" {
		info, err := buildinfo.ReadFile(selected)
		if err != nil {
			t.Fatal(err)
		}
		for _, setting := range info.Settings {
			if setting.Key == "-tags" {
				args = append(args, "-tags", setting.Value)
			}
			if setting.Key == "-race" && setting.Value == "true" {
				args = append(args, "-race")
			}
			if setting.Key == "CGO_ENABLED" {
				buildEnv = append(buildEnv, "CGO_ENABLED="+setting.Value)
			}
		}
	}
	args = append(args, "../../cmd/protectorate")
	cmd := exec.CommandContext(ctx, "go", args...)
	cmd.Env = buildEnv
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("build release candidate: %v\n%s", err, out)
	}
	b, err := os.ReadFile(target)
	if err != nil {
		t.Fatal(err)
	}
	return b
}

func TestBinaryGitHubReleaseUpdates(t *testing.T) {
	selected := testenv.E2E(t)
	if runtime.GOOS != "linux" {
		t.Skip("hub self-update requires Linux")
	}
	const next = "v99.0.1"
	const broken = "v99.0.2"
	const originalTag = "v98.0.0"
	candidate := buildReleaseCandidate(t, selected, next)
	c := newCluster(t)
	original, err := os.ReadFile(c.binary)
	if err != nil {
		t.Fatal(err)
	}
	initial := c.call("HubService", "GetStatus", map[string]any{})["version"].(string)
	server := newReleaseServer(t)
	server.add(t, next, next, candidate)
	server.add(t, originalTag, initial, original)
	bad := fmt.Sprintf("#!/bin/sh\nif [ \"$1\" = version ]; then printf '%%s\\n' '{\"version\":\"%s\",\"os\":\"%s\",\"arch\":\"%s\"}'; exit 0; fi\nexit 42\n", broken, runtime.GOOS, runtime.GOARCH)
	server.add(t, "dev", broken, []byte(bad))
	server.set(func() { server.stable, server.dev = next, "dev" })
	releases := filepath.Join(c.root, "releases")
	c.stop(c.hub)
	c.env = append(c.env, "INVOCATION_ID=inherited-session")
	c.boot("--release-repo", "e2e/protectorate", "--release-api-url", server.server.URL, "--release-dir", releases, "--hub-update-timeout", "3s", "--binary-dir", "")
	pid := c.hub.cmd.Process.Pid
	arguments, err := os.ReadFile(fmt.Sprintf("/proc/%d/cmdline", pid))
	if err != nil {
		t.Fatal(err)
	}
	check := func(channel string) map[string]any {
		t.Helper()
		return c.call("ReleaseService", "CheckRelease", map[string]any{"channel": channel})
	}
	for _, channel := range []struct{ name, want string }{{"stable", next}, {"dev", broken}} {
		t.Run("channel-"+channel.name, func(t *testing.T) {
			if got := fmt.Sprint(check(channel.name)); !strings.Contains(got, channel.want) {
				t.Fatalf("channel %s did not discover %s: %s", channel.name, channel.want, got)
			}
		})
	}
	check("stable")
	status := c.call("HubService", "GetStatus", map[string]any{})
	if status["availableVersion"] != next || status["updateChannel"] != "stable" || status["releaseNotesUrl"] != server.server.URL+"/notes/"+next {
		t.Fatalf("hub did not expose its release availability: %v", status)
	}
	server.set(func() { server.corrupt = true })
	c.rejectedCLI("checksum", "hub", "update")
	if got := c.call("HubService", "GetStatus", map[string]any{})["version"]; got != initial {
		t.Fatalf("corrupt release changed running hub: %v", got)
	}
	if executable, err := os.ReadFile(c.binary); err != nil || !bytes.Equal(executable, original) {
		t.Fatalf("corrupt release replaced the installed hub: bytes=%d error=%v", len(executable), err)
	}
	if _, err := os.Stat(filepath.Join(releases, "current")); !os.IsNotExist(err) {
		t.Fatalf("corrupt release activated a current pointer: %v", err)
	}
	server.set(func() { server.corrupt = false })
	c.cli("hub", "update")
	c.eventually(func() bool {
		var status map[string]any
		return c.rpc("HubService", "GetStatus", map[string]any{}, &status) == 200 && status["version"] == next
	})
	if after, err := os.ReadFile(fmt.Sprintf("/proc/%d/cmdline", pid)); err != nil || !bytes.Equal(arguments, after) {
		t.Fatalf("hub restart changed invocation: before=%q after=%q error=%v", arguments, after, err)
	}
	var healthyGuard struct {
		Phase    string `json:"phase"`
		GuardPID int    `json:"guard_pid"`
	}
	c.eventually(func() bool {
		journal, err := os.ReadFile(filepath.Join(c.home, "hub-update.json"))
		return err == nil && json.Unmarshal(journal, &healthyGuard) == nil && healthyGuard.Phase == "confirmed" && healthyGuard.GuardPID > 0
	})
	c.eventually(func() bool {
		_, err := os.Stat(fmt.Sprintf("/proc/%d", healthyGuard.GuardPID))
		return os.IsNotExist(err)
	})
	assertCurrent := func(version string) {
		t.Helper()
		current, err := filepath.EvalSymlinks(filepath.Join(releases, "current"))
		if err != nil || filepath.Base(current) != version {
			t.Fatalf("current release: %s error=%v, want %s", current, err, version)
		}
		for _, asset := range []string{"VERSION", "protectorate-linux-amd64", "protectorate-linux-arm64", "protectorate-windows-amd64.exe", "protectorate-windows-arm64.exe", "protectorate-android-arm64", "web/index.html"} {
			if _, err := os.Stat(filepath.Join(current, asset)); err != nil {
				t.Fatalf("release missing %s: %v", asset, err)
			}
		}
	}
	assertCurrent(next)
	for _, endpoint := range []struct {
		path string
		want []byte
	}{{"/", []byte("release " + next)}, {"/dl/protectorate-" + runtime.GOOS + "-" + runtime.GOARCH, candidate}} {
		req, err := http.NewRequestWithContext(c.ctx, http.MethodGet, c.url+endpoint.path, nil)
		if err != nil {
			t.Fatal(err)
		}
		res, err := c.client.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		body, err := io.ReadAll(res.Body)
		res.Body.Close()
		if err != nil || res.StatusCode != 200 || !bytes.Equal(body, endpoint.want) {
			t.Fatalf("current release endpoint %s: status=%d bytes=%d error=%v", endpoint.path, res.StatusCode, len(body), err)
		}
	}
	c.cli("node", "update", "--all", "--no-wait")
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
	c.node = nodeCluster.start("release-node", append(c.env, "PROTECTORATE_HOME="+filepath.Join(c.root, "node"), "INVOCATION_ID=e2e"), "node", "serve", "--listen", strings.TrimPrefix(c.nodeURL, "http://"))
	c.eventually(func() bool {
		return strings.Contains(fmt.Sprint(c.call("NodeService", "GetNode", map[string]any{"name": "peer"})), next)
	})
	if got, err := os.ReadFile(nodeCluster.binary); err != nil || !bytes.Equal(got, candidate) {
		t.Fatalf("node did not install fetched release binary: bytes=%d error=%v", len(got), err)
	}
	c.rejectedCLI("rolled back", "hub", "update", "--channel", "dev")
	journal, err := os.ReadFile(filepath.Join(c.home, "hub-update.json"))
	if err != nil {
		t.Fatal(err)
	}
	var trial struct {
		GuardPID int `json:"guard_pid"`
	}
	if err := json.Unmarshal(journal, &trial); err != nil || trial.GuardPID <= 0 {
		t.Fatalf("update journal has no guardian PID: %s error=%v", journal, err)
	}
	t.Cleanup(func() {
		if process, err := os.FindProcess(trial.GuardPID); err == nil {
			_ = process.Signal(syscall.SIGTERM)
		}
	})
	c.eventually(func() bool {
		select {
		case <-c.hub.done:
			return true
		default:
			return false
		}
	})
	c.eventually(func() bool {
		var status map[string]any
		return c.rpc("HubService", "GetStatus", map[string]any{}, &status) == 200 && status["version"] == next
	})
	assertCurrent(next)
	if restored, err := os.ReadFile(c.binary); err != nil || !bytes.Equal(restored, candidate) {
		t.Fatalf("unhealthy hub did not restore previous binary: bytes=%d error=%v", len(restored), err)
	}
	c.cli("hub", "update", "--version", originalTag)
	c.eventually(func() bool {
		var status map[string]any
		return c.rpc("HubService", "GetStatus", map[string]any{}, &status) == 200 && status["version"] == initial
	})
	assertCurrent(initial)
	entries, err := os.ReadDir(releases)
	if err != nil {
		t.Fatal(err)
	}
	var retained []string
	for _, entry := range entries {
		if entry.IsDir() && !strings.HasPrefix(entry.Name(), ".") {
			retained = append(retained, entry.Name())
		}
	}
	sort.Strings(retained)
	if len(retained) != 2 {
		t.Fatalf("release retention: got %v, want exactly two versions", retained)
	}
	wantRetained := []string{initial, next}
	sort.Strings(wantRetained)
	if fmt.Sprint(retained) != fmt.Sprint(wantRetained) {
		t.Fatalf("release retention lost rollback generation: got %v, want %v", retained, wantRetained)
	}
	server.mu.Lock()
	defer server.mu.Unlock()
	for _, endpoint := range []string{"latest", "tags/dev", "tags/" + originalTag} {
		if server.requests["/repos/e2e/protectorate/releases/"+endpoint] == 0 {
			t.Errorf("GitHub channel endpoint was never requested: %s", endpoint)
		}
	}
	if server.requests["conditional:/repos/e2e/protectorate/releases/latest"] == 0 {
		t.Error("repeat stable check did not use the cached ETag")
	}
}
