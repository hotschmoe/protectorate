package main

import (
	"archive/tar"
	"compress/gzip"
	"crypto/sha256"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

const fakeBinary = `#!/bin/sh
echo "$@" >> "$HOME/protectorate-calls"
case "$*" in
"service systemd hub"*)
	mkdir -p "$HOME/.config/systemd/user"
	echo "$*" > "$HOME/.config/systemd/user/protectorate-hub.service" ;;
esac
`

func fakeRelease(t *testing.T, version, arch string) string {
	t.Helper()
	dir := t.TempDir()
	var sums strings.Builder
	add := func(name string, data []byte) {
		if err := os.WriteFile(filepath.Join(dir, name), data, 0o644); err != nil {
			t.Fatal(err)
		}
		fmt.Fprintf(&sums, "%x  %s\n", sha256.Sum256(data), name)
	}
	for _, n := range []string{"linux-amd64", "linux-arm64", "windows-amd64.exe", "windows-arm64.exe", "android-arm64"} {
		add("protectorate-"+n, []byte(fakeBinary+"# "+n+"\n"))
	}
	var tgz strings.Builder
	gz := gzip.NewWriter(&tgz)
	tw := tar.NewWriter(gz)
	body := []byte("<html></html>")
	if err := tw.WriteHeader(&tar.Header{Name: "index.html", Mode: 0o644, Size: int64(len(body))}); err != nil {
		t.Fatal(err)
	}
	tw.Write(body)
	tw.Close()
	gz.Close()
	add("protectorate-web-"+version+".tar.gz", []byte(tgz.String()))
	if err := os.WriteFile(filepath.Join(dir, "SHA256SUMS"), []byte(sums.String()), 0o644); err != nil {
		t.Fatal(err)
	}
	return dir
}

func shims(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	for _, n := range []string{"bwrap", "tmux", "git", "loginctl"} {
		body := "#!/bin/sh\nexit 0\n"
		if n == "loginctl" {
			body = "#!/bin/sh\necho Linger=no\n"
		}
		if err := os.WriteFile(filepath.Join(dir, n), []byte(body), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	sc := "#!/bin/sh\necho \"systemctl $*\" >> \"$HOME/systemctl-calls\"\nexit 0\n"
	if err := os.WriteFile(filepath.Join(dir, "systemctl"), []byte(sc), 0o755); err != nil {
		t.Fatal(err)
	}
	return dir
}

func runInstaller(t *testing.T, home, shim string, args ...string) (string, error) {
	t.Helper()
	script, err := filepath.Abs("../../install-hub.sh")
	if err != nil {
		t.Fatal(err)
	}
	cmd := exec.Command("bash", append([]string{script}, args...)...)
	cmd.Env = []string{"HOME=" + home, "PATH=" + shim + ":/usr/bin:/bin", "TMPDIR=" + home}
	out, err := cmd.CombinedOutput()
	return string(out), err
}

func TestInstallHub(t *testing.T) {
	if runtime.GOOS != "linux" || (runtime.GOARCH != "amd64" && runtime.GOARCH != "arm64") {
		t.Skip("installer targets linux amd64 and arm64")
	}
	for _, tool := range []string{"bash", "curl", "tar", "sha256sum"} {
		if _, err := exec.LookPath(tool); err != nil {
			t.Skip(tool + " not available")
		}
	}
	const version = "v9.9.9"
	rel := fakeRelease(t, version, runtime.GOARCH)
	base := "file://" + rel
	own := "protectorate-linux-" + runtime.GOARCH

	t.Run("dry run installs nothing", func(t *testing.T) {
		home := t.TempDir()
		out, err := runInstaller(t, home, shims(t), "--version", version, "--base-url", base, "--listen", "127.0.0.1:7999", "--dry-run")
		if err != nil {
			t.Fatalf("%v\n%s", err, out)
		}
		for _, want := range []string{"would run: " + home + "/.local/bin/protectorate hub tailnet install", "--listen 127.0.0.1:7999 --web " + home + "/.local/share/protectorate/web"} {
			if !strings.Contains(out, want) {
				t.Errorf("output lacks %q:\n%s", want, out)
			}
		}
		if _, err := os.Stat(filepath.Join(home, ".local")); err == nil {
			t.Error("dry run created ~/.local")
		}
	})

	t.Run("install then upgrade", func(t *testing.T) {
		home, shim := t.TempDir(), shims(t)
		for i := 0; i < 2; i++ {
			out, err := runInstaller(t, home, shim, "--version", version, "--base-url", base, "--public-url", "http://hub.lan:7470")
			if err != nil {
				t.Fatalf("run %d: %v\n%s", i, err, out)
			}
			if !strings.Contains(out, "enable-linger") || !strings.Contains(out, "Open http://hub.lan:7470") {
				t.Errorf("run %d: missing next steps:\n%s", i, out)
			}
		}
		data := filepath.Join(home, ".local", "share", "protectorate")
		for _, p := range []string{filepath.Join(home, ".local", "bin", "protectorate"), filepath.Join(data, "web", "index.html"), filepath.Join(data, "bin", "protectorate-windows-arm64.exe"), filepath.Join(data, "bin", "protectorate-android-arm64")} {
			if _, err := os.Stat(p); err != nil {
				t.Errorf("missing %s", p)
			}
		}
		if _, err := os.Stat(filepath.Join(data, "bin", own)); err == nil {
			t.Errorf("own platform build %s should not be in bin", own)
		}
		unit, err := os.ReadFile(filepath.Join(home, ".config", "systemd", "user", "protectorate-hub.service"))
		if err != nil {
			t.Fatal(err)
		}
		if !strings.Contains(string(unit), "--listen 127.0.0.1:7470 --web "+data+"/web --public-url http://hub.lan:7470") {
			t.Errorf("unit: %s", unit)
		}
		calls, _ := os.ReadFile(filepath.Join(home, "systemctl-calls"))
		if !strings.Contains(string(calls), "enable --now protectorate-hub") {
			t.Errorf("systemctl calls: %s", calls)
		}
	})

	t.Run("checksum mismatch fails", func(t *testing.T) {
		bad := fakeRelease(t, version, runtime.GOARCH)
		if err := os.WriteFile(filepath.Join(bad, own), []byte("tampered"), 0o644); err != nil {
			t.Fatal(err)
		}
		home := t.TempDir()
		out, err := runInstaller(t, home, shims(t), "--version", version, "--base-url", "file://"+bad)
		if err == nil || !strings.Contains(out, "checksum mismatch") {
			t.Fatalf("want checksum failure, got %v\n%s", err, out)
		}
		if _, err := os.Stat(filepath.Join(home, ".local", "bin", "protectorate")); err == nil {
			t.Error("binary installed despite mismatch")
		}
	})

	t.Run("missing prerequisites are named", func(t *testing.T) {
		tools := t.TempDir()
		for _, n := range []string{"curl", "tar", "sha256sum", "uname"} {
			if err := os.Symlink(mustLookPath(t, n), filepath.Join(tools, n)); err != nil {
				t.Fatal(err)
			}
		}
		script, err := filepath.Abs("../../install-hub.sh")
		if err != nil {
			t.Fatal(err)
		}
		cmd := exec.Command(mustLookPath(t, "bash"), script, "--version", version, "--base-url", base)
		cmd.Env = []string{"HOME=" + t.TempDir(), "PATH=" + tools}
		out, err := cmd.CombinedOutput()
		for _, want := range []string{"missing prerequisites", "bwrap", "tmux", "git", "systemd user manager"} {
			if err == nil || !strings.Contains(string(out), want) {
				t.Fatalf("want %q in failure, got %v\n%s", want, err, out)
			}
		}
	})
}

func mustLookPath(t *testing.T, name string) string {
	t.Helper()
	p, err := exec.LookPath(name)
	if err != nil {
		t.Skip(name + " not available")
	}
	return p
}
