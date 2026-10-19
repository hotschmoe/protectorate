package main

import (
	"bytes"
	"context"
	"net"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"

	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/hub"
)

func startAuthHub(t *testing.T) (url, home string) {
	t.Helper()
	home = t.TempDir()
	t.Setenv("PROTECTORATE_HOME", home)
	t.Setenv(tokenEnv, "")
	owner, err := hub.EnsureToken()
	if err != nil {
		t.Fatal(err)
	}
	store, err := auth.OpenStore(filepath.Join(home, "tokens.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	audit := auth.NewAudit(filepath.Join(home, "audit"))
	t.Cleanup(func() { audit.Close() })
	h := hub.New("test", owner)
	h.SetAuth(auth.New(owner, store, audit))
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- h.Serve(ctx, l) }()
	t.Cleanup(func() { cancel(); <-done })
	return "http://" + l.Addr().String(), home
}

func runCLI(url string, args ...string) (string, error) {
	var buf bytes.Buffer
	cmd := newRoot()
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs(append(args, "--hub", url))
	err := cmd.Execute()
	return buf.String(), err
}

func TestTokenCLI(t *testing.T) {
	url, home := startAuthHub(t)

	out, err := runCLI(url, "token", "create", "bridge", "--scope", "read", "--scope", "backlog_*", "--ttl", "24h")
	if err != nil {
		t.Fatal(err)
	}
	secret := regexp.MustCompile(`secret\s+(ptk_[0-9a-f]+)`).FindStringSubmatch(out)
	if secret == nil {
		t.Fatalf("no secret in output:\n%s", out)
	}
	if _, err := runCLI(url, "token", "create", "bad", "--scope", "typo_*"); err == nil {
		t.Fatal("bad scope accepted")
	}
	if _, err := runCLI(url, "token", "create", "none"); err == nil {
		t.Fatal("scopeless token accepted")
	}

	list, err := runCLI(url, "token", "ls")
	if err != nil || !strings.Contains(list, "bridge") || !strings.Contains(list, "active") || strings.Contains(list, secret[1]) {
		t.Fatalf("ls: %v\n%s", err, list)
	}
	id := strings.Fields(strings.Split(list, "\n")[1])[0]

	t.Setenv(tokenEnv, secret[1])
	if out, err := runCLI(url, "status"); err != nil || !strings.Contains(out, "version") {
		t.Fatalf("env token read: %v %s", err, out)
	}
	if _, err := runCLI(url, "token", "ls"); err == nil || !strings.Contains(err.Error(), "owner") {
		t.Fatalf("scoped token listed tokens: %v", err)
	}
	if _, err := runCLI(url, "session", "kill", "ghost"); err == nil || !strings.Contains(err.Error(), "session_kill_session") {
		t.Fatalf("out-of-scope mutation not denied with the scope named: %v", err)
	}

	file := filepath.Join(t.TempDir(), "owner.tok")
	ownerTok, _ := hub.ReadToken()
	if err := os.WriteFile(file, []byte(ownerTok+"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if out, err := runCLI(url, "--token-file", file, "token", "ls"); err != nil {
		t.Fatalf("--token-file must beat env: %v %s", err, out)
	}
	t.Setenv(tokenEnv, "")

	if _, err := runCLI(url, "token", "revoke", id); err != nil {
		t.Fatal(err)
	}
	t.Setenv(tokenEnv, secret[1])
	if _, err := runCLI(url, "status"); err == nil {
		t.Fatal("revoked token still works")
	}
	t.Setenv(tokenEnv, "")

	tail, err := runCLI(url, "audit", "tail", "-n", "20")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(tail, "CreateToken") || !strings.Contains(tail, "RevokeToken") || strings.Contains(tail, secret[1]) {
		t.Fatalf("audit tail:\n%s", tail)
	}
	two, _ := runCLI(url, "audit", "tail", "-n", "2")
	if lines := strings.Count(strings.TrimSpace(two), "\n") + 1; lines != 2 {
		t.Fatalf("tail -n 2 printed %d lines", lines)
	}
	raw, _ := os.ReadFile(filepath.Join(home, "tokens.yaml"))
	if strings.Contains(string(raw), secret[1]) {
		t.Fatal("secret in tokens.yaml")
	}
}

func TestPairMintsScopedDeviceToken(t *testing.T) {
	url, _ := startAuthHub(t)
	out, err := runCLI(url, "pair", "--name", "phone", "--ttl", "48h")
	if err != nil {
		t.Fatal(err)
	}
	owner, _ := hub.ReadToken()
	if strings.Contains(out, owner) {
		t.Fatal("pairing link embeds the owner token")
	}
	m := regexp.MustCompile(`web:\s+\S+token=(ptk_[0-9a-f]+)`).FindStringSubmatch(out)
	if m == nil {
		t.Fatalf("no device token in link:\n%s", out)
	}
	ls, err := runCLI(url, "token", "ls")
	if err != nil || !strings.Contains(ls, "pair: phone") || !strings.Contains(ls, "[backlog_* events read session_* sleeve_* terminal]") {
		t.Fatalf("%v\n%s", err, ls)
	}
	if _, err := runCLI(url, "pair", "--name", "tablet", "--host-shell"); err != nil {
		t.Fatal(err)
	}
	ls, err = runCLI(url, "token", "ls")
	if err != nil || !strings.Contains(ls, "[backlog_* events read session_* sleeve_* terminal terminal:host]") {
		t.Fatalf("--host-shell did not add terminal:host: %v\n%s", err, ls)
	}
	t.Setenv(tokenEnv, m[1])
	if _, err := runCLI(url, "status"); err != nil {
		t.Fatalf("device token cannot read: %v", err)
	}
	if _, err := runCLI(url, "token", "ls"); err == nil {
		t.Fatal("device token can manage tokens")
	}
}
