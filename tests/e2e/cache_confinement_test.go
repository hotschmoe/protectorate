package e2e

import (
	"context"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"testing"

	"github.com/hotschmoe/protectorate/internal/sleeve"
)

func TestBinarySleeveCacheConfinement(t *testing.T) {
	c := newCluster(t)
	if _, err := exec.LookPath("bwrap"); err != nil {
		t.Skip("cache isolation requires bwrap")
	}
	sdk := filepath.Join(c.root, "sdk")
	pub := filepath.Join(c.root, ".pub-cache")
	workspace := filepath.Join(c.root, "dart-project")
	for _, dir := range []string{filepath.Join(sdk, "bin", "cache"), filepath.Join(pub, "hosted"), filepath.Join(pub, "global_packages"), workspace} {
		if err := os.MkdirAll(dir, 0700); err != nil {
			t.Fatal(err)
		}
	}
	for path, body := range map[string]string{
		filepath.Join(sdk, "bin", "flutter"):         "fixture SDK",
		filepath.Join(sdk, "bin", "cache", "marker"): "private cache",
		filepath.Join(workspace, "pubspec.yaml"):     "name: isolation\n",
	} {
		if err := os.WriteFile(path, []byte(body), 0600); err != nil {
			t.Fatal(err)
		}
	}
	cfg := sleeve.Config{Home: c.home, UserHome: c.root, ToolDirs: []string{sdk, filepath.Join(pub, "global_packages")},
		System: sleeve.SystemMounts(), PathDirs: []string{"/usr/bin", "/bin"}, TasksMax: 32}
	t.Run("cancelled seed", func(t *testing.T) {
		launch, err := cfg.Plan(sleeve.Spec{Name: "cache-cancel", Harness: sleeve.Shell, Workspace: workspace}, sleeve.Binary{Path: "/bin/sh"})
		if err != nil {
			t.Fatal(err)
		}
		ctx, cancel := context.WithCancel(c.ctx)
		cancel()
		if err := launch.Layout.Apply(ctx); err == nil {
			t.Fatal("cancelled cache copy succeeded")
		}
		staging, err := filepath.Glob(filepath.Join(c.home, "sleeves", "cache-cancel", "home", ".cache", ".cache-seed-*"))
		if err != nil || len(staging) != 0 {
			t.Fatalf("cancelled cache copy left staging directories: %v: %v", staging, err)
		}
		if err := launch.Layout.Apply(c.ctx); err != nil {
			t.Fatalf("retry cache preparation after cancellation: %v", err)
		}
	})
	for _, tc := range []struct{ name, path string }{
		{"flutter", ".cache/flutter"},
		{"pub", ".cache/pub/hosted"},
		{"parent", ".cache"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			spec := sleeve.Spec{Name: "cache-" + tc.name, Harness: sleeve.Shell, Workspace: workspace,
				Args: []string{"sh", "-ec", `test "$(cat "$1/marker")" = 'private cache'
test ! -e "$1/token"
mv "$HOME/$3" "$HOME/saved-cache"
ln -s "$2" "$HOME/$3"`, "sh", filepath.Join(sdk, "bin", "cache"), c.home, tc.path}}
			launch, err := cfg.Plan(spec, sleeve.Binary{Path: "/bin/sh"})
			if err != nil {
				t.Fatal(err)
			}
			if err := launch.Layout.Apply(c.ctx); err != nil {
				t.Fatal(err)
			}
			// Exercise the production filesystem box without requiring cgroup delegation.
			i := slices.Index(launch.Argv, "bwrap")
			if i < 0 {
				t.Fatal("sleeve plan has no filesystem box")
			}
			cmd := exec.CommandContext(c.ctx, launch.Argv[i], launch.Argv[i+1:]...)
			if out, err := cmd.CombinedOutput(); err != nil {
				t.Fatalf("run cache isolation box: %v: %s", err, out)
			}
			if _, err := cfg.Plan(spec, sleeve.Binary{Path: "/bin/sh"}); !errors.Is(err, sleeve.ErrInvalid) {
				t.Fatalf("relaunch accepted a sleeve-written cache symlink: %v", err)
			}
			if err := launch.Layout.Apply(c.ctx); !errors.Is(err, sleeve.ErrInvalid) {
				t.Fatalf("layout followed a cache symlink changed after planning: %v", err)
			}
		})
	}
	owner, err := os.ReadFile(filepath.Join(c.home, "token"))
	if err != nil || string(owner) != c.token+"\n" {
		t.Fatalf("cache preparation changed the hub credential: %v", err)
	}
}
