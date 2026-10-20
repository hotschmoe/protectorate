//go:build linux

package sleeve

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/hotschmoe/protectorate/internal/testenv"
)

func hostGit(t *testing.T, dir string, args ...string) {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
	cmd.Env = append(os.Environ(), "GIT_CONFIG_GLOBAL=/dev/null", "GIT_CONFIG_NOSYSTEM=1",
		"GIT_AUTHOR_NAME=t", "GIT_AUTHOR_EMAIL=t@x", "GIT_COMMITTER_NAME=t", "GIT_COMMITTER_EMAIL=t@x")
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("git %v: %v: %s", args, err, out)
	}
}

// A repository config written by an agent names a clean filter that tries to
// leave a marker on the host; status still works and the marker never appears.
func TestGitRunsRepositoryProgramsInAJail(t *testing.T) {
	testenv.E2E(t)
	if _, err := exec.LookPath("bwrap"); err != nil {
		t.Skip("bwrap not installed")
	}
	cfg, err := DefaultConfig(t.TempDir(), nil)
	if err != nil {
		t.Fatal(err)
	}
	m := &Manager{cfg: cfg}
	ws := t.TempDir()
	marker := filepath.Join(t.TempDir(), "pwned")
	hostGit(t, ws, "init", "-q")
	if err := os.WriteFile(filepath.Join(ws, "a.txt"), []byte("one\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	hostGit(t, ws, "add", "a.txt")
	hostGit(t, ws, "commit", "-qm", "first")
	hostGit(t, ws, "config", "filter.evil.clean", "sh -c 'touch "+marker+" "+filepath.Join(ws, "inside")+"; cat'")
	if err := os.WriteFile(filepath.Join(ws, ".gitattributes"), []byte("* filter=evil\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ws, "a.txt"), []byte("two\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	tests := []struct {
		args []string
		want string
	}{
		{[]string{"status", "--short"}, "M a.txt"},
		{[]string{"diff", "--stat", "HEAD"}, "a.txt"},
		{[]string{"log", "--format=%s"}, "first"},
	}
	for _, tc := range tests {
		out, err := m.Git(context.Background(), "w", ws, tc.args...)
		if err != nil {
			t.Fatalf("%v: %v", tc.args, err)
		}
		if !strings.Contains(out, tc.want) {
			t.Errorf("%v = %q, want %q", tc.args, out, tc.want)
		}
	}
	for _, p := range []string{marker, filepath.Join(ws, "inside")} {
		if _, err := os.Stat(p); err == nil {
			t.Fatalf("the repository's filter wrote %s on the host", p)
		}
	}
	if _, err := m.Git(context.Background(), "w", cfg.UserHome, "status"); err == nil {
		t.Fatal("git ran on the user's home")
	}
}
