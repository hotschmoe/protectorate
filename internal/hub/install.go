package hub

import (
	"bytes"
	"context"
	_ "embed"
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"regexp"
	"runtime"
	"strings"
	"sync"
	"text/template"
	"time"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

var (
	//go:embed install.sh.tmpl
	installShSrc string
	//go:embed install.ps1.tmpl
	installPs1Src string

	scriptFuncs = template.FuncMap{"shq": shQuote, "psq": psQuote}
	installSh   = template.Must(template.New("install.sh").Funcs(scriptFuncs).Parse(installShSrc))
	installPs1  = template.Must(template.New("install.ps1").Funcs(scriptFuncs).Parse(installPs1Src))

	// enrollKeyRe is what a headscale pre-auth key looks like; anything else
	// is refused rather than quoted into a command line.
	enrollKeyRe = regexp.MustCompile(`^[A-Za-z0-9._~+/=-]{8,256}$`)
)

// Platforms the hub hands out builds for, as `make cross` names them.
var downloadPlatforms = map[string]bool{
	BinaryName("linux", "amd64"):   true,
	BinaryName("linux", "arm64"):   true,
	BinaryName("windows", "amd64"): true,
	BinaryName("windows", "arm64"): true,
	BinaryName("android", "arm64"): true,
}

// shQuote quotes s as one POSIX sh word.
func shQuote(s string) string { return "'" + strings.ReplaceAll(s, "'", `'\''`) + "'" }

// psQuote quotes s as a PowerShell verbatim string. PowerShell also takes the
// typographic single quotes as quote marks, so they are doubled too.
func psQuote(s string) string {
	var b strings.Builder
	b.WriteByte('\'')
	for _, r := range s {
		switch r {
		case '\'', '‘', '’', '‚', '‛':
			b.WriteRune(r)
		}
		b.WriteRune(r)
	}
	b.WriteByte('\'')
	return b.String()
}

// installBase normalizes the URL new machines reach the hub at to
// scheme://host[:port]. Paths, credentials, queries and odd host characters
// are refused: the URL is baked into scripts and command lines.
func installBase(raw string) (string, error) {
	u, err := url.Parse(strings.TrimSpace(raw))
	if err != nil || (u.Scheme != "http" && u.Scheme != "https") || u.Host == "" || u.User != nil ||
		strings.Trim(u.Path, "/") != "" || u.RawQuery != "" || u.Fragment != "" || u.Opaque != "" {
		return "", fmt.Errorf("install URL %q must be http(s)://host[:port]", raw)
	}
	if !validNodeAddr(u.Host) {
		return "", fmt.Errorf("install URL %q has an invalid host", raw)
	}
	return u.Scheme + "://" + u.Host, nil
}

type requestBaseKey struct{}

// requestBase is the URL a request came in on, as its Host header and
// transport say. It only shapes what that same caller gets back.
func requestBase(r *http.Request) string {
	scheme := "http"
	if r.TLS != nil || strings.EqualFold(r.Header.Get("X-Forwarded-Proto"), "https") {
		scheme = "https"
	}
	return scheme + "://" + r.Host
}

func withRequestBase(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		next.ServeHTTP(w, r.WithContext(context.WithValue(r.Context(), requestBaseKey{}, requestBase(r))))
	})
}

// installURL picks the base for install commands: the caller's choice, the
// hub's --public-url, or where the request came in.
func (h *Hub) installURL(ctx context.Context, asked string) (string, error) {
	switch {
	case asked != "":
		return installBase(asked)
	case h.PublicURL != "":
		return installBase(h.PublicURL)
	}
	from, _ := ctx.Value(requestBaseKey{}).(string)
	return installBase(from)
}

type scriptData struct{ Hub, Control string }

// InstallScript renders install.sh ("sh") or install.ps1 ("ps1") for a hub
// at hubURL whose tailnet control server is control.
func InstallScript(kind, hubURL, control string) ([]byte, error) {
	t := installSh
	if kind == "ps1" {
		t = installPs1
	}
	var b bytes.Buffer
	if err := t.Execute(&b, scriptData{Hub: hubURL, Control: control}); err != nil {
		return nil, fmt.Errorf("render %s: %w", t.Name(), err)
	}
	return b.Bytes(), nil
}

