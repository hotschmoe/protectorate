// Package sleeve runs one harness in one workspace inside a bubblewrap
// sandbox in a systemd user scope, as a tmux session on the hub. Linux only.
package sleeve

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"os/user"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"time"
)

var (
	ErrNotFound    = errors.New("sleeve not found")
	ErrExists      = errors.New("sleeve already exists")
	ErrInvalid     = errors.New("invalid sleeve request")
	ErrUnsupported = errors.New("sleeves are linux-only")
	ErrUnavailable = errors.New("sleeve runtime unavailable")
)

type Harness string

const (
	Claude Harness = "claude"
	Codex  Harness = "codex"
	Grok   Harness = "grok"
	Pi     Harness = "pi"
	Shell  Harness = "shell"
)

func ParseHarness(s string) (Harness, error) {
	switch h := Harness(s); h {
	case Claude, Codex, Grok, Pi, Shell:
		return h, nil
	}
	return "", fmt.Errorf("%w: unknown harness %q (want claude, codex, grok, pi or shell)", ErrInvalid, s)
}

func (h Harness) binary() string {
	if h == Shell {
		return "bash"
	}
	return string(h)
}

type Limits struct {
	CPUPercent  uint32
	MemoryBytes uint64
}

const (
	minMemoryBytes = 64 << 20
	maxNameLen     = 40
	// DefaultUnitPrefix starts the name of every sleeve scope.
	DefaultUnitPrefix = "prot-sleeve-"
	sessionPrefix     = "sleeve-"
)

var (
	nameRE   = regexp.MustCompile(`^[a-z0-9][a-z0-9-]{0,39}$`)
	envKeyRE = regexp.MustCompile(`^[A-Za-z_][A-Za-z0-9_]*$`)
)

func ValidName(name string) error {
	if !nameRE.MatchString(name) {
		return fmt.Errorf("%w: name must be 1-%d of a-z 0-9 and '-', starting with a letter or digit", ErrInvalid, maxNameLen)
	}
	return nil
}

func SessionName(name string) string { return sessionPrefix + name }

// UnitName is the systemd unit (without .scope) of a sleeve.
func (c Config) UnitName(name string) string { return c.UnitPrefix + name }

// IsSleeveSession reports whether a session name is reserved for sleeves.
// Such sessions must only be started through the sleeve manager, which wraps
// them in the sandbox; plain session RPCs must refuse them.
func IsSleeveSession(name string) bool { return strings.HasPrefix(name, sessionPrefix) }

// Validate checks the limits a scope can enforce.
func (l Limits) Validate() error {
	if l.MemoryBytes != 0 && l.MemoryBytes < minMemoryBytes {
		return fmt.Errorf("%w: memory limit must be at least 64 MiB", ErrInvalid)
	}
	return nil
}

// Config holds the host facts the sandbox is built from.
type Config struct {
	Home     string   // PROTECTORATE_HOME
	UserHome string   // the hub user's home, also $HOME inside sleeves
	User     string   // user name exported as $USER
	System   []string // bwrap arguments for the read-only system mounts
	ToolDirs []string // existing directories bound read-only at the same path
	PathDirs []string // directories searched for harnesses and used as $PATH
	TasksMax int
	// UnitPrefix starts every scope name. Tests set their own so a real hub on
	// the same user manager never sees their scopes.
	UnitPrefix string
	// CgroupRoot is the cgroup directory that holds the user's scopes.
	CgroupRoot string
	// Protected are extra resolved paths that must never be mounted or used
	// as a workspace, such as the targets of symlinked credential dirs.
	Protected []string
	GitName   string
	GitEmail  string
}

// Names of the orchestrator sleeve and the state subdirectory that is its
// workspace and holds its token file.
const (
	OrchestratorName = "envoy"
	// OrchestratorTokenName names the token minted for the orchestrator.
	OrchestratorTokenName = "orchestrator-envoy"
	OrchestratorDir       = "orchestrator"
)

// defaultToolDirs are mounted read-only when they exist, relative to $HOME.
// They are narrow on purpose: ~/.cargo and ~/.local/share hold registry
// tokens and application data. Harnesses installed under ~/.local/share are
// reached through the symlink chain of their binary in ~/.local/bin.
var defaultToolDirs = []string{
	".local/bin", ".local/lib", ".local/opt", ".local/go", ".local/share/pi-omp-setup",
	".nvm", "go", ".cargo/bin", ".rustup", ".pub-cache/bin", ".pub-cache/global_packages",
}

// maskedFiles are known secret files under $HOME. Any that exists inside a
// mounted directory is overlaid with /dev/null.
var maskedFiles = []string{".cargo/credentials", ".cargo/credentials.toml"}

// credentialDirs under $HOME hold real logins and are never mounted or used
// as a workspace.
var credentialDirs = []string{".claude", ".codex", ".grok", ".pi", ".ssh", ".gnupg", ".aws"}

