package sleeve

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/hotschmoe/protectorate/internal/driver"
)

func TestSelectionPersistence(t *testing.T) {
	home := t.TempDir()
	m := &Manager{cfg: Config{Home: home}, path: filepath.Join(home, "sleeves.json"), sleeves: map[string]*entry{
		"envoy": {Info: Info{Name: "envoy", Harness: Codex, Mode: ModeChat, Chat: &Chat{Model: "native", Effort: "low",
			Requested: driver.Selection{Model: "chosen", Effort: "xhigh"}, Defaults: driver.Selection{Model: "native", Effort: "low"}}}},
	}}
	if err := m.saveLocked(); err != nil {
		t.Fatal(err)
	}
	if _, err := m.SetChat("envoy", func(c *Chat) { c.Model = "chosen"; c.Effort = "xhigh" }); err != nil {
		t.Fatal(err)
	}
	raw, err := os.ReadFile(m.path)
	if err != nil {
		t.Fatal(err)
	}
	var stored fileFormat
	if err := json.Unmarshal(raw, &stored); err != nil {
		t.Fatal(err)
	}
	chat := stored.Sleeves[0].Chat
	if chat.Requested.Effort != "xhigh" || chat.Effort != "xhigh" || chat.Defaults.Effort != "low" {
		t.Fatalf("selection not persisted independently: %+v", chat)
	}
}

func TestCatalogHasNoWorkspaceOrRealHomeMount(t *testing.T) {
	c := testConfig()
	c.Home = t.TempDir()
	bin := t.TempDir()
	c.PathDirs = []string{bin}
	if err := os.WriteFile(filepath.Join(bin, "codex"), []byte("#!/bin/sh\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(c.authDir(Codex), 0o700); err != nil {
		t.Fatal(err)
	}
	m := &Manager{cfg: c}
	opts, err := m.CatalogOptions()
	if err != nil {
		t.Fatal(err)
	}
	argv, err := opts.Command("codex", []string{"app-server"})
	if err != nil {
		t.Fatal(err)
	}
	joined := strings.Join(argv, " ")
	if !strings.Contains(joined, "--tmpfs "+c.UserHome) || strings.Contains(joined, "--bind "+c.UserHome+" ") || strings.Contains(joined, "--bind /work") {
		t.Fatalf("discovery exposed host home/workspace: %s", joined)
	}
	if !strings.Contains(joined, "--bind "+c.authDir(Codex)+" "+c.authDir(Codex)) {
		t.Fatal("native auth not mounted")
	}
	if !strings.Contains(joined, "--setenv CODEX_HOME "+filepath.Join(c.UserHome, ".codex")) ||
		!strings.Contains(joined, "--symlink "+filepath.Join(c.authDir(Codex), "auth.json")+" "+filepath.Join(c.UserHome, ".codex", "auth.json")) {
		t.Fatal("catalog config/session data must stay in the temporary home, with only auth linked")
	}
}

func TestSelectionPersistenceFailureKeepsPreviousRecord(t *testing.T) {
	home := t.TempDir()
	old := &Chat{Effort: "low"}
	m := &Manager{cfg: Config{Home: home}, path: home, sleeves: map[string]*entry{"envoy": {Info: Info{Name: "envoy", Chat: old}}}}
	if _, err := m.SetChat("envoy", func(c *Chat) { c.Effort = "xhigh" }); err == nil {
		t.Fatal("expected persistence failure")
	}
	if m.sleeves["envoy"].Chat != old {
		t.Fatal("failed save changed record")
	}
}
