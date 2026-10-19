package sleeve

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"

	"github.com/hotschmoe/protectorate/internal/fsutil"
)

// Spec is a validated request to run a harness.
type Spec struct {
	Name      string
	Harness   Harness
	Args      []string
	Workspace string // absolute, symlinks resolved
	Env       map[string]string
	Limits    Limits
}

// Launch is everything needed to start a sleeve: the pane argv and the host
// filesystem layout it expects. Caller-supplied env values never appear in
// Argv, which is visible in /proc and in session listings: they travel in
// EnvFile, a 0600 file holding bwrap's NUL-separated --args form, which
// the pane's shell opens as fd 3 for `bwrap --args 3`. EnvFile is empty when
// there is no caller env.
type Launch struct {
	Argv    []string
	Unit    string
	Layout  Layout
	EnvFile string
	EnvArgs []byte
}

const envFileName = "env.args"

func (c Config) envFile(name string) string { return filepath.Join(c.sleeveDir(name), envFileName) }

// writeEnvFile stores the launch's caller env for the pane's shell, or drops
// a previous one when this launch has none.
func (c Config) writeEnvFile(name string, l Launch) error {
	if l.EnvFile == "" {
		return c.removeEnvFile(name)
	}
	if err := os.MkdirAll(filepath.Dir(l.EnvFile), 0o700); err != nil {
		return fmt.Errorf("create %s: %w", filepath.Dir(l.EnvFile), err)
	}
	if err := fsutil.WriteFile(l.EnvFile, l.EnvArgs, 0o600); err != nil {
		return fmt.Errorf("write sleeve env: %w", err)
	}
	return nil
}

func (c Config) removeEnvFile(name string) error {
	if err := os.Remove(c.envFile(name)); err != nil && !errors.Is(err, os.ErrNotExist) {
		return fmt.Errorf("remove sleeve env: %w", err)
	}
	return nil
}

// envShim opens the env file as fd 3 and execs the real command.
const envShim = `exec 3<"$1"; shift; exec "$@"`

// Binary is a harness executable located on the host, with the extra
// directories it needs mounted read-only to run.
type Binary struct {
	Path string
	RO   []string
}

var systemDirs = []string{"/usr", "/etc", "/bin", "/sbin", "/lib", "/lib32", "/lib64", "/proc", "/dev", "/sys"}

// Locate finds the harness binary on the sandbox PATH and works out which
// directories hold it after symlinks.
func (c Config) Locate(h Harness) (Binary, error) {
	for _, d := range c.PathDirs {
		p := filepath.Join(d, h.binary())
		fi, err := os.Stat(p)
		if err != nil || fi.IsDir() || fi.Mode()&0o111 == 0 {
			continue
		}
		b := Binary{Path: p}
		for _, dir := range symlinkDirs(p) {
			if !c.mounted(dir) && !contains(b.RO, dir) {
				b.RO = append(b.RO, dir)
			}
		}
		return b, nil
	}
	return Binary{}, fmt.Errorf("%w: %s not found in %s", ErrUnavailable, h.binary(), strings.Join(c.PathDirs, ":"))
}

// symlinkDirs returns the directory of p and of every symlink hop it
// resolves through, so each can be mounted.
func symlinkDirs(p string) []string {
	dirs := []string{filepath.Dir(p)}
	for i := 0; i < 16; i++ {
		fi, err := os.Lstat(p)
		if err != nil || fi.Mode()&os.ModeSymlink == 0 {
			break
		}
		t, err := os.Readlink(p)
		if err != nil {
			break
		}
		if !filepath.IsAbs(t) {
			t = filepath.Join(filepath.Dir(p), t)
		}
		p = filepath.Clean(t)
		dirs = append(dirs, filepath.Dir(p))
	}
	return dirs
}

func (c Config) mounted(p string) bool {
	if c.underTools(p) {
		return true
	}
	for _, d := range []string{"/usr", "/bin", "/sbin", "/lib", "/lib32", "/lib64"} {
		if within(p, d) {
			return true
		}
	}
	return false
}

func contains(l []string, s string) bool {
	for _, v := range l {
		if v == s {
			return true
		}
	}
	return false
}

// CheckWorkspace rejects directories that would shadow the sandbox or expose
// real credentials or hub state.
func (c Config) CheckWorkspace(ws string) error {
	if !filepath.IsAbs(ws) || filepath.Clean(ws) != ws {
		return fmt.Errorf("%w: workspace %q must be a clean absolute path", ErrInvalid, ws)
	}
	bad := func(why string) error { return fmt.Errorf("%w: workspace %s %s", ErrInvalid, ws, why) }
	if ws == "/" {
		return bad("is the filesystem root")
	}
	for _, d := range systemDirs {
		if within(ws, d) {
			return bad("is a system directory")
		}
	}
	if within(c.UserHome, ws) {
		return bad("contains the user home directory")
	}
	if within(c.Home, ws) {
		return bad("contains the protectorate state directory")
	}
	for _, d := range credentialDirs {
		if within(ws, filepath.Join(c.UserHome, d)) {
			return bad("is inside a directory holding harness credentials")
		}
	}
	for _, d := range []string{"auth", "sleeves"} {
		if within(ws, filepath.Join(c.Home, d)) {
			return bad("is inside protectorate state")
		}
	}
	return nil
}

