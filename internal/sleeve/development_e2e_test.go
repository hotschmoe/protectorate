//go:build linux

package sleeve

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestDevelopmentToolchainsEndToEnd(t *testing.T) {
	r := newRig(t)
	r.m.mu.Lock()
	r.m.cfg.GitName, r.m.cfg.GitEmail = "Runtime E2E", "runtime@example.invalid"
	r.cfg = r.m.cfg
	r.m.mu.Unlock()
	name := r.pfx + "development"
	ws := r.workspace(t, "development")
	flutter := false
	for _, d := range r.cfg.ToolDirs {
		if _, err := os.Stat(filepath.Join(d, "bin/flutter")); err == nil {
			flutter = true
		}
	}
	script := `set -eu
test "$GOMAXPROCS" = 1
test "$GOFLAGS" = -p=1
test "$FLUTTER_TEST_CONCURRENCY" = 1
mkdir -p "$PUB_CACHE" "$GOMODCACHE"
touch "$PUB_CACHE/writable" "$GOMODCACHE/writable"
if command -v go >/dev/null; then
  printf 'module example.invalid/runtime\n\ngo 1.24\n' > go.mod
  printf 'package main\nfunc main() { println("runtime ready") }\n' > main.go
  go build -o go-probe .
  ./go-probe
fi
git init -q
git add result.txt
git commit -qm 'Development environment'
test "$(git log -1 --format=%ae)" = runtime@example.invalid
`
	if flutter {
		script += `flutter --version
flutter create --no-pub --platforms=linux environment_probe
printf 'void main() { print("runtime ready"); }\n' > tool_probe.dart
dart compile exe tool_probe.dart -o dart-probe
./dart-probe
if command -v buf >/dev/null && command -v protoc-gen-dart >/dev/null; then
  mkdir schema
  printf 'syntax = "proto3"; package runtime; message Probe { string value = 1; }\n' > schema/probe.proto
  buf generate schema --template '{"version":"v2","plugins":[{"local":"protoc-gen-dart","out":"generated"}]}'
  test -f generated/probe.pb.dart
fi
`
	}
	script += "touch verified\nexec sleep 600\n"
	if err := os.WriteFile(filepath.Join(ws, "result.txt"), []byte("development\n"), 0600); err != nil {
		t.Fatal(err)
	}
	// Only Dart workspaces get a writable copy of the SDK and Pub caches.
	if err := os.WriteFile(filepath.Join(ws, "pubspec.yaml"), []byte("name: environment_probe\n"), 0600); err != nil {
		t.Fatal(err)
	}
	_, err := r.m.Spawn(context.Background(), SpawnRequest{
		Name: name, Workspace: ws, Harness: Shell, Args: []string{"bash", "-c", script},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = r.m.Kill(context.Background(), name) })
	deadline := time.Now().Add(3 * time.Minute)
	for {
		if _, err := os.Stat(filepath.Join(ws, "verified")); err == nil {
			break
		}
		info, err := r.m.Get(name)
		if err != nil {
			t.Fatal(err)
		}
		if info.State == Exited || time.Now().After(deadline) {
			tail, _ := r.sess.Capture(context.Background(), SessionName(name), 80)
			t.Fatalf("development toolchain probe did not complete: %s", tail)
		}
		time.Sleep(100 * time.Millisecond)
	}
	if !flutter {
		t.Log("Flutter SDK absent on host; Go, git, writable Pub and module caches exercised")
	}
	if r.cfg.GitName != "" && r.cfg.GitEmail != "" {
		b, err := os.ReadFile(filepath.Join(r.cfg.sleeveHome(name), ".gitconfig"))
		if err != nil || !strings.Contains(string(b), r.cfg.GitEmail) {
			t.Fatalf("sleeve-local git identity: %s, %v", b, err)
		}
	}
}
