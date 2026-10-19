// Package sleeve runs one harness in one workspace inside a bubblewrap
// sandbox in a systemd user scope, as a tmux session on the hub. Linux only.
package sleeve

import (
	"errors"
	"fmt"
	"os"
	"os/user"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
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
	unitPrefix     = "prot-sleeve-"
	sessionPrefix  = "sleeve-"
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

func UnitName(name string) string    { return unitPrefix + name }
func SessionName(name string) string { return sessionPrefix + name }

// IsSleeveSession reports whether a session name is reserved for sleeves.
// Such sessions must only be started through the sleeve manager, which wraps
// them in the sandbox; plain session RPCs must refuse them.
func IsSleeveSession(name string) bool { return strings.HasPrefix(name, sessionPrefix) }

func (l Limits) validate() error {
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
	// CgroupRoot is the cgroup directory that holds the user's scopes.
	CgroupRoot string
}

var defaultToolDirs = []string{".local", ".nvm", "go", ".cargo", ".rustup"}

// credentialDirs under $HOME hold real logins and are never mounted or used
// as a workspace.
var credentialDirs = []string{".claude", ".codex", ".grok", ".pi", ".ssh", ".gnupg", ".aws"}

// DefaultConfig probes the host. toolDirs replaces the default toolchain
// list when non-nil; every entry must exist.
func DefaultConfig(home string, toolDirs []string) (Config, error) {
	uh, err := os.UserHomeDir()
	if err != nil {
		return Config{}, fmt.Errorf("locate user home: %w", err)
	}
	c := Config{Home: home, UserHome: uh, TasksMax: 4096}
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
		if c.touchesCredentials(d) {
			return Config{}, fmt.Errorf("tool dir %q would expose harness credentials", d)
		}
	}
	c.PathDirs = c.defaultPath(os.Getenv("PATH"))
	return c, nil
}

func (c Config) touchesCredentials(p string) bool {
	if within(c.UserHome, p) {
		return true
	}
	for _, d := range credentialDirs {
		if within(p, filepath.Join(c.UserHome, d)) {
			return true
		}
	}
	return false
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
	for _, d := range []string{".local/bin", ".cargo/bin", "go/bin"} {
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
	return rel == "." || (rel != ".." && !strings.HasPrefix(rel, "../"))
}

func (c Config) sleeveDir(name string) string  { return filepath.Join(c.Home, "sleeves", name) }
func (c Config) sleeveHome(name string) string { return filepath.Join(c.sleeveDir(name), "home") }
func (c Config) authDir(h Harness) string      { return filepath.Join(c.Home, "auth", string(h)) }
func (c Config) workspacesDir() string         { return filepath.Join(c.Home, "workspaces") }