var reserved = map[string]bool{
	"HOME": true, "PATH": true, "TERM": true, "USER": true, "LOGNAME": true, "SHELL": true, "LANG": true,
	"COLORTERM": true, "CLAUDE_CONFIG_DIR": true, "CLAUDE_SECURESTORAGE_CONFIG_DIR": true, "CODEX_HOME": true,
	"GROK_HOME": true, "PI_CODING_AGENT_DIR": true, "PI_CODING_AGENT_SESSION_DIR": true,
}

func checkEnv(env map[string]string) error {
	for k, v := range env {
		switch {
		case !envKeyRE.MatchString(k):
			return fmt.Errorf("%w: invalid env name %q", ErrInvalid, k)
		case reserved[k]:
			return fmt.Errorf("%w: env %s is managed by the sleeve", ErrInvalid, k)
		case strings.ContainsRune(v, 0):
			return fmt.Errorf("%w: env %s contains a NUL byte", ErrInvalid, k)
		}
	}
	return nil
}

// Plan builds the systemd-run and bwrap command for spec. It has no side
// effects; Layout.Apply prepares the host directories it refers to.
func (c Config) Plan(spec Spec, bin Binary) (Launch, error) {
	if err := ValidName(spec.Name); err != nil {
		return Launch{}, err
	}
	if err := spec.Limits.validate(); err != nil {
		return Launch{}, err
	}
	if err := checkEnv(spec.Env); err != nil {
		return Launch{}, err
	}
	if err := c.CheckWorkspace(spec.Workspace); err != nil {
		return Launch{}, err
	}
	for _, a := range spec.Args {
		if strings.ContainsRune(a, 0) {
			return Launch{}, fmt.Errorf("%w: argument contains a NUL byte", ErrInvalid)
		}
	}
	lay := c.layout(spec.Name, spec.Harness)
	unit := UnitName(spec.Name)

	argv := []string{"systemd-run", "--user", "--scope", "--quiet", "--collect", "--unit=" + unit}
	if m := spec.Limits.MemoryBytes; m != 0 {
		argv = append(argv, "-p", "MemoryMax="+strconv.FormatUint(m, 10), "-p", "MemorySwapMax=0")
	}
	if p := spec.Limits.CPUPercent; p != 0 {
		argv = append(argv, "-p", "CPUQuota="+strconv.FormatUint(uint64(p), 10)+"%")
	}
	argv = append(argv, "-p", "TasksMax="+strconv.Itoa(c.TasksMax), "-p", "OOMPolicy=continue", "--")

	argv = append(argv, "bwrap", "--unshare-pid", "--unshare-ipc", "--unshare-uts", "--die-with-parent",
		"--hostname", spec.Name, "--tmpfs", "/")
	argv = append(argv, c.System...)
	argv = append(argv, "--dev", "/dev", "--proc", "/proc", "--tmpfs", "/tmp", "--tmpfs", "/var/tmp")
	argv = append(argv, "--bind", c.sleeveHome(spec.Name), c.UserHome)
	for _, d := range append(append([]string(nil), c.ToolDirs...), bin.RO...) {
		argv = append(argv, "--ro-bind", d, d)
	}
	for _, d := range lay.Binds {
		argv = append(argv, "--bind", d, d)
	}
	argv = append(argv, "--bind", spec.Workspace, spec.Workspace, "--chdir", spec.Workspace, "--clearenv")

	env := [][2]string{
		{"HOME", c.UserHome},
		{"PATH", strings.Join(c.PathDirs, ":")},
		{"USER", c.User},
		{"LOGNAME", c.User},
		{"SHELL", "/bin/bash"},
		{"TERM", "xterm-256color"},
		{"COLORTERM", "truecolor"},
		{"LANG", "C.UTF-8"},
	}
	env = append(env, lay.Env...)
	for _, kv := range env {
		argv = append(argv, "--setenv", kv[0], kv[1])
	}
	var envArgs []byte
	keys := make([]string, 0, len(spec.Env))
	for k := range spec.Env {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, k := range keys {
		envArgs = append(envArgs, "--setenv\x00"+k+"\x00"+spec.Env[k]+"\x00"...)
	}
	if len(envArgs) > 0 {
		argv = append(argv, "--args", "3")
	}
	argv = append(argv, "--", bin.Path)
	if spec.Harness == Shell && len(spec.Args) > 0 {
		argv = append(argv[:len(argv)-1], spec.Args...)
	} else {
		argv = append(argv, spec.Args...)
	}
	launch := Launch{Argv: argv, Unit: unit, Layout: lay}
	if len(envArgs) > 0 {
		launch.EnvFile, launch.EnvArgs = c.envFile(spec.Name), envArgs
		launch.Argv = append([]string{"sh", "-c", envShim, "sh", launch.EnvFile}, argv...)
	}
	return launch, nil
}
