package main

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func repoRoot(t *testing.T) string {
	t.Helper()
	if _, err := exec.LookPath("make"); err != nil {
		t.Skip("make not installed")
	}
	root, err := filepath.Abs("../..")
	if err != nil {
		t.Fatal(err)
	}
	return root
}

func runIn(t *testing.T, root string, env []string, name string, args ...string) (string, error) {
	t.Helper()
	c := exec.Command(name, args...)
	c.Dir = root
	c.Env = append(os.Environ(), env...)
	out, err := c.CombinedOutput()
	return strings.TrimSpace(string(out)), err
}

func TestVersionDerivation(t *testing.T) {
	root := repoRoot(t)
	tests := []struct {
		name    string
		version string
		want    string
	}{
		{"exact tag", "v1.2.3", "1.2.3+77"},
		{"tag plus commits", "v1.2.3-4-gabc1234", "1.2.3+77"},
		{"dirty", "v1.2.3-4-gabc1234-dirty", "1.2.3+77"},
		{"dirty on the tag", "v1.2.3-dirty", "1.2.3+77"},
		{"no v prefix", "2.0.1", "2.0.1+77"},
		{"no tag", "abc1234", "0.0.0+77"},
		{"no tag, dirty", "abc1234-dirty", "0.0.0+77"},
		{"dev fallback", "dev", "0.0.0+77"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			env := []string{"VERSION=" + tt.version, "BUILD_NUMBER=77"}
			if got, err := runIn(t, root, env, "make", "-s", "--no-print-directory", "version-app"); err != nil || got != tt.want {
				t.Errorf("version-app = %q, %v; want %q", got, err, tt.want)
			}
			if got, err := runIn(t, root, env, "make", "-s", "--no-print-directory", "version"); err != nil || got != tt.version {
				t.Errorf("version = %q, %v; want %q", got, err, tt.version)
			}
		})
	}
}

func TestAppBuildsCarryTheVersion(t *testing.T) {
	root := repoRoot(t)
	for _, target := range []string{"app-build-web", "apk", "app-build-linux"} {
		out, err := runIn(t, root, []string{"VERSION=v3.1.4-2-gabc", "BUILD_NUMBER=9"}, "make", "-n", target)
		if err != nil {
			t.Fatal(err)
		}
		for _, f := range []string{"--build-name=3.1.4", "--build-number=9", "--dart-define=PROTECTORATE_VERSION=v3.1.4-2-gabc"} {
			if !strings.Contains(out, f) {
				t.Errorf("make %s lacks %s: %s", target, f, out)
			}
		}
	}
}

func TestCheckVersion(t *testing.T) {
	root := repoRoot(t)
	dir := t.TempDir()
	write := func(name, body string) string {
		p := filepath.Join(dir, name)
		if err := os.WriteFile(p, []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
		return p
	}
	good := write("good.yaml", "name: protectorate\nversion: 0.0.0+0\n")
	stale := write("stale.yaml", "name: protectorate\nversion: 1.0.0+1\n")
	cleanCI := write("clean.yml", "steps:\n  - run: make apk\n")
	bareFlutter := write("bare.yml", "steps:\n  - run: cd app && flutter build apk\n")
	bareGo := write("barego.yml", "steps:\n  - run: go build ./cmd/protectorate\n")
	tests := []struct {
		name      string
		pubspec   string
		workflow  string
		wantFail  bool
		wantInOut string
	}{
		{"consistent", good, cleanCI, false, ""},
		{"pubspec carries a real version", stale, cleanCI, true, "placeholder"},
		{"workflow builds the app by hand", good, bareFlutter, true, "build through make"},
		{"workflow builds go by hand", good, bareGo, true, "build through make"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			out, err := runIn(t, root, []string{"PUBSPEC=" + tt.pubspec, "WORKFLOW=" + tt.workflow}, "sh", "scripts/check-version.sh")
			if (err != nil) != tt.wantFail || !strings.Contains(out, tt.wantInOut) {
				t.Errorf("err = %v, output = %q; want fail=%v containing %q", err, out, tt.wantFail, tt.wantInOut)
			}
		})
	}
}