// secretPaths under $HOME hold other tokens or application data. Narrow tool
// mounts may reach inside some of them, but a workspace, which is bound
// read-write without masks, may neither be inside nor contain one.
var secretPaths = []string{
	".cargo", ".local/share", ".config", ".docker", ".kube", ".netrc", ".git-credentials",
	".npmrc", ".pypirc", ".password-store", ".pub-cache",
}

// DefaultConfig probes the host. toolDirs replaces the default toolchain
// list when non-nil; every entry must exist.
func DefaultConfig(home string, toolDirs []string) (Config, error) {
	uh, err := os.UserHomeDir()
	if err != nil {
		return Config{}, fmt.Errorf("locate user home: %w", err)
	}
	if uh, err = filepath.EvalSymlinks(uh); err != nil {
		return Config{}, fmt.Errorf("resolve user home: %w", err)
	}
	home, err = filepath.EvalSymlinks(home)
	if err != nil {
		return Config{}, fmt.Errorf("resolve state directory: %w", err)
	}
	c := Config{Home: home, UserHome: uh, TasksMax: 4096, UnitPrefix: DefaultUnitPrefix}
	for _, d := range credentialDirs {
		if r, err := filepath.EvalSymlinks(filepath.Join(uh, d)); err == nil {
			c.Protected = append(c.Protected, r)
		}
	}
	if u, err := user.Current(); err == nil {
		c.User = u.Username
	}
	if c.User == "" {
		c.User = filepath.Base(uh)
	}
	c.System = SystemMounts()
	c.CgroupRoot = filepath.Join("/sys/fs/cgroup/user.slice", fmt.Sprintf("user-%d.slice", os.Getuid()),
		fmt.Sprintf("user@%d.service", os.Getuid()), "app.slice")

	if toolDirs == nil {
		flutter := os.Getenv("PROTECTORATE_FLUTTER_SDK")
		if flutter == "" {
			if p, err := exec.LookPath("flutter"); err == nil {
				if p, err = filepath.EvalSymlinks(p); err == nil {
					flutter = filepath.Dir(filepath.Dir(p))
				}
			}
		}
		if flutter == "" {
			flutter = filepath.Join(uh, "development/flutter")
		}
		if fi, err := os.Stat(flutter); err == nil && fi.IsDir() {
			c.ToolDirs = append(c.ToolDirs, flutter)
		}
		for _, d := range defaultToolDirs {
			if fi, err := os.Stat(filepath.Join(uh, d)); err == nil && fi.IsDir() {
				c.ToolDirs = append(c.ToolDirs, filepath.Join(uh, d))
			}
		}
	} else {
		for _, d := range toolDirs {
			fi, err := os.Stat(d)
			if err != nil || !fi.IsDir() {
				return Config{}, fmt.Errorf("tool dir %q is not a directory", d)
			}
			c.ToolDirs = append(c.ToolDirs, filepath.Clean(d))
		}
	}
	for _, d := range c.ToolDirs {
		r, err := filepath.EvalSymlinks(d)
		if err != nil {
			return Config{}, fmt.Errorf("resolve tool dir %q: %w", d, err)
		}
		if c.touchesCredentials(d) || c.touchesCredentials(r) {
			return Config{}, fmt.Errorf("tool dir %q would expose credentials or hub state (it contains the user home or the state directory, or overlaps a credential dir)", d)
		}
	}
	c.PathDirs = c.defaultPath(os.Getenv("PATH"))
	for _, d := range c.ToolDirs {
		if _, err := os.Stat(filepath.Join(d, "bin/flutter")); err == nil {
			c.PathDirs = append(c.PathDirs, filepath.Join(d, "bin"), filepath.Join(d, "bin/cache/dart-sdk/bin"))
		}
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	for key, dst := range map[string]*string{"user.name": &c.GitName, "user.email": &c.GitEmail} {
		cmd := exec.CommandContext(ctx, "git", "config", "--global", "--get", key)
		if b, err := cmd.Output(); err == nil {
			*dst = strings.TrimSpace(string(b))
		}
		if *dst == "" {
			cmd := exec.CommandContext(ctx, "git", "config", "--file", filepath.Join(uh, "github/protectorate/.git/config"), "--no-includes", "--get", key)
			if b, err := cmd.Output(); err == nil {
				*dst = strings.TrimSpace(string(b))
			}
		}
	}
	return c, nil
}

// AddBinaryDir makes the directory holding exe, and every directory along its
// symlink chain, readable inside sleeves and last on their PATH, so a sleeve
// can run the protectorate CLI. Directories that are already mounted are not
// mounted again; one that would expose credentials is refused.
func (c *Config) AddBinaryDir(exe string) error {
	if !filepath.IsAbs(exe) {
		return fmt.Errorf("%w: binary path %q must be absolute", ErrInvalid, exe)
	}
	dirs := symlinkDirs(exe)
	for _, d := range dirs {
		if !c.mounted(d) && c.touchesCredentials(d) {
			return fmt.Errorf("%w: mounting %s would expose credentials or hub state (it contains the user home or the state directory, or overlaps a credential dir); put the binary somewhere like ~/.local/bin", ErrInvalid, d)
		}
	}
	for _, d := range dirs {
		if !c.mounted(d) {
			c.ToolDirs = append(c.ToolDirs, d)
		}
	}
	if dir := filepath.Dir(exe); !contains(c.PathDirs, dir) {
		c.PathDirs = append(append([]string(nil), c.PathDirs...), dir)
	}
	return nil
}

// protectedDirs lists the credential directories, lexical and resolved.
func (c Config) protectedDirs() []string {
	out := append([]string(nil), c.Protected...)
	for _, d := range credentialDirs {
		out = append(out, filepath.Join(c.UserHome, d))
	}
	return out
}

// workspaceDenied lists every path a workspace may neither be inside nor
// contain, lexically and resolved.
func (c Config) workspaceDenied() []string {
	out := c.protectedDirs()
	for _, d := range secretPaths {
		p := filepath.Join(c.UserHome, d)
		out = append(out, p)
		if r, err := filepath.EvalSymlinks(p); err == nil && r != p {
			out = append(out, r)
		}
	}
	return out
}

func (c Config) touchesCredentials(p string) bool {
	if within(c.UserHome, p) || within(c.Home, p) {
		return true
	}
	for _, d := range c.protectedDirs() {
		if within(p, d) || within(d, p) {
			return true
		}
	}
	return false
}

// masks returns the secret files that exist inside any of dirs.
func (c Config) masks(dirs []string) []string {
	var out []string
	for _, f := range maskedFiles {
		p := filepath.Join(c.UserHome, f)
		for _, d := range dirs {
			if !within(p, d) {
				continue
			}
			if _, err := os.Lstat(p); err == nil {
				out = append(out, p)
			}
			break
		}
	}
	return out
}

func (c Config) defaultPath(hostPath string) []string {
	var out []string
	seen := map[string]bool{}
	add := func(d string) {
		if d == "" || seen[d] {
			return
		}
		if fi, err := os.Stat(d); err != nil || !fi.IsDir() {
			return
		}
		seen[d] = true
		out = append(out, d)
	}
	for _, d := range filepath.SplitList(hostPath) {
		if c.underTools(d) {
			add(d)
		}
	}
	for _, d := range []string{".local/bin", ".local/go/bin", ".cargo/bin", "go/bin", ".pub-cache/bin"} {
		if c.underTools(filepath.Join(c.UserHome, d)) {
			add(filepath.Join(c.UserHome, d))
		}
	}
	if nodes, _ := filepath.Glob(filepath.Join(c.UserHome, ".nvm/versions/node/*/bin")); len(nodes) > 0 {
		sort.Strings(nodes)
		if latest := nodes[len(nodes)-1]; c.underTools(latest) {
			add(latest)
		}
	}
	for _, d := range []string{"/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"} {
		add(d)
	}
	return out
}

func (c Config) underTools(p string) bool {
	for _, t := range c.ToolDirs {
		if within(p, t) {
			return true
		}
	}
	return false
}

// SystemMounts returns the bwrap arguments that expose the host's system
// directories read-only and nothing else.
func SystemMounts() []string {
	args := []string{"--ro-bind", "/usr", "/usr"}
	for _, p := range []string{"/bin", "/sbin", "/lib", "/lib32", "/lib64"} {
		fi, err := os.Lstat(p)
		switch {
		case err != nil:
		case fi.Mode()&os.ModeSymlink != 0:
			if t, err := os.Readlink(p); err == nil {
				args = append(args, "--symlink", t, p)
			}
		case fi.IsDir():
			args = append(args, "--ro-bind", p, p)
		}
	}
	args = append(args, "--ro-bind", "/etc", "/etc")
	if r, err := filepath.EvalSymlinks("/etc/resolv.conf"); err == nil && !within(r, "/etc") && !within(r, "/usr") {
		d := filepath.Dir(r)
		args = append(args, "--ro-bind", d, d)
	}
	return args
}

// within reports whether p is root or lies under it.
func within(p, root string) bool {
	rel, err := filepath.Rel(root, p)
	if err != nil {
		return false
	}
	return rel == "." || (rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator)))
}

func (c Config) sleeveDir(name string) string  { return filepath.Join(c.Home, "sleeves", name) }
func (c Config) sleeveHome(name string) string { return filepath.Join(c.sleeveDir(name), "home") }
func (c Config) authDir(h Harness) string      { return filepath.Join(c.Home, "auth", string(h)) }
func (c Config) workspacesDir() string         { return filepath.Join(c.Home, "workspaces") }
