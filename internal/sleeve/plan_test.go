package sleeve

import (
	"errors"
	"flag"
	"os"
	"path/filepath"
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

func TestPlanGolden(t *testing.T) {
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
