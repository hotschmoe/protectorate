package sleeve

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

func TestLoginPlan(t *testing.T) {
	home := t.TempDir()
	c := Config{Home: home}
	auth := func(h string) string { return filepath.Join(home, "auth", h) }
	tests := []struct {
		name    string
		h       Harness
		extra   []string
		env     [][2]string
		argv    []string
		wantErr bool
	}{
		{"claude", Claude, nil,
			[][2]string{{"CLAUDE_CONFIG_DIR", filepath.Join(home, "login", "claude")}, {"CLAUDE_SECURESTORAGE_CONFIG_DIR", auth("claude")}},
			[]string{"claude", "auth", "login"}, false},
		{"codex device", Codex, []string{"--device-auth"},
			[][2]string{{"CODEX_HOME", auth("codex")}}, []string{"codex", "login", "--device-auth"}, false},
		{"grok", Grok, nil, [][2]string{{"GROK_HOME", auth("grok")}}, []string{"grok", "login"}, false},
		{"pi", Pi, nil, [][2]string{{"PI_CODING_AGENT_DIR", auth("pi")}}, []string{"pi"}, false},
		{"pi extra", Pi, []string{"x"}, nil, nil, true},
		{"shell", Shell, nil, nil, nil, true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			l, err := c.LoginPlan(tt.h, tt.extra)
			if (err != nil) != tt.wantErr {
				t.Fatalf("err = %v, wantErr %v", err, tt.wantErr)
			}
			if tt.wantErr {
				return
			}
			if !reflect.DeepEqual(l.Env, tt.env) || !reflect.DeepEqual(l.Argv, tt.argv) {
				t.Fatalf("env %v argv %v", l.Env, l.Argv)
			}
			if err := l.Prepare(); err != nil {
				t.Fatal(err)
			}
			if fi, err := os.Stat(auth(string(tt.h))); err != nil || !fi.IsDir() {
				t.Fatalf("auth dir not created: %v", err)
			}
		})
	}
}

func TestLoggedIn(t *testing.T) {
	c := Config{Home: t.TempDir()}
	for _, h := range []Harness{Claude, Codex, Grok, Pi} {
		if ok, err := c.LoggedIn(h); err != nil || ok {
			t.Fatalf("%s before login: %v %v", h, ok, err)
		}
	}
	l, _ := c.LoginPlan(Codex, nil)
	if err := l.Prepare(); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(c.authDir(Codex), "auth.json"), []byte("{}"), 0o600); err != nil {
		t.Fatal(err)
	}
	if ok, err := c.LoggedIn(Codex); err != nil || !ok {
		t.Fatalf("codex after login: %v %v", ok, err)
	}
	if ok, _ := c.LoggedIn(Grok); ok {
		t.Fatal("grok should still be missing")
	}
}
