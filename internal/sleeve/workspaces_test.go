package sleeve

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

func writeFile(t *testing.T, path, body string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestGitDetails(t *testing.T) {
	tests := []struct {
		name         string
		head, config string
		remote, ref  string
	}{
		{"branch and remote", "ref: refs/heads/main\n", "[core]\n\tbare = false\n[remote \"origin\"]\n\turl = https://github.com/a/b.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n", "https://github.com/a/b.git", "main"},
		{"token stripped", "ref: refs/heads/dev\n", "[remote \"origin\"]\n\turl = https://tok:x@github.com/a/b.git\n", "https://github.com/a/b.git", "dev"},
		{"scp url kept", "ref: refs/heads/x/y\n", "[remote \"origin\"]\n\turl = git@github.com:a/b.git\n", "git@github.com:a/b.git", "x/y"},
		{"ssh user kept", "ref: refs/heads/m\n", "[remote \"origin\"]\n\turl = ssh://git@host/a/b\n", "ssh://git@host/a/b", "m"},
		{"detached no remote", "0123456789abcdef\n", "[core]\n", "", ""},
		{"other remote only", "ref: refs/heads/m\n", "[remote \"up\"]\n\turl = https://h/x\n", "", "m"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			d := t.TempDir()
			writeFile(t, filepath.Join(d, ".git", "HEAD"), tt.head)
			writeFile(t, filepath.Join(d, ".git", "config"), tt.config)
			r, b := gitDetails(d)
			if r != tt.remote || b != tt.ref {
				t.Errorf("got (%q, %q), want (%q, %q)", r, b, tt.remote, tt.ref)
			}
		})
	}
	if r, b := gitDetails(t.TempDir()); r != "" || b != "" {
		t.Errorf("non-git directory reported (%q, %q)", r, b)
	}
}

func TestWorkspaces(t *testing.T) {
	linuxHost(t)
	base, err := filepath.EvalSymlinks(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	home := filepath.Join(base, "state")
	user := filepath.Join(base, "user")
	outside := filepath.Join(base, "outside", "proj")
	for _, d := range []string{filepath.Join(home, "workspaces", "alpha"), filepath.Join(home, "workspaces", "scratch"), filepath.Join(home, "workspaces", ".hidden"), outside, user} {
		if err := os.MkdirAll(d, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	writeFile(t, filepath.Join(home, "workspaces", "alpha", ".git", "HEAD"), "ref: refs/heads/main\n")
	writeFile(t, filepath.Join(home, "workspaces", "alpha", ".git", "config"), "[remote \"origin\"]\n\turl = https://h/a.git\n")
	writeFile(t, filepath.Join(home, "workspaces", "file"), "not a directory")
	if err := os.Symlink(user, filepath.Join(home, "workspaces", "home-link")); err != nil {
		t.Fatal(err)
	}
	cfg := Config{Home: home, UserHome: user, Protected: []string{filepath.Join(user, ".ssh")}}
	m := &Manager{cfg: cfg, sleeves: map[string]*entry{
		"alpha":  {Info: Info{Name: "alpha", Workspace: filepath.Join(home, "workspaces", "alpha"), State: Running}},
		"other":  {Info: Info{Name: "other", Workspace: outside, State: Exited}},
		"gone":   {Info: Info{Name: "gone", Workspace: filepath.Join(base, "missing"), State: Running}},
		"helper": {Info: Info{Name: "helper", Workspace: outside, Helper: "handoff", State: Running}},
		"poe":    {Info: Info{Name: "poe", Workspace: filepath.Join(home, "orchestrator"), State: Running}},
	}}
	got := m.Workspaces()
	want := []Workspace{
		{Name: "alpha", Path: filepath.Join(home, "workspaces", "alpha"), Remote: "https://h/a.git", Branch: "main", Sleeve: "alpha", Busy: true, RequiredName: "alpha"},
		{Name: "proj", Path: outside, Sleeve: "other"},
		{Name: "scratch", Path: filepath.Join(home, "workspaces", "scratch"), RequiredName: "scratch"},
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("got  %+v\nwant %+v", got, want)
	}
}
