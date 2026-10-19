package hub

import (
	"crypto/sha256"
	"encoding/hex"
	"flag"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

var updateGolden = flag.Bool("update", false, "rewrite the golden files in testdata")

func TestInstallScriptGolden(t *testing.T) {
	for _, kind := range []string{"sh", "ps1"} {
		t.Run(kind, func(t *testing.T) {
			got, err := InstallScript(kind, "http://hub.lan:7470", "http://10.0.0.5:7480")
			if err != nil {
				t.Fatal(err)
			}
			path := filepath.Join("testdata", "install."+kind+".golden")
			if *updateGolden {
				if err := os.MkdirAll("testdata", 0o755); err != nil {
					t.Fatal(err)
				}
				if err := os.WriteFile(path, got, 0o644); err != nil {
					t.Fatal(err)
				}
			}
			want, err := os.ReadFile(path)
			if err != nil {
				t.Fatalf("%v (run go test -run TestInstallScriptGolden -update)", err)
			}
			if string(got) != string(want) {
				t.Fatalf("%s differs from %s (rerun with -update after checking the change)", kind, path)
			}
		})
	}
}

func TestInstallScriptQuotesItsURLs(t *testing.T) {
	sh, err := InstallScript("sh", "http://h:1", "http://c/x'y")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(sh), `control='http://c/x'\''y'`) {
		t.Fatalf("sh does not quote the control URL:\n%s", sh)
	}
	ps, err := InstallScript("ps1", "http://h:1", "http://c/x'y")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(ps), `$control = 'http://c/x''y'`) {
		t.Fatalf("ps1 does not quote the control URL:\n%s", ps)
	}
}

// TestInstallShTarget runs the generated install.sh under each POSIX shell
// found here, with a faked uname, and checks which build it picks.
func TestInstallShTarget(t *testing.T) {
	script, err := InstallScript("sh", "http://hub.lan:7470", "http://hs:7480")
	if err != nil {
		t.Fatal(err)
	}
	dir := t.TempDir()
	path := filepath.Join(dir, "install.sh")
	// A shell function beats any uname on PATH, including busybox's builtin
	// applet, so the fake holds in every shell.
	uname := "uname() {\n\tcase \"$1\" in\n\t-s) echo \"$FAKE_S\" ;;\n\t-m) echo \"$FAKE_M\" ;;\n\t-o) [ -n \"$FAKE_O\" ] || return 1; echo \"$FAKE_O\" ;;\n\tesac\n}\n"
	if err := os.WriteFile(path, append([]byte(uname), script...), 0o644); err != nil {
		t.Fatal(err)
	}
	var shells [][]string
	for _, s := range [][]string{{"sh"}, {"dash"}, {"bash", "--posix"}, {"busybox", "sh"}} {
		if p, err := exec.LookPath(s[0]); err == nil {
			shells = append(shells, append([]string{p}, s[1:]...))
		}
	}
	if len(shells) == 0 {
		t.Skip("no POSIX shell")
	}
	tests := []struct {
		name, s, m, o, termux string
		want, wantErr         string
	}{
		{name: "linux x86_64", s: "Linux", m: "x86_64", o: "GNU/Linux", want: "protectorate-linux-amd64"},
		{name: "linux aarch64", s: "Linux", m: "aarch64", o: "GNU/Linux", want: "protectorate-linux-arm64"},
		{name: "linux arm64 spelling", s: "Linux", m: "arm64", want: "protectorate-linux-arm64"},
		{name: "termux by uname -o", s: "Linux", m: "aarch64", o: "Android", want: "protectorate-android-arm64"},
		{name: "termux by TERMUX_VERSION", s: "Linux", m: "aarch64", termux: "0.118.0", want: "protectorate-android-arm64"},
		{name: "32-bit arm", s: "Linux", m: "armv7l", o: "GNU/Linux", wantErr: "unsupported architecture armv7l"},
		{name: "android x86_64", s: "Linux", m: "x86_64", o: "Android", wantErr: "aarch64 only"},
		{name: "macos", s: "Darwin", m: "arm64", wantErr: "unsupported system Darwin"},
	}
	for _, sh := range shells {
		for _, tt := range tests {
			t.Run(filepath.Base(sh[0])+"/"+tt.name, func(t *testing.T) {
				cmd := exec.Command(sh[0], append(sh[1:], path, "--print-target")...)
				cmd.Env = []string{"PATH=" + os.Getenv("PATH"), "HOME=" + dir, "FAKE_S=" + tt.s, "FAKE_M=" + tt.m, "FAKE_O=" + tt.o}
				if tt.termux != "" {
					cmd.Env = append(cmd.Env, "TERMUX_VERSION="+tt.termux)
				}
				out, err := cmd.CombinedOutput()
				got := strings.TrimSpace(string(out))
				if tt.wantErr != "" {
					if err == nil || !strings.Contains(got, tt.wantErr) {
						t.Fatalf("err=%v out=%q, want failure with %q", err, got, tt.wantErr)
					}
					return
				}
				if err != nil || got != tt.want {
					t.Fatalf("err=%v out=%q, want %q", err, got, tt.want)
				}
			})
		}
	}
	t.Run("needs the key", func(t *testing.T) {
		cmd := exec.Command(shells[0][0], append(shells[0][1:], path, "--name", "box")...)
		cmd.Env = []string{"PATH=" + os.Getenv("PATH"), "HOME=" + dir, "FAKE_S=Linux", "FAKE_M=x86_64"}
		out, err := cmd.CombinedOutput()
		if err == nil || !strings.Contains(string(out), "PROTECTORATE_ENROLL_KEY") {
			t.Fatalf("err=%v\n%s", err, out)
		}
	})
}

