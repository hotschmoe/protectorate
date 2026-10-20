package e2e

import (
	"archive/tar"
	"bytes"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/hotschmoe/protectorate/internal/testenv"
)

func TestHubInstallerVerifiesFleetBeforeInstalling(t *testing.T) {
	binary := testenv.E2E(t)
	if runtime.GOOS != "linux" {
		t.Skip("hub installer requires Linux")
	}
	const version = "v0.4.2-1-gabcdef0"
	var build []byte
	if binary == "" {
		build = buildReleaseCandidate(t, "", version)
	} else {
		var err error
		build, err = os.ReadFile(binary)
		if err != nil {
			t.Fatal(err)
		}
	}
	assets := map[string][]byte{"VERSION": []byte(version + "\n")}
	for _, platform := range []string{"linux-amd64", "linux-arm64", "windows-amd64.exe", "windows-arm64.exe", "android-arm64"} {
		assets["protectorate-"+platform] = build
	}
	var web bytes.Buffer
	gz := gzip.NewWriter(&web)
	tw := tar.NewWriter(gz)
	index := []byte("verified release web app")
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
	assets["protectorate-web-"+version+".tar.gz"] = web.Bytes()
	var sums strings.Builder
	for name, data := range assets {
		fmt.Fprintf(&sums, "%x  %s\n", sha256.Sum256(data), name)
	}
	assets["SHA256SUMS"] = []byte(sums.String())
	installer, err := filepath.Abs("../../install-hub.sh")
	if err != nil {
		t.Fatal(err)
	}
	for _, tc := range []struct {
		name    string
		channel string
		corrupt bool
	}{
		{name: "stable", channel: "stable"},
		{name: "dev", channel: "dev"},
		{name: "corrupt fleet asset", channel: "stable", corrupt: true},
	} {
		t.Run(tc.name, func(t *testing.T) {
			root := t.TempDir()
			var mu sync.Mutex
			seen := map[string]bool{}
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				name := strings.TrimPrefix(r.URL.Path, "/")
				data, ok := assets[name]
				if !ok {
					http.NotFound(w, r)
					return
				}
				mu.Lock()
				seen[name] = true
				mu.Unlock()
				if tc.corrupt && name == "protectorate-android-arm64" {
					data = []byte("untrusted development build")
				}
				_, _ = w.Write(data)
			}))
			defer server.Close()
			ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
			defer cancel()
			cmd := exec.CommandContext(ctx, "bash", installer, "--dry-run", "--base-url", server.URL, "--listen", "127.0.0.1:7470", "--channel", tc.channel)
			cmd.Env = append(os.Environ(), "HOME="+root, "XDG_DATA_HOME="+filepath.Join(root, "data"), "TMPDIR="+root)
			out, runErr := cmd.CombinedOutput()
			if tc.corrupt {
				if runErr == nil || !strings.Contains(string(out), "checksum mismatch for protectorate-android-arm64") || strings.Contains(string(out), "would run: install") {
					t.Fatalf("installer did not refuse the fleet before installing: %v\n%s", runErr, out)
				}
			} else {
				if runErr != nil || !strings.Contains(string(out), filepath.Join(root, "data", "protectorate", "releases", version, "web")) || !strings.Contains(string(out), "--update-channel "+tc.channel) {
					t.Fatalf("installer did not prepare the release layout: %v\n%s", runErr, out)
				}
				mu.Lock()
				for asset := range assets {
					if !seen[asset] {
						t.Errorf("installer skipped asset %s", asset)
					}
				}
				mu.Unlock()
			}
			if _, err := os.Stat(filepath.Join(root, "data")); !os.IsNotExist(err) {
				t.Fatalf("dry run wrote the installation directory: %v", err)
			}
		})
	}
}
