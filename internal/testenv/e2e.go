package testenv

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

// TmuxDir keeps Unix socket paths short even when TMPDIR is disk-backed and deep.
func TmuxDir(t testing.TB) string {
	t.Helper()
	base := os.TempDir()
	if runtime.GOOS != "windows" {
		base = "/tmp"
	}
	dir, err := os.MkdirTemp(base, "prot-e2e-tmux-")
	if err != nil {
		t.Fatalf("create private tmux directory: %v", err)
	}
	t.Cleanup(func() {
		if err := os.RemoveAll(dir); err != nil {
			t.Errorf("remove private tmux directory: %v", err)
		}
	})
	return dir
}

// E2E gates real-process scenarios and isolates their hub identity.
// The optional returned binary is used by scenarios that launch the CLI.
func E2E(t testing.TB) string {
	t.Helper()
	if os.Getenv("PROT_E2E") != "1" {
		t.Skip("real-process scenarios: set PROT_E2E=1 (optionally PROT_E2E_BINARY=/absolute/protectorate)")
	}
	for _, key := range []string{"PROTECTORATE_TOKEN_FILE", "PROTECTORATE_HUB", "PROTECTORATE_TOKEN"} {
		t.Setenv(key, "")
		if err := os.Unsetenv(key); err != nil {
			t.Fatalf("clear inherited %s: %v", key, err)
		}
	}
	binary := os.Getenv("PROT_E2E_BINARY")
	if binary != "" {
		if !filepath.IsAbs(binary) {
			t.Fatal("PROT_E2E_BINARY must be an absolute path")
		}
		if info, err := os.Stat(binary); err != nil || info.IsDir() {
			t.Fatalf("PROT_E2E_BINARY must name an existing binary: %s: %v", binary, err)
		}
	}
	return binary
}

// TmuxDir keeps Unix socket paths short even with disk-backed test fixtures.
func TmuxDir(t testing.TB) string {
	t.Helper()
	root := "/tmp"
	if runtime.GOOS == "windows" {
		root = os.TempDir()
	}
	dir, err := os.MkdirTemp(root, "pt-tmux-")
	if err != nil {
		t.Fatalf("create private tmux directory: %v", err)
	}
	t.Cleanup(func() {
		if err := os.RemoveAll(dir); err != nil {
			t.Errorf("clean private tmux directory: %v", err)
		}
	})
	return dir
}
