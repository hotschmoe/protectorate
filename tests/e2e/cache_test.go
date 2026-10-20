//go:build linux

package e2e

import (
	"crypto/sha256"
	"fmt"
	"io"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestBinarySleeveCaches(t *testing.T) {
	for _, mode := range []string{"overlay", "copy"} {
		t.Run(mode, func(t *testing.T) { sleeveCacheScenario(t, mode) })
	}
}

func sleeveCacheScenario(t *testing.T, mode string) {
	c := newCluster(t)
	c.stop(c.hub)
	bwrap, err := exec.LookPath("bwrap")
	if err != nil {
		t.Skipf("cache scenario requires bwrap: %v", err)
	}
	if mode == "copy" {
		bwrap = "/usr/bin/bwrap"
	}
	help, err := exec.CommandContext(c.ctx, bwrap, "--help").Output()
	if err != nil {
		t.Skipf("bwrap unavailable: %v", err)
	}
	wantOverlay := mode == "overlay"
	if strings.Contains(string(help), "--overlay-src ") != wantOverlay {
		t.Skipf("%s requires bwrap overlay support=%t", mode, wantOverlay)
	}
	pathDir := filepath.Join(c.root, "bwrap-bin")
	if err := os.Mkdir(pathDir, 0700); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(bwrap, filepath.Join(pathDir, "bwrap")); err != nil {
		t.Fatal(err)
	}
	c.env = append(c.env, "PATH="+pathDir+":"+os.Getenv("PATH"))
	goRoot, err := exec.CommandContext(c.ctx, "go", "env", "GOROOT").Output()
	if err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(strings.TrimSpace(string(goRoot)), filepath.Join(c.root, ".local/go")); err != nil {
		t.Fatal(err)
	}
	flutter := filepath.Join(c.root, "development/flutter")
	pub := filepath.Join(c.root, ".pub-cache")
	cache := filepath.Join(c.root, ".cache/go-build")
	ws := filepath.Join(c.root, "cache-workspace")
	for _, d := range []string{filepath.Join(flutter, "bin/cache"), filepath.Join(pub, "bin"), filepath.Join(pub, "global_packages"), filepath.Join(pub, "hosted"), filepath.Join(pub, "git"), cache, ws} {
		if err := os.MkdirAll(d, 0700); err != nil {
			t.Fatal(err)
		}
	}
	write := func(p string, data []byte) {
		t.Helper()
		if err := os.WriteFile(p, data, 0700); err != nil {
			t.Fatal(err)
		}
	}
	write(filepath.Join(flutter, "bin/flutter"), []byte("#!/bin/sh\nexit 0\n"))
	write(filepath.Join(flutter, "bin/cache/prewarmed"), make([]byte, 4<<20))
	write(filepath.Join(pub, "hosted/prewarmed"), make([]byte, 1<<20))
	write(filepath.Join(pub, "git/prewarmed"), []byte("host git cache\n"))
	write(filepath.Join(ws, "go.mod"), []byte("module example.invalid/cacheprobe\n\ngo 1.24\n"))
	write(filepath.Join(ws, "main.go"), []byte("package main\nfunc main() { println(\"cache reused\") }\n"))
	if mode == "copy" {
		write(filepath.Join(ws, "pubspec.yaml"), []byte("name: cache_probe\n"))
	}
	warm := exec.CommandContext(c.ctx, "go", "build", "-p=2", "-o", "host-probe", ".")
	warm.Dir, warm.Env = ws, append(c.env, "GOCACHE="+cache, "GOROOT="+filepath.Join(c.root, ".local/go"))
	if out, err := warm.CombinedOutput(); err != nil {
		t.Fatalf("warm owner cache: %v: %s", err, out)
	}
	badOutput := filepath.Join(cache, "00", strings.Repeat("0", 64)+"-d")
	write(badOutput, []byte("incomplete host cache write"))
	badExecutable := filepath.Join(cache, "01", strings.Repeat("1", 64)+"-d")
	if err := os.Mkdir(badExecutable, 0700); err != nil {
		t.Fatal(err)
	}
	write(filepath.Join(badExecutable, "executable"), []byte("incomplete cached executable"))
	before := cacheDigest(t, c.root, []string{flutter, pub, cache})
	c.boot()
	runtime := c.call("HubService", "GetSleeveRuntime", map[string]any{})
	if got, _ := runtime["overlays"].(bool); got != wantOverlay {
		t.Fatalf("runtime capabilities: %v", runtime)
	}
	if got, _ := runtime["bwrapPath"].(string); got != filepath.Join(pathDir, "bwrap") {
		t.Fatalf("hub did not pin PATH bwrap: %v", runtime)
	}
	if status := c.cli("status"); !strings.Contains(status, filepath.Join(pathDir, "bwrap")) || !strings.Contains(status, fmt.Sprint(runtime["bwrapVersion"])) {
		t.Fatalf("status omitted sleeve host capabilities: %s", status)
	}
	var lower, frozen, cacheEntry string
	if wantOverlay {
		c.eventually(func() bool {
			runtime = c.call("HubService", "GetSleeveRuntime", map[string]any{})
			lower, _ = runtime["goBuildLower"].(string)
			return lower != "" && runtime["snapshotPreparing"] != true
		})
		if _, err := os.Stat(filepath.Join(lower, "00", filepath.Base(badOutput))); !os.IsNotExist(err) {
			t.Fatalf("snapshot imported an unreferenced output: %v", err)
		}
		if _, err := os.Stat(filepath.Join(lower, "01", filepath.Base(badExecutable))); !os.IsNotExist(err) {
			t.Fatalf("snapshot imported an unreferenced executable directory: %v", err)
		}
		frozen = cacheDigest(t, c.home, []string{lower})
		changed := false
		if err := filepath.WalkDir(cache, func(p string, d fs.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if !changed && d.Type().IsRegular() && strings.HasSuffix(d.Name(), "-d") && p != badOutput {
				rel, err := filepath.Rel(cache, p)
				if err != nil {
					return err
				}
				if fi, err := os.Stat(filepath.Join(lower, rel)); os.IsNotExist(err) {
					return nil
				} else if err != nil {
					return err
				} else if source, err := os.Stat(p); err != nil || !os.SameFile(fi, source) {
					return fmt.Errorf("snapshot output is not hardlinked: %v", err)
				}
				changed, cacheEntry = true, rel
				return os.Remove(p)
			}
			return nil
		}); err != nil {
			t.Fatal(err)
		}
		if !changed {
			t.Fatal("owner warm cache contained no output to replace")
		}
		before = cacheDigest(t, c.root, []string{flutter, pub, cache})
		if cacheDigest(t, c.home, []string{lower}) != frozen {
			t.Fatal("owner trim changed the published Go lower")
		}
	}
	c.requireRuntime(t)
	for _, dir := range []string{cache, filepath.Join(flutter, "bin/cache"), flutter, pub} {
		if status := c.rpc("SleeveService", "SpawnSleeve", map[string]any{
			"name": "cache-escape", "harness": "HARNESS_SHELL", "workspace": dir,
			"args": []string{"true"},
		}, nil); status != 400 {
			t.Fatalf("owner cache writable workspace was not refused: %s: %d", dir, status)
		}
	}
	script := `set -eu
touch ready
printf private > "$FLUTTER_CACHE/prewarmed"
printf delta > "$FLUTTER_CACHE/created"
printf private > "$PUB_CACHE/hosted/prewarmed"
printf delta > "$PUB_CACHE/git/created"
mkdir -p "$GOCACHE"
printf private > "$GOCACHE/probe"
go build -p=2 -x -o inside-probe . 2>build.trace
./inside-probe >result 2>&1
if [ -n "$CACHE_ENTRY" ]; then printf poisoned > "$GOCACHE/$CACHE_ENTRY"; fi
touch verified
exec sleep 60
`
	spawn := func() {
		c.call("SleeveService", "SpawnSleeve", map[string]any{
			"name": "cache-probe", "harness": "HARNESS_SHELL", "workspace": ws,
			"args":   []string{"bash", "-c", script},
			"limits": map[string]any{"cpuPercent": 200},
			"env":    map[string]string{"FLUTTER_CACHE": filepath.Join(flutter, "bin/cache"), "GOCACHE": filepath.Join(c.root, ".cache/go-build"), "GOROOT": filepath.Join(c.root, ".local/go"), "CACHE_ENTRY": cacheEntry},
		})
	}
	start := time.Now()
	spawn()
	t.Cleanup(func() { c.rpc("SleeveService", "KillSleeve", map[string]any{"name": "cache-probe"}, nil) })
	c.eventually(func() bool { _, err := os.Stat(filepath.Join(ws, "ready")); return err == nil })
	if elapsed := time.Since(start); wantOverlay && elapsed >= time.Second {
		t.Fatalf("overlay spawn took %s, want under one second", elapsed)
	}
	c.eventually(func() bool { _, err := os.Stat(filepath.Join(ws, "verified")); return err == nil })
	trace, err := os.ReadFile(filepath.Join(ws, "build.trace"))
	if err != nil {
		t.Fatal(err)
	}
	if wantOverlay && strings.Contains(string(trace), "/compile ") {
		t.Fatalf("Go rebuilt warm host packages:\n%s", trace)
	}
	if data, err := os.ReadFile(filepath.Join(ws, "result")); err != nil || strings.TrimSpace(string(data)) != "cache reused" {
		t.Fatalf("cached build did not execute correctly: %q: %v", data, err)
	}
	if wantOverlay {
		t.Cleanup(func() { c.rpc("SleeveService", "KillSleeve", map[string]any{"name": "cache-reader"}, nil) })
		c.call("SleeveService", "SpawnSleeve", map[string]any{
			"name": "cache-reader", "harness": "HARNESS_SHELL", "workspace": ws,
			"args": []string{"bash", "-c", `set -eu
test ! -e "$FLUTTER_CACHE/created"
test ! -e "$PUB_CACHE/git/created"
test "$(sha256sum "$GOCACHE/$CACHE_ENTRY" | cut -d ' ' -f 1)" = "$CACHE_HASH"
touch reader-verified
exec sleep 60`},
			"env": map[string]string{"FLUTTER_CACHE": filepath.Join(flutter, "bin/cache"), "CACHE_ENTRY": cacheEntry, "CACHE_HASH": filepath.Base(cacheEntry)[:64]},
		})
		c.eventually(func() bool { _, err := os.Stat(filepath.Join(ws, "reader-verified")); return err == nil })
		c.call("SleeveService", "KillSleeve", map[string]any{"name": "cache-reader"})
	}
	c.call("SleeveService", "KillSleeve", map[string]any{"name": "cache-probe"})
	if after := cacheDigest(t, c.root, []string{flutter, pub, cache}); before != after {
		t.Fatal("sleeve changed owner cache bytes or paths")
	}
	if wantOverlay && cacheDigest(t, c.home, []string{lower}) != frozen {
		t.Fatal("sleeve changed the shared Go snapshot")
	}
	home := filepath.Join(c.home, "sleeves/cache-probe/home")
	if wantOverlay {
		upper := filepath.Join(home, ".cache/overlays/flutter/upper")
		data, err := os.ReadFile(filepath.Join(upper, "prewarmed"))
		if err != nil || string(data) != "private" {
			t.Fatalf("Flutter delta missing: %q: %v", data, err)
		}
		if data, err := os.ReadFile(filepath.Join(home, ".cache/overlays/go-build", filepath.Base(filepath.Dir(lower)), "upper", cacheEntry)); err != nil || string(data) != "poisoned" {
			t.Fatalf("Go cache poison did not stay in private upper: %q: %v", data, err)
		}
		var size int64
		if err := filepath.WalkDir(filepath.Join(home, ".cache/overlays"), func(p string, d fs.DirEntry, err error) error {
			if err == nil && d.Type().IsRegular() {
				fi, err := d.Info()
				if err != nil {
					return err
				}
				size += fi.Size()
			}
			return err
		}); err != nil {
			t.Fatal(err)
		}
		if size > 1<<20 {
			t.Fatalf("warm overlay copied %d bytes, want only a small delta", size)
		}
	} else {
		if _, err := os.Stat(filepath.Join(home, ".cache/flutter/created")); err != nil {
			t.Fatalf("copy fallback missing writable SDK copy: %v", err)
		}
	}
	script = `set -eu; test "$(cat "$FLUTTER_CACHE/created")" = delta; test "$(cat "$PUB_CACHE/git/created")" = delta; if [ -n "$CACHE_ENTRY" ]; then test "$(cat "$GOCACHE/$CACHE_ENTRY")" = poisoned; fi; touch respawned; exec sleep 60`
	spawn()
	c.eventually(func() bool { _, err := os.Stat(filepath.Join(ws, "respawned")); return err == nil })
	c.call("SleeveService", "KillSleeve", map[string]any{"name": "cache-probe"})
	if after := cacheDigest(t, c.root, []string{flutter, pub, cache}); before != after {
		t.Fatal("respawn changed owner cache bytes or paths")
	}
	if wantOverlay && cacheDigest(t, c.home, []string{lower}) != frozen {
		t.Fatal("respawn changed the shared Go snapshot")
	}
	cachePath := filepath.Join(home, ".cache/flutter")
	if wantOverlay {
		cachePath = filepath.Join(home, ".cache/overlays")
	}
	if err := os.Rename(cachePath, cachePath+".saved"); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(cache, cachePath); err != nil {
		t.Fatal(err)
	}
	if status := c.rpc("SleeveService", "SpawnSleeve", map[string]any{
		"name": "cache-probe", "harness": "HARNESS_SHELL", "workspace": ws,
		"args": []string{"true"},
	}, nil); status != 400 {
		t.Fatalf("cache source symlink escaped the sleeve home: HTTP %d", status)
	}
	if after := cacheDigest(t, c.root, []string{flutter, pub, cache}); before != after {
		t.Fatal("rejected cache symlink changed owner cache bytes or paths")
	}
}

func cacheDigest(t *testing.T, root string, dirs []string) string {
	t.Helper()
	h := sha256.New()
	for _, dir := range dirs {
		if err := filepath.WalkDir(dir, func(p string, d fs.DirEntry, err error) error {
			if err != nil {
				return err
			}
			rel, err := filepath.Rel(root, p)
			if err != nil {
				return err
			}
			fmt.Fprintln(h, rel, d.Type())
			if d.Type().IsRegular() {
				f, err := os.Open(p)
				if err != nil {
					return err
				}
				_, err = io.Copy(h, f)
				closeErr := f.Close()
				if err != nil {
					return err
				}
				return closeErr
			}
			return nil
		}); err != nil {
			t.Fatal(err)
		}
	}
	return fmt.Sprintf("%x", h.Sum(nil))
}
