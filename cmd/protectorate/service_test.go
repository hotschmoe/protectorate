package main

import (
	"bytes"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func runService(t *testing.T, args ...string) (string, error) {
	t.Helper()
	root := newRoot()
	var out bytes.Buffer
	root.SetOut(&out)
	root.SetErr(&bytes.Buffer{})
	root.SetArgs(append([]string{"service"}, args...))
	err := root.Execute()
	return out.String(), err
}

func TestServiceWriteNoOverwrite(t *testing.T) {
	dir := t.TempDir()
	args := []string{"systemd", "hub", "--binary", "/bin/p", "--home", "/h", "--write", dir, "--", "--listen", ":1"}
	if _, err := runService(t, args...); err != nil {
		t.Fatal(err)
	}
	b, err := os.ReadFile(filepath.Join(dir, "protectorate-hub.service"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(b), "ExecStart=/bin/p hub serve --listen :1") {
		t.Errorf("flags not passed through:\n%s", b)
	}
	if _, err := runService(t, args...); err == nil {
		t.Error("second write overwrote the file")
	}
}

func TestServicePrintAndErrors(t *testing.T) {
	out, err := runService(t, "install-windows", "--binary", `C:\p.exe`, "--home", `C:\h`)
	if err != nil || !strings.Contains(out, "sc.exe create protectorate-node") {
		t.Fatalf("install-windows: %v\n%s", err, out)
	}
	if _, err := runService(t, "systemd", "bogus"); err == nil {
		t.Error("bad role accepted")
	}
	if _, err := runService(t, "windows-task", "--install"); err == nil {
		t.Error("--install accepted for scheduled task")
	}
	dir := t.TempDir()
	if _, err := runService(t, "termux", "--binary", "/p", "--home", "/h", "--write", dir); err != nil {
		t.Fatal(err)
	}
	for _, f := range []string{"protectorate-node/run", "boot/protectorate-node"} {
		st, err := os.Stat(filepath.Join(dir, f))
		if err != nil || (runtime.GOOS != "windows" && st.Mode()&0o100 == 0) {
			t.Errorf("%s missing or not executable: %v", f, err)
		}
	}
	if _, err := runService(t, "windows-task", "--binary", "/p", "--home", "/h", "--write", dir); err != nil {
		t.Fatal(err)
	}
	b, _ := os.ReadFile(filepath.Join(dir, "protectorate-node.xml"))
	if len(b) < 2 || b[0] != 0xff || b[1] != 0xfe {
		t.Error("task xml is not UTF-16 with BOM")
	}
}
