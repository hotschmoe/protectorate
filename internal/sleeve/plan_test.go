package sleeve

import (
	"errors"
	"flag"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"runtime"
	"strings"
	"testing"
)

var update = flag.Bool("update", false, "rewrite testdata golden files")

func testConfig() Config {
	return Config{
		Home:       "/state",
		UserHome:   "/home/u",
		User:       "u",
		System:     []string{"--ro-bind", "/usr", "/usr", "--symlink", "usr/bin", "/bin", "--ro-bind", "/etc", "/etc"},
		ToolDirs:   []string{"/home/u/.local", "/home/u/.nvm"},
		PathDirs:   []string{"/home/u/.local/bin", "/usr/bin", "/bin"},
		TasksMax:   4096,
		CgroupRoot: "/cg",
	}
}

// linuxHost skips tests of host paths and permissions that the Linux-only
// sleeve runtime defines in POSIX terms.
func linuxHost(t *testing.T) {
	t.Helper()
	if runtime.GOOS != "linux" {
		t.Skip("sleeves run on Linux hosts only")
	}
}

func TestPlanGolden(t *testing.T) {
	linuxHost(t)
	tests := []struct {
		name string
		spec Spec
		bin  Binary
	}{
		{"claude", Spec{Name: "alice", Harness: Claude, Workspace: "/work/proj", Limits: Limits{CPUPercent: 200, MemoryBytes: 8 << 30},
			Env: map[string]string{"B": "2", "A": "1"}}, Binary{Path: "/home/u/.local/bin/claude"}},
		{"codex", Spec{Name: "bob", Harness: Codex, Workspace: "/work/x", Args: []string{"--model", "o3"}}, Binary{Path: "/home/u/.nvm/bin/codex"}},
		{"grok", Spec{Name: "carol", Harness: Grok, Workspace: "/work/x"}, Binary{Path: "/home/u/.local/bin/grok", RO: []string{"/home/u/.grok/downloads"}}},
		{"pi", Spec{Name: "dave", Harness: Pi, Workspace: "/work/x", Limits: Limits{MemoryBytes: 1 << 30}}, Binary{Path: "/home/u/.local/bin/pi"}},
		{"poe", Spec{Name: "poe", Harness: Claude, Workspace: "/state/orchestrator", Limits: Limits{CPUPercent: 200, MemoryBytes: 4 << 30},
			Env: map[string]string{"PROTECTORATE_HUB": "http://127.0.0.1:7470", "PROTECTORATE_TOKEN_FILE": "/state/orchestrator/.protectorate-token"}},
			Binary{Path: "/home/u/.local/bin/claude"}},
		{"shell", Spec{Name: "eve", Harness: Shell, Workspace: "/work/x", Args: []string{"sleep", "600"}}, Binary{Path: "/usr/bin/bash"}},
		{"shell-interactive", Spec{Name: "frank", Harness: Shell, Workspace: "/work/x"}, Binary{Path: "/usr/bin/bash"}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			l, err := testConfig().Plan(tt.spec, tt.bin)
			if err != nil {
				t.Fatal(err)
			}
			got := strings.Join(l.Argv, "\n") + "\n"
			golden := filepath.Join("testdata", tt.name+".argv")
			if *update {
				if err := os.WriteFile(golden, []byte(got), 0o644); err != nil {
					t.Fatal(err)
				}
			}
			want, err := os.ReadFile(golden)
			if err != nil {
				t.Fatal(err)
			}
			if got != string(want) {
				t.Errorf("argv differs from %s (run go test -update and review):\n%s", golden, got)
			}
			if l.Unit != "prot-sleeve-"+tt.spec.Name {
				t.Errorf("unit = %q", l.Unit)
			}
		})
	}
}

