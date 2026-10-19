package main

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func runCmd(t *testing.T, args ...string) (string, error) {
	t.Helper()
	root := hubCmd()
	var out bytes.Buffer
	root.SetOut(&out)
	root.SetErr(&out)
	root.SetArgs(args)
	err := root.Execute()
	return out.String(), err
}

func TestHubBackupAndRestoreCommands(t *testing.T) {
	home := t.TempDir()
	t.Setenv("PROTECTORATE_HOME", home)
	if err := os.WriteFile(filepath.Join(home, "token"), []byte("tok\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(home, "mailbox.jsonl"), []byte("{}\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	dest := filepath.Join(t.TempDir(), "nas")

	out, err := runCmd(t, "backup", "--to", dest)
	if err != nil {
		t.Fatal(err)
	}
	lines := strings.Split(strings.TrimSpace(out), "\n")
	if len(lines) != 2 || !strings.HasPrefix(lines[0], filepath.Join(dest, "protectorate-hub-")) || !strings.HasPrefix(lines[1], "sha256 ") || len(lines[1]) != len("sha256 ")+64 {
		t.Fatalf("output = %q", out)
	}
	archive := lines[0]

	restored := filepath.Join(t.TempDir(), "new-home")
	out, err = runCmd(t, "restore", archive, "--home", restored)
	if err != nil || !strings.Contains(out, "restored 2 files") {
		t.Fatalf("restore: %v %q", err, out)
	}
	if b, _ := os.ReadFile(filepath.Join(restored, "token")); string(b) != "tok\n" {
		t.Fatalf("token = %q", b)
	}

	if _, err := runCmd(t, "restore", archive, "--home", restored); err == nil || !strings.Contains(err.Error(), "not empty") {
		t.Fatalf("restore into a used home: %v", err)
	}
	if _, err := runCmd(t, "restore", archive); err == nil {
		t.Fatal("restore without --home must fail")
	}
}
