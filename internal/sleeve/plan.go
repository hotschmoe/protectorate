package sleeve

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
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
// filesystem layout it expects.
type Launch struct {
	Argv   []string
	Unit   string
	Layout Layout
}

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
		real, err := filepath.EvalSymlinks(p)
		if err != nil {
			continue
		}
		b := Binary{Path: p}
		for _, dir := range []string{d, filepath.Dir(real)} {
			if !c.mounted(dir) && !contains(b.RO, dir) {
				b.RO = append(b.RO, dir)
			}
		}
		return b, nil
	}
	return Binary{}, fmt.Errorf("%w: %s not found in %s", ErrUnavailable, h.binary(), strings.Join(c.PathDirs, ":"))
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
	keys := make([]string, 0, len(spec.Env))
	for k := range spec.Env {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, k := range keys {
		env = append(env, [2]string{k, spec.Env[k]})
	}
	for _, kv := range env {
		argv = append(argv, "--setenv", kv[0], kv[1])
	}
	argv = append(argv, "--", bin.Path)
	if spec.Harness == Shell && len(spec.Args) > 0 {
		argv = append(argv[:len(argv)-1], spec.Args...)
	} else {
		argv = append(argv, spec.Args...)
	}
	return Launch{Argv: argv, Unit: unit, Layout: lay}, nil
}