func TestPlanRejects(t *testing.T) {
	linuxHost(t)
	ok := Spec{Name: "a", Harness: Shell, Workspace: "/work/x"}
	tests := []struct {
		name string
		edit func(*Spec)
	}{
		{"empty name", func(s *Spec) { s.Name = "" }},
		{"uppercase name", func(s *Spec) { s.Name = "Alice" }},
		{"slash name", func(s *Spec) { s.Name = "a/b" }},
		{"long name", func(s *Spec) { s.Name = strings.Repeat("a", 41) }},
		{"small memory", func(s *Spec) { s.Limits.MemoryBytes = 1 << 20 }},
		{"root workspace", func(s *Spec) { s.Workspace = "/" }},
		{"usr workspace", func(s *Spec) { s.Workspace = "/usr/lib" }},
		{"home workspace", func(s *Spec) { s.Workspace = "/home/u" }},
		{"parent of home", func(s *Spec) { s.Workspace = "/home" }},
		{"state workspace", func(s *Spec) { s.Workspace = "/state" }},
		{"auth workspace", func(s *Spec) { s.Workspace = "/state/auth/claude" }},
		{"other sleeve home", func(s *Spec) { s.Workspace = "/state/sleeves/b/home" }},
		{"claude creds", func(s *Spec) { s.Workspace = "/home/u/.claude/projects" }},
		{"ssh", func(s *Spec) { s.Workspace = "/home/u/.ssh" }},
		{"relative workspace", func(s *Spec) { s.Workspace = "work/x" }},
		{"unclean workspace", func(s *Spec) { s.Workspace = "/work/../etc" }},
		{"reserved env", func(s *Spec) { s.Env = map[string]string{"HOME": "/x"} }},
		{"managed env", func(s *Spec) { s.Env = map[string]string{"CLAUDE_CONFIG_DIR": "/x"} }},
		{"bad env name", func(s *Spec) { s.Env = map[string]string{"A=B": "x"} }},
		{"nul arg", func(s *Spec) { s.Args = []string{"a\x00b"} }},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			s := ok
			tt.edit(&s)
			if _, err := testConfig().Plan(s, Binary{Path: "/usr/bin/bash"}); !errors.Is(err, ErrInvalid) {
				t.Errorf("err = %v, want ErrInvalid", err)
			}
		})
	}
	if _, err := testConfig().Plan(ok, Binary{Path: "/usr/bin/bash"}); err != nil {
		t.Errorf("valid spec rejected: %v", err)
	}
}

func TestWithin(t *testing.T) {
	tests := []struct {
		p, root string
		want    bool
	}{
		{"/a/b", "/a", true}, {"/a", "/a", true}, {"/ab", "/a", false}, {"/", "/a", false}, {"/a/..b", "/a", true},
	}
	for _, tt := range tests {
		if got := within(tt.p, tt.root); got != tt.want {
			t.Errorf("within(%q,%q) = %v", tt.p, tt.root, got)
		}
	}
}

func TestParseHarness(t *testing.T) {
	for _, s := range []string{"claude", "codex", "grok", "pi", "shell"} {
		if _, err := ParseHarness(s); err != nil {
			t.Errorf("%s: %v", s, err)
		}
	}
	if _, err := ParseHarness("vim"); !errors.Is(err, ErrInvalid) {
		t.Errorf("err = %v", err)
	}
}