// TestInstallPs1Arch checks the architecture mapping of install.ps1 where
// PowerShell is installed.
func TestInstallPs1Arch(t *testing.T) {
	pwsh, err := exec.LookPath("pwsh")
	if err != nil {
		t.Skip("pwsh not installed")
	}
	script, err := InstallScript("ps1", "http://hub.lan:7470", "http://hs:7480")
	if err != nil {
		t.Fatal(err)
	}
	defs := strings.TrimSuffix(strings.TrimSpace(string(script)), "Install-ProtectorateNode")
	tests := []struct{ in, want string }{
		{"ARM64", "arm64"}, {"Arm64", "arm64"}, {"AMD64", "amd64"}, {"X64", "amd64"}, {"x86", ""}, {"Arm", ""},
	}
	for _, tt := range tests {
		t.Run(tt.in, func(t *testing.T) {
			out, err := exec.Command(pwsh, "-NoProfile", "-NonInteractive", "-Command", defs+"\n[string](ConvertTo-ProtectorateArch '"+tt.in+"')").CombinedOutput()
			if err != nil || strings.TrimSpace(string(out)) != tt.want {
				t.Fatalf("err=%v out=%q, want %q", err, out, tt.want)
			}
		})
	}
}

func TestQuoting(t *testing.T) {
	tests := []struct{ in, sh, ps string }{
		{"plain", `'plain'`, `'plain'`},
		{"it's", `'it'\''s'`, `'it''s'`},
		{"$(id) `x` \"y\"", `'$(id) ` + "`x`" + ` "y"'`, `'$(id) ` + "`x`" + ` "y"'`},
		{"a\u2019b", "'a\u2019b'", "'a\u2019\u2019b'"},
	}
	for _, tt := range tests {
		if got := shQuote(tt.in); got != tt.sh {
			t.Errorf("shQuote(%q) = %s, want %s", tt.in, got, tt.sh)
		}
		if got := psQuote(tt.in); got != tt.ps {
			t.Errorf("psQuote(%q) = %s, want %s", tt.in, got, tt.ps)
		}
	}
}

func TestInstallCommands(t *testing.T) {
	cmds, err := InstallCommands("https://hub.example:7470", "hskey-auth-abc_DEF-123", "office-arm")
	if err != nil {
		t.Fatal(err)
	}
	want := map[string]string{
		"INSTALL_PLATFORM_WINDOWS": `$env:PROTECTORATE_ENROLL_KEY='hskey-auth-abc_DEF-123'; $env:PROTECTORATE_NODE_NAME='office-arm'; irm 'https://hub.example:7470/install.ps1' | iex`,
		"INSTALL_PLATFORM_LINUX":   `curl -fsSL 'https://hub.example:7470/install.sh' | PROTECTORATE_ENROLL_KEY='hskey-auth-abc_DEF-123' sh -s -- --name 'office-arm'`,
		"INSTALL_PLATFORM_TERMUX":  `curl -fsSL 'https://hub.example:7470/install.sh' | PROTECTORATE_ENROLL_KEY='hskey-auth-abc_DEF-123' sh -s -- --name 'office-arm'`,
	}
	if len(cmds) != len(want) {
		t.Fatalf("%d commands", len(cmds))
	}
	for _, c := range cmds {
		if c.Command != want[c.Platform.String()] {
			t.Errorf("%s:\n got %s\nwant %s", c.Platform, c.Command, want[c.Platform.String()])
		}
		if c.Note == "" || c.Shell == "" {
			t.Errorf("%s lacks shell or note", c.Platform)
		}
	}

	bad := []struct{ name, key string }{
		{"a'b", "goodkey12"}, {"$(id)", "goodkey12"}, {"x y", "goodkey12"}, {"Upper", "goodkey12"},
		{"-lead", "goodkey12"}, {"trail-", "goodkey12"}, {"a.b", "goodkey12"}, {"a_b", "goodkey12"},
		{strings.Repeat("a", 64), "goodkey12"}, {"", "goodkey12"}, {"x;rm", "goodkey12"},
		{"box", "key'quote"}, {"box", "key with space"}, {"box", "short"}, {"box", "k$(id)xxxx"},
	}
	for _, b := range bad {
		if _, err := InstallCommands("http://h", b.key, b.name); err == nil {
			t.Errorf("name %q key %q accepted", b.name, b.key)
		}
	}
}

