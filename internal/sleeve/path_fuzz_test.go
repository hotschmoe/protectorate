package sleeve

import (
	"path/filepath"
	"strings"
	"testing"
)

func FuzzWorkspaceBoundary(f *testing.F) {
	for _, seed := range []string{"/state/workspaces/alice", "/state/workspaces/bob", "/state/workspaces/alice/../bob", "/home/user/.config", "/home/user/.config-backup", "/work/project", "/", "relative", "/work/\x00secret"} {
		f.Add(seed)
	}
	f.Fuzz(func(t *testing.T, path string) {
		c := Config{Home: "/state", UserHome: "/home/user"}
		err := c.CheckWorkspace("alice", path)
		if err != nil {
			return
		}
		if !filepath.IsAbs(path) || filepath.Clean(path) != path {
			t.Fatalf("accepted noncanonical path %q", path)
		}
		rel, e := filepath.Rel(c.Home, path)
		if e == nil && rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
			own, e := filepath.Rel("/state/workspaces/alice", path)
			if e != nil || own == ".." || strings.HasPrefix(own, ".."+string(filepath.Separator)) {
				t.Fatalf("accepted another sleeve's state: %q", path)
			}
		}
		for _, protected := range []string{"/home/user/.config", "/home/user/.ssh", "/home/user/.cargo"} {
			rel, e := filepath.Rel(protected, path)
			if e == nil && rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
				t.Fatalf("accepted credential path %q", path)
			}
		}
	})
}

func FuzzEnvironmentArgumentEncoding(f *testing.F) {
	f.Add("TOKEN", "secret with spaces\nquotes'\"=$()")
	f.Add("UNICODE", "a\x00b")
	f.Fuzz(func(t *testing.T, key, value string) {
		if key == "" || strings.ContainsAny(key, "\x00=") || strings.ContainsRune(value, 0) {
			t.Skip("NUL and equals in keys are outside the process environment format")
		}
		got := strings.Split(string(SetenvArgs([][2]string{{key, value}})), "\x00")
		if len(got) != 4 || got[0] != "--setenv" || got[1] != key || got[2] != value || got[3] != "" {
			t.Fatalf("environment did not roundtrip: %q", got)
		}
	})
}