func TestLocateFollowsSymlinkChain(t *testing.T) {
	linuxHost(t)
	root := t.TempDir()
	for _, d := range []string{"tools/bin", "other/bin", "other/dl"} {
		if err := os.MkdirAll(filepath.Join(root, d), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	real := filepath.Join(root, "other/dl/h-1.0")
	if err := os.WriteFile(real, []byte("#!/bin/sh\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(real, filepath.Join(root, "other/bin/grok")); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink("../../other/bin/grok", filepath.Join(root, "tools/bin/grok")); err != nil {
		t.Fatal(err)
	}
	c := testConfig()
	c.ToolDirs = []string{filepath.Join(root, "tools")}
	c.PathDirs = []string{filepath.Join(root, "tools/bin")}
	b, err := c.Locate(Grok)
	if err != nil {
		t.Fatal(err)
	}
	want := []string{filepath.Join(root, "other/bin"), filepath.Join(root, "other/dl")}
	if strings.Join(b.RO, ",") != strings.Join(want, ",") {
		t.Errorf("RO = %v, want %v", b.RO, want)
	}
	if _, err := c.Locate(Pi); !errors.Is(err, ErrUnavailable) {
		t.Errorf("missing harness err = %v", err)
	}
}

func TestAddBinaryDir(t *testing.T) {
	linuxHost(t)
	t.Run("unmounted dir is mounted and on PATH last", func(t *testing.T) {
		c := testConfig()
		if err := c.AddBinaryDir("/opt/prot/bin/protectorate"); err != nil {
			t.Fatal(err)
		}
		if !contains(c.ToolDirs, "/opt/prot/bin") {
			t.Errorf("ToolDirs = %v", c.ToolDirs)
		}
		if got := c.PathDirs[len(c.PathDirs)-1]; got != "/opt/prot/bin" {
			t.Errorf("PathDirs = %v", c.PathDirs)
		}
		l, err := c.Plan(Spec{Name: "a", Harness: Shell, Workspace: "/work/x"}, Binary{Path: "/usr/bin/bash"})
		if err != nil {
			t.Fatal(err)
		}
		args := strings.Join(l.Argv, " ")
		if !strings.Contains(args, "--ro-bind /opt/prot/bin /opt/prot/bin") || !strings.Contains(args, ":/opt/prot/bin") {
			t.Errorf("sandbox lacks the binary dir: %s", args)
		}
	})
	t.Run("already mounted dir is not mounted twice", func(t *testing.T) {
		c := testConfig()
		n := len(c.ToolDirs)
		if err := c.AddBinaryDir("/home/u/.local/bin/protectorate"); err != nil {
			t.Fatal(err)
		}
		if len(c.ToolDirs) != n || len(c.PathDirs) != 3 {
			t.Errorf("ToolDirs = %v PathDirs = %v", c.ToolDirs, c.PathDirs)
		}
	})
	t.Run("credential dirs and the user home are refused", func(t *testing.T) {
		for _, exe := range []string{"/home/u/protectorate", "/home/u/.ssh/protectorate", "/home/u/.claude/bin/protectorate"} {
			c := testConfig()
			if err := c.AddBinaryDir(exe); !errors.Is(err, ErrInvalid) {
				t.Errorf("%s: err = %v", exe, err)
			}
		}
	})
	t.Run("relative path", func(t *testing.T) {
		c := testConfig()
		if err := c.AddBinaryDir("bin/protectorate"); !errors.Is(err, ErrInvalid) {
			t.Errorf("err = %v", err)
		}
	})
}

func TestCheckWorkspaceStateOwnership(t *testing.T) {
	linuxHost(t)
	c := testConfig()
	tests := []struct {
		sleeve, ws string
		ok         bool
	}{
		{"poe", "/state/orchestrator", true},
		{"poe", "/state/orchestrator/sub", true},
		{"alice", "/state/orchestrator", false},
		{"alice", "/state/orchestrator/sub", false},
		{"alice", "/state/workspaces", false},
		{"poe", "/state/workspaces", false},
		{"alice", "/state/workspaces/alice", true},
		{"alice", "/state/workspaces/alice/repo", true},
		{"alice", "/state/workspaces/bob", false},
		{"alice", "/state/workspaces/alicia", false},
		{"alice", "/state/sleeves/alice", false},
		{"poe", "/state/sleeves/poe", false},
		{"alice", "/state/auth/claude", false},
		{"alice", "/state/token", false},
		{"alice", "/state", false},
		{"alice", "/", false},
		{"alice", "/home/u", false},
		{"alice", "/home/u/.codex/sessions", false},
		{"alice", "/work/x", true},
	}
	for _, tt := range tests {
		if err := c.CheckWorkspace(tt.sleeve, tt.ws); (err == nil) != tt.ok {
			t.Errorf("CheckWorkspace(%s, %s) = %v, want ok=%v", tt.sleeve, tt.ws, err, tt.ok)
		}
	}
}

func TestCheckWorkspaceProtectedTargets(t *testing.T) {
	linuxHost(t)
	c := testConfig()
	c.Protected = []string{"/mnt/secrets/codex"}
	for ws, ok := range map[string]bool{
		"/mnt/secrets/codex": false, "/mnt/secrets/codex/x": false, "/mnt/secrets": true,
	} {
		if err := c.CheckWorkspace("alice", ws); (err == nil) != ok {
			t.Errorf("CheckWorkspace(%s) = %v, want ok=%v", ws, err, ok)
		}
	}
}

func TestDefaultConfigCanonicalizesPaths(t *testing.T) {
	linuxHost(t)
	base := t.TempDir()
	real := filepath.Join(base, "mnt", "state")
	if err := os.MkdirAll(real, 0o700); err != nil {
		t.Fatal(err)
	}
	link := filepath.Join(base, "state-link")
	if err := os.Symlink(real, link); err != nil {
		t.Fatal(err)
	}
	c, err := DefaultConfig(link, nil)
	if err != nil {
		t.Fatal(err)
	}
	want, _ := filepath.EvalSymlinks(real)
	if c.Home != want {
		t.Fatalf("Home = %s, want %s", c.Home, want)
	}
	if err := c.CheckWorkspace("alice", want); err == nil {
		t.Error("the resolved state directory is a valid workspace")
	}
}

func TestDefaultToolDirsAreNarrow(t *testing.T) {
	linuxHost(t)
	uh := t.TempDir()
	for _, d := range []string{".local/bin", ".local/share/app", ".cargo/bin", ".cargo/registry", ".nvm"} {
		if err := os.MkdirAll(filepath.Join(uh, d), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	t.Setenv("HOME", uh)
	c, err := DefaultConfig(t.TempDir(), nil)
	if err != nil {
		t.Fatal(err)
	}
	r, _ := filepath.EvalSymlinks(uh)
	got := strings.Join(c.ToolDirs, " ")
	for _, bad := range []string{filepath.Join(r, ".local") + " ", filepath.Join(r, ".cargo") + " ", "share", "registry"} {
		if strings.Contains(got+" ", bad) {
			t.Errorf("ToolDirs %q contains %q", got, bad)
		}
	}
	for _, d := range []string{".local/bin", ".cargo/bin", ".nvm"} {
		if !contains(c.ToolDirs, filepath.Join(r, d)) {
			t.Errorf("ToolDirs %q lacks %s", got, d)
		}
	}
}

func TestPlanMasksCredentialFiles(t *testing.T) {
	linuxHost(t)
	uh := t.TempDir()
	if err := os.MkdirAll(filepath.Join(uh, ".cargo"), 0o755); err != nil {
		t.Fatal(err)
	}
	cred := filepath.Join(uh, ".cargo", "credentials.toml")
	if err := os.WriteFile(cred, nil, 0o600); err != nil {
		t.Fatal(err)
	}
	tests := []struct {
		name  string
		tools []string
		want  bool
	}{
		{"whole cargo dir mounted", []string{filepath.Join(uh, ".cargo")}, true},
		{"cargo bin only", []string{filepath.Join(uh, ".cargo", "bin")}, false},
		{"no tools", nil, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			c := testConfig()
			c.UserHome, c.ToolDirs = uh, tt.tools
			l, err := c.Plan(Spec{Name: "a", Harness: Shell, Workspace: "/work/x"}, Binary{Path: "/usr/bin/bash"})
			if err != nil {
				t.Fatal(err)
			}
			if got := strings.Contains(strings.Join(l.Argv, " "), "--ro-bind /dev/null "+cred); got != tt.want {
				t.Errorf("masked = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestLocateCredentialDirChains(t *testing.T) {
	linuxHost(t)
	uh := t.TempDir()
	write := func(rel string, mode os.FileMode) string {
		p := filepath.Join(uh, rel)
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte("#!/bin/sh\n"), mode); err != nil {
			t.Fatal(err)
		}
		return p
	}
	link := func(rel, target string) string {
		p := filepath.Join(uh, rel)
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.Symlink(target, p); err != nil {
			t.Fatal(err)
		}
		return p
	}
	grok := write(".grok/downloads/grok-1.0", 0o755)
	link(".grok/bin/grok", "../downloads/grok-1.0")
	link(".local/bin/grok", filepath.Join(uh, ".grok/bin/grok"))
	claude := write(".local/share/claude/versions/1", 0o755)
	link(".local/bin/claude", claude)
	write(".codex/auth.json", 0o755)
	link(".local/bin/codex", filepath.Join(uh, ".codex/auth.json"))
	write(".codex/plain", 0o644)
	link(".local/bin/pi", filepath.Join(uh, ".codex/plain"))

	tests := []struct {
		name    string
		bin     string
		wantErr bool
		ro      []string
		files   []string
		links   []Link
	}{
		{"grok via credential dir", "grok", false, nil, []string{grok}, []Link{
			{filepath.Join(uh, ".grok/bin/grok"), "../downloads/grok-1.0"}}},
		{"claude via share versions", "claude", false, []string{filepath.Dir(claude)}, nil, nil},
		{"credential file target", "codex", true, nil, nil, nil},
		{"non-executable target", "pi", true, nil, nil, nil},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			c := testConfig()
			c.UserHome, c.ToolDirs = uh, []string{filepath.Join(uh, ".local", "bin")}
			c.PathDirs = []string{filepath.Join(uh, ".local", "bin")}
			b, err := c.Locate(Harness(tt.bin))
			if tt.wantErr {
				if !errors.Is(err, ErrUnavailable) {
					t.Fatalf("err = %v", err)
				}
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			if !reflect.DeepEqual(b.RO, tt.ro) || !reflect.DeepEqual(b.Files, tt.files) || !reflect.DeepEqual(b.Symlinks, tt.links) {
				t.Fatalf("RO %v Files %v Symlinks %v", b.RO, b.Files, b.Symlinks)
			}
			l, err := c.Plan(Spec{Name: "a", Harness: Shell, Workspace: "/work/x"}, b)
			if err != nil {
				t.Fatal(err)
			}
			args := strings.Join(l.Argv, " ")
			for _, f := range tt.files {
				if !strings.Contains(args, "--ro-bind "+f+" "+f) {
					t.Errorf("no file bind for %s", f)
				}
			}
			for _, k := range tt.links {
				if !strings.Contains(args, "--symlink "+k.Target+" "+k.Path) {
					t.Errorf("no symlink for %s", k.Path)
				}
			}
			if strings.Contains(args, "--ro-bind "+filepath.Join(uh, ".grok")+" ") {
				t.Errorf("credential dir mounted: %s", args)
			}
		})
	}
}

func TestPlanKeepsEnvOutOfArgv(t *testing.T) {
	linuxHost(t)
	c := testConfig()
	c.Home = t.TempDir()
	spec := Spec{Name: "alice", Harness: Shell, Workspace: "/work/x", Env: map[string]string{"TOKEN": "s3cr3t-value", "A": "1"}}
	l, err := c.Plan(spec, Binary{Path: "/usr/bin/bash"})
	if err != nil {
		t.Fatal(err)
	}
	if joined := strings.Join(l.Argv, "\x00"); strings.Contains(joined, "s3cr3t-value") || strings.Contains(joined, "TOKEN") {
		t.Fatalf("env leaked into argv: %q", l.Argv)
	}
	want := "--setenv\x00A\x001\x00--setenv\x00TOKEN\x00s3cr3t-value\x00"
	if string(l.EnvArgs) != want {
		t.Fatalf("EnvArgs = %q, want %q", l.EnvArgs, want)
	}

	if err := c.writeEnvFile("alice", l); err != nil {
		t.Fatal(err)
	}
	fi, err := os.Stat(l.EnvFile)
	if err != nil || fi.Mode().Perm() != 0o600 {
		t.Fatalf("env file: %v, %v", fi, err)
	}

	out, err := exec.Command("sh", "-c", envShim, "sh", l.EnvFile, "sh", "-c", "cat <&3").Output()
	if err != nil || string(out) != want {
		t.Fatalf("shim read %q, %v", out, err)
	}

	if err := c.writeEnvFile("alice", Launch{}); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(l.EnvFile); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("stale env file kept: %v", err)
	}
	spec.Env = nil
	l, err = c.Plan(spec, Binary{Path: "/usr/bin/bash"})
	if err != nil || l.EnvFile != "" || l.Argv[0] != "systemd-run" {
		t.Fatalf("no-env launch = %v, %v", l, err)
	}
}