func TestInstallBase(t *testing.T) {
	tests := []struct{ in, want string }{
		{"http://hub.lan:7470", "http://hub.lan:7470"},
		{"https://hub.example/", "https://hub.example"},
		{"http://10.0.0.5:7470", "http://10.0.0.5:7470"},
		{"http://[fd00::1]:7470", "http://[fd00::1]:7470"},
		{"ftp://hub", ""},
		{"http://user:pw@hub", ""},
		{"http://hub/app", ""},
		{"http://hub?x=1", ""},
		{"http://hub#f", ""},
		{"http://hu'b", ""},
		{"hub.lan:7470", ""},
		{"", ""},
	}
	for _, tt := range tests {
		got, err := installBase(tt.in)
		if (err != nil) != (tt.want == "") || got != tt.want {
			t.Errorf("installBase(%q) = %q, %v; want %q", tt.in, got, err, tt.want)
		}
	}
}

func TestInstallEndpoints(t *testing.T) {
	dir := t.TempDir()
	self := filepath.Join(dir, "self")
	bins := filepath.Join(dir, "bin")
	if err := os.MkdirAll(bins, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(self, []byte("hub's own build"), 0o755); err != nil {
		t.Fatal(err)
	}
	missing := "protectorate-linux-amd64"
	if BinaryName(runtime.GOOS, runtime.GOARCH) == missing {
		missing = "protectorate-linux-arm64"
	}
	other := "windows-arm64 build"
	if err := os.WriteFile(filepath.Join(bins, "protectorate-windows-arm64.exe"), []byte(other), 0o755); err != nil {
		t.Fatal(err)
	}
	h := New("test", testToken)
	h.SelfExe, h.BinaryDir = self, bins
	srv := httptest.NewServer(h.Handler())
	t.Cleanup(srv.Close)
	get := func(path string) (int, string) {
		t.Helper()
		res, err := http.Get(srv.URL + path)
		if err != nil {
			t.Fatal(err)
		}
		defer res.Body.Close()
		b, _ := io.ReadAll(res.Body)
		return res.StatusCode, string(b)
	}
	sum := sha256.Sum256([]byte(other))

	if code, body := get("/install.sh"); code != http.StatusServiceUnavailable || !strings.Contains(body, "tailnet") {
		t.Fatalf("install.sh without tailnet: %d %s", code, body)
	}
	h.SetTailnetAdmin(newRecordingAdmin())

	tests := []struct {
		name, path string
		code       int
		want       string
	}{
		{"build from the binary dir", "/dl/protectorate-windows-arm64.exe", 200, other},
		{"its digest", "/dl/protectorate-windows-arm64.exe.sha256", 200, hex.EncodeToString(sum[:]) + "  protectorate-windows-arm64.exe\n"},
		{"missing build", "/dl/" + missing, 404, "make cross"},
		{"unknown platform", "/dl/protectorate-plan9-mips", 404, ""},
		{"traversal", "/dl/..%2f..%2fetc%2fpasswd", 404, ""},
		{"not a build", "/dl/token", 404, ""},
		{"sh names this hub", "/install.sh", 200, "hub='" + srv.URL + "'\ncontrol='http://hs.lan:7480'"},
		{"ps1 names this hub", "/install.ps1", 200, "$hub = '" + srv.URL + "'"},
	}
	if p := BinaryName(runtime.GOOS, runtime.GOARCH); downloadPlatforms[p] {
		tests = append(tests, struct {
			name, path string
			code       int
			want       string
		}{"own platform is the hub's executable", "/dl/" + p, 200, "hub's own build"})
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			code, body := get(tt.path)
			if code != tt.code || !strings.Contains(body, tt.want) {
				t.Fatalf("%s: %d %q", tt.path, code, body)
			}
		})
	}
	h.PublicURL = "https://hub.example"
	if _, body := get("/install.sh"); !strings.Contains(body, "hub='https://hub.example'") {
		t.Fatalf("public URL not used:\n%s", body)
	}
}
