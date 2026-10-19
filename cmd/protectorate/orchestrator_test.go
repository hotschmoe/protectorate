package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/hotschmoe/protectorate/internal/orchestrator"
)

func TestCommandDocsAndWorkflow(t *testing.T) {
	docs := commandDocs(newRoot())
	byPath := map[string]orchestrator.Command{}
	for _, d := range docs {
		byPath[d.Path] = d
	}
	for _, s := range orchestrator.Workflow {
		path := []string{}
		for _, f := range strings.Fields(s.Command) {
			if strings.HasPrefix(f, "-") {
				break
			}
			path = append(path, f)
		}
		if _, ok := byPath[strings.Join(path, " ")]; !ok {
			t.Errorf("workflow command %q is not in the CLI", s.Command)
		}
	}
	for _, want := range []string{"quota", "quota suggest", "sleeve spawn", "sleeve ls", "session capture", "backlog edit", "api call", "events"} {
		if _, ok := byPath[want]; !ok {
			t.Errorf("missing %q", want)
		}
	}
	for path := range orchestratorHidden {
		if _, ok := byPath[path]; ok {
			t.Errorf("%q must be left out", path)
		}
	}
	for _, d := range docs {
		if strings.HasPrefix(d.Path, "hub") || strings.HasPrefix(d.Path, "token") || strings.HasPrefix(d.Path, "orchestrator") {
			t.Errorf("owner-only command listed: %s", d.Path)
		}
	}
	sp := byPath["sleeve spawn"]
	var haveWorkspace bool
	for _, f := range sp.Flags {
		haveWorkspace = haveWorkspace || f.Name == "workspace"
		if f.Name == "hub" || f.Name == "json" {
			t.Errorf("global flag %s listed per command", f.Name)
		}
	}
	if !haveWorkspace {
		t.Errorf("sleeve spawn flags = %v", sp.Flags)
	}
}

func TestToolDocsMatchCatalog(t *testing.T) {
	tools, err := toolDocs()
	if err != nil {
		t.Fatal(err)
	}
	have := map[string]bool{}
	for _, tl := range tools {
		have[tl.Name] = true
	}
	for _, n := range []string{"quota_suggest", "sleeve_spawn_sleeve", "sleeve_kill_sleeve"} {
		if !have[n] {
			t.Errorf("tool %s missing", n)
		}
	}
}

func TestTokenFileEnv(t *testing.T) {
	t.Setenv(tokenEnv, "from-env")
	file := filepath.Join(t.TempDir(), "tok")
	if err := os.WriteFile(file, []byte("from-file\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	t.Setenv(tokenFileEnv, file)
	o := &opts{}
	if got, err := o.token(); err != nil || got != "from-file" {
		t.Errorf("env file must beat env token: %q, %v", got, err)
	}
	other := filepath.Join(t.TempDir(), "other")
	os.WriteFile(other, []byte("flag-file"), 0o600)
	o.tokenFile = other
	if got, _ := o.token(); got != "flag-file" {
		t.Errorf("flag must beat env: %q", got)
	}
}