// InstallCommands returns the one-liners that install a node named name with
// key from the hub at hubURL.
func InstallCommands(hubURL, key, name string) ([]*pb.InstallCommand, error) {
	if !enrollKeyRe.MatchString(key) {
		return nil, errors.New("the enrollment key has unexpected characters")
	}
	if !validInviteName(name) {
		return nil, fmt.Errorf("name %q must be a lowercase DNS label (a-z, 0-9, -)", name)
	}
	sh := fmt.Sprintf("curl -fsSL %s | PROTECTORATE_ENROLL_KEY=%s sh -s -- --name %s", shQuote(hubURL+"/install.sh"), shQuote(key), shQuote(name))
	return []*pb.InstallCommand{
		{
			Platform: pb.InstallPlatform_INSTALL_PLATFORM_WINDOWS, Shell: "PowerShell",
			Command: fmt.Sprintf("$env:PROTECTORATE_ENROLL_KEY=%s; $env:PROTECTORATE_NODE_NAME=%s; irm %s | iex", psQuote(key), psQuote(name), psQuote(hubURL+"/install.ps1")),
			Note:    "Paste into a normal PowerShell (not Run as administrator) as the user who stays logged in. It installs a logon task for that user, so the agent runs in their desktop session without administrator rights. x64 and ARM64.",
		},
		{
			Platform: pb.InstallPlatform_INSTALL_PLATFORM_LINUX, Shell: "sh", Command: sh,
			Note: "Paste into a terminal as the user the agent should run as (not root). Needs curl. It installs a systemd user unit; to keep it running while logged out, run sudo loginctl enable-linger $USER once.",
		},
		{
			Platform: pb.InstallPlatform_INSTALL_PLATFORM_TERMUX, Shell: "Termux", Command: sh,
			Note: "Paste into Termux. For start at boot, first pkg install termux-services (then restart Termux) and install the Termux:Boot app.",
		},
	}, nil
}

// controlURL is the tailnet control server new nodes enroll with; without a
// tailnet nothing can be installed.
func (h *Hub) controlURL() (string, error) {
	if h.tnAdmin == nil || h.tnAdmin.ControlURL() == "" {
		return "", errNoTailnet
	}
	return h.tnAdmin.ControlURL(), nil
}

func (h *Hub) handleInstallScript(kind string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		control, err := h.controlURL()
		if err != nil {
			http.Error(w, err.Error(), http.StatusServiceUnavailable)
			return
		}
		base, err := h.installURL(r.Context(), "")
		if err != nil {
			http.Error(w, err.Error(), http.StatusBadRequest)
			return
		}
		body, err := InstallScript(kind, base, control)
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		w.Header().Set("Content-Type", "text/plain; charset=utf-8")
		w.Header().Set("Cache-Control", "no-store")
		_, _ = w.Write(body)
	}
}

// binaryPath is where the hub finds the build for a platform: its own
// executable for its own platform, otherwise --binary-dir.
func (h *Hub) binaryPath(goos, arch string) string {
	self, _ := os.Executable()
	if h.SelfExe != "" {
		self = h.SelfExe
	}
	dir := h.BinaryDir
	if dir == "" && self != "" {
		dir = filepath.Dir(self)
	}
	return pickBinary(dir, goos, arch, runtime.GOOS, runtime.GOARCH, self)
}

// digestCache remembers the SHA-256 of served builds by path, size and
// modification time, so a .sha256 request does not hash 30 MB each time.
type digestCache struct {
	mu sync.Mutex
	m  map[string]digestEntry
}

type digestEntry struct {
	size int64
	mod  time.Time
	sum  string
}

func (c *digestCache) sum(path string, fi os.FileInfo) (string, error) {
	c.mu.Lock()
	e, ok := c.m[path]
	c.mu.Unlock()
	if ok && e.size == fi.Size() && e.mod.Equal(fi.ModTime()) {
		return e.sum, nil
	}
	_, sum, err := fileDigest(path)
	if err != nil {
		return "", err
	}
	c.mu.Lock()
	if c.m == nil {
		c.m = map[string]digestEntry{}
	}
	c.m[path] = digestEntry{size: fi.Size(), mod: fi.ModTime(), sum: sum}
	c.mu.Unlock()
	return sum, nil
}

// handleDownload serves GET /dl/protectorate-<os>-<arch>[.exe] and its
// .sha256 sibling (sha256sum format). The builds are not secret.
func (h *Hub) handleDownload(w http.ResponseWriter, r *http.Request) {
	file := r.PathValue("file")
	name, digest := strings.CutSuffix(file, ".sha256")
	if !downloadPlatforms[name] {
		http.NotFound(w, r)
		return
	}
	goos, arch, _ := strings.Cut(strings.TrimSuffix(strings.TrimPrefix(name, "protectorate-"), ".exe"), "-")
	path := h.binaryPath(goos, arch)
	fh, err := os.Open(path)
	if err != nil {
		http.Error(w, fmt.Sprintf("this hub has no %s build (run `make cross` and start the hub with --binary-dir)", name), http.StatusNotFound)
		return
	}
	defer fh.Close()
	fi, err := fh.Stat()
	if err != nil || !fi.Mode().IsRegular() {
		http.Error(w, "build unreadable", http.StatusInternalServerError)
		return
	}
	w.Header().Set("Cache-Control", "no-store")
	if digest {
		sum, err := h.digests.sum(path, fi)
		if err != nil {
			http.Error(w, "hash build: "+err.Error(), http.StatusInternalServerError)
			return
		}
		w.Header().Set("Content-Type", "text/plain; charset=utf-8")
		fmt.Fprintf(w, "%s  %s\n", sum, name)
		return
	}
	w.Header().Set("Content-Type", "application/octet-stream")
	w.Header().Set("Content-Disposition", `attachment; filename="`+name+`"`)
	http.ServeContent(w, r, name, fi.ModTime(), fh)
}
