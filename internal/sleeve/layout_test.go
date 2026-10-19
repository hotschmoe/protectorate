package sleeve

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func layoutConfig(t *testing.T) Config {
	t.Helper()
	root := t.TempDir()
	c := testConfig()
	c.Home = filepath.Join(root, "state")
	c.UserHome = filepath.Join(root, "user")
	return c
}

func mustWrite(t *testing.T, path, content string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatal(err)
	}
}

func mustRead(t *testing.T, path string) string {
	t.Helper()
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}

func renameOver(t *testing.T, path, content string) {
	t.Helper()
	tmp := path + ".tmp.123"
	mustWrite(t, tmp, content)
	if err := os.Rename(tmp, path); err != nil {
		t.Fatal(err)
	}
}

// Each harness's save pattern, performed through sleeve A's view of its
// credential file, must be seen through sleeve B's view.
func TestSharedAuthVisibleAcrossSleeves(t *testing.T) {
	c := layoutConfig(t)
	for _, h := range []Harness{Claude, Codex, Grok, Pi} {
		t.Run(string(h), func(t *testing.T) {
			a, b := c.layout("a-"+string(h), h), c.layout("b-"+string(h), h)
			for _, l := range []Layout{a, b} {
				if err := l.Apply(); err != nil {
					t.Fatal(err)
				}
			}
			share := c.authDir(h)
			if len(a.Binds) != 1 || a.Binds[0] != share || b.Binds[0] != share {
				t.Fatalf("binds = %v %v, want shared %s", a.Binds, b.Binds, share)
			}
			var viewA, viewB string
			switch h {
			case Claude:
				viewA, viewB = filepath.Join(share, ".credentials.json"), filepath.Join(share, ".credentials.json")
				mustWrite(t, viewA, "v1")
				renameOver(t, viewA, "v2")
			case Pi:
				viewA, viewB = filepath.Join(share, "auth.json"), filepath.Join(share, "auth.json")
				mustWrite(t, viewA, "v1")
				if err := os.WriteFile(viewA, []byte("v2"), 0o600); err != nil {
					t.Fatal(err)
				}
			case Codex:
				viewA = filepath.Join(c.sleeveHome("a-codex"), ".codex/auth.json")
				viewB = filepath.Join(c.sleeveHome("b-codex"), ".codex/auth.json")
				mustWrite(t, viewA, "v1")
				mustWrite(t, viewA, "v2")
			case Grok:
				viewA = filepath.Join(c.sleeveHome("a-grok"), ".grok/auth.json")
				viewB = filepath.Join(c.sleeveHome("b-grok"), ".grok/auth.json")
				mustWrite(t, viewA, "v1")
				target, err := filepath.EvalSymlinks(viewA)
				if err != nil {
					t.Fatal(err)
				}
				renameOver(t, target, "v2")
				for _, name := range []string{"a-grok", "b-grok"} {
					lock := filepath.Join(c.sleeveHome(name), ".grok/auth.json.lock")
					if got, _ := os.Readlink(lock); got != filepath.Join(share, "auth.json.lock") {
						t.Errorf("%s lock link = %q", name, got)
					}
				}
			}
			if got := mustRead(t, viewB); got != "v2" {
				t.Errorf("sleeve B sees %q, want v2", got)
			}
			if fi, err := os.Lstat(viewA); err != nil {
				t.Fatal(err)
			} else if (h == Codex || h == Grok) && fi.Mode()&os.ModeSymlink == 0 {
				t.Errorf("per-sleeve %s was replaced by a regular file", viewA)
			}
		})
	}
}

func TestLayoutApply(t *testing.T) {
	linuxHost(t)
	c := layoutConfig(t)
	l := c.layout("s", Claude)
	if err := l.Apply(); err != nil {
		t.Fatal(err)
	}
	seed := filepath.Join(c.sleeveHome("s"), ".claude/.claude.json")
	if got := mustRead(t, seed); got != claudeSeed {
		t.Errorf("seed = %q", got)
	}
	mustWrite(t, seed, "user-edited")
	if err := l.Apply(); err != nil {
		t.Fatal(err)
	}
	if got := mustRead(t, seed); got != "user-edited" {
		t.Errorf("seed overwritten: %q", got)
	}
	if fi, err := os.Stat(c.authDir(Claude)); err != nil || fi.Mode().Perm() != 0o700 {
		t.Errorf("auth dir: %v %v", fi, err)
	}

	cl := c.layout("s", Codex)
	link := filepath.Join(c.sleeveHome("s"), ".codex/auth.json")
	if err := os.MkdirAll(filepath.Dir(link), 0o700); err != nil {
		t.Fatal(err)
	}
	mustWrite(t, link, "stale local login")
	if err := cl.Apply(); err != nil {
		t.Fatal(err)
	}
	if got, _ := os.Readlink(link); got != filepath.Join(c.authDir(Codex), "auth.json") {
		t.Errorf("link target = %q", got)
	}
	ents, _ := filepath.Glob(link + ".orphaned-*")
	if len(ents) != 1 || mustRead(t, ents[0]) != "stale local login" {
		t.Errorf("stale file not preserved: %v", ents)
	}
	if err := cl.Apply(); err != nil {
		t.Fatalf("second apply: %v", err)
	}
	if ents, _ := filepath.Glob(link + ".orphaned-*"); len(ents) != 1 {
		t.Errorf("apply is not idempotent: %v", ents)
	}
}

func TestLayoutEnv(t *testing.T) {
	linuxHost(t)
	c := testConfig()
	tests := []struct {
		h    Harness
		want []string
	}{
		{Claude, []string{"CLAUDE_CONFIG_DIR=/home/u/.claude", "CLAUDE_SECURESTORAGE_CONFIG_DIR=/state/auth/claude"}},
		{Codex, []string{"CODEX_HOME=/home/u/.codex"}},
		{Grok, []string{"GROK_HOME=/home/u/.grok"}},
		{Pi, []string{"PI_CODING_AGENT_DIR=/state/auth/pi", "PI_CODING_AGENT_SESSION_DIR=/home/u/.pi/sessions"}},
		{Shell, nil},
	}
	for _, tt := range tests {
		var got []string
		for _, kv := range c.layout("s", tt.h).Env {
			if kv[0] != "DISABLE_AUTOUPDATER" {
				got = append(got, kv[0]+"="+kv[1])
			}
		}
		if strings.Join(got, " ") != strings.Join(tt.want, " ") {
			t.Errorf("%s env = %v, want %v", tt.h, got, tt.want)
		}
	}
}
