package sleeve

import (
	"errors"
	"fmt"
	"maps"
	"os"
	"path/filepath"
	"slices"
	"strconv"
	"strings"

	"github.com/hotschmoe/protectorate/internal/fsutil"
)

// Spec is a validated request to run a harness.
type Spec struct {
	BaseRepo  string
	BaseTip   string
	Name      string
	Harness   Harness
	Args      []string
	Workspace string // absolute, symlinks resolved
	Env       map[string]string
	Limits    Limits
	ReadOnly  []Mount
	Nested    bool
}

type Mount struct {
	Source string `json:"source"`
	Target string `json:"target"`
}

func (c Config) ResolveMounts(mounts []Mount) ([]Mount, error) {
	out := make([]Mount, 0, len(mounts))
	for _, m := range mounts {
		if m.Target == "" {
			m.Target = m.Source
		}
		for _, p := range []string{m.Source, m.Target} {
			if err := c.CheckBind(p); err != nil {
				return nil, err
			}
			if within(p, c.Home) {
				return nil, fmt.Errorf("%w: read-only mount touches hub state", ErrInvalid)
			}
		}
		r, err := filepath.EvalSymlinks(m.Source)
		if err != nil {
			return nil, fmt.Errorf("%w: resolve read-only mount: %w", ErrInvalid, err)
		}
		if err := c.CheckBind(r); err != nil {
			return nil, err
		}
		if within(r, c.Home) {
			return nil, fmt.Errorf("%w: read-only mount resolves into hub state", ErrInvalid)
		}
		fi, err := os.Stat(r)
		if err != nil {
			return nil, fmt.Errorf("%w: stat read-only source %s: %w", ErrInvalid, r, err)
		}
		if !fi.IsDir() {
			return nil, fmt.Errorf("%w: read-only source %s is not a directory", ErrInvalid, r)
		}
		parent := m.Target
		for {
			resolved, err := filepath.EvalSymlinks(parent)
			if err == nil {
				rel, _ := filepath.Rel(parent, m.Target)
				resolved = filepath.Join(resolved, rel)
				if err := c.CheckBind(resolved); err != nil {
					return nil, err
				}
				if within(resolved, c.Home) {
					return nil, fmt.Errorf("%w: read-only target resolves into hub state", ErrInvalid)
				}
				break
			}
			if !os.IsNotExist(err) || parent == "/" {
				return nil, fmt.Errorf("%w: resolve read-only target: %w", ErrInvalid, err)
			}
			parent = filepath.Dir(parent)
		}
		out = append(out, Mount{Source: r, Target: m.Target})
	}
	return out, nil
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
//
// A chain hop that lies under a credential directory is not mounted as a
// directory. The final regular file is bound read-only on its own (Files) and
// each symlink hop is recreated inside the sandbox (Symlinks).
type Binary struct {
	Path     string
	RO       []string
	Files    []string
	Symlinks []Link
}

// credentialFiles are base names that are never bound as a harness binary.
var credentialFiles = map[string]bool{
	"auth.json": true, "auth.json.lock": true, "credentials": true, "credentials.toml": true,
	".credentials.json": true, "agent_id": true, "token": true, "id_rsa": true, "id_ed25519": true,
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
		b, err := c.bindChain(p)
		if err != nil {
			return Binary{}, err
		}
		return b, nil
	}
	return Binary{}, fmt.Errorf("%w: %s not found in %s", ErrUnavailable, h.binary(), strings.Join(c.PathDirs, ":"))
}

// bindChain works out what the sandbox must mount so p runs: each directory
// along its symlink chain read-only, except under credential directories,
// where only the symlinks and the final executable file are provided.
func (c Config) bindChain(p string) (Binary, error) {
	b := Binary{Path: p}
	hop := p
	for i := 0; ; i++ {
		if i > 16 {
			return Binary{}, fmt.Errorf("%w: %s: too many symlinks", ErrUnavailable, p)
		}
		dir := filepath.Dir(hop)
		fi, err := os.Lstat(hop)
		if err != nil {
			return Binary{}, fmt.Errorf("%w: %s: %w", ErrUnavailable, p, err)
		}
		isLink := fi.Mode()&os.ModeSymlink != 0
		switch {
		case c.mounted(dir):
		case !c.touchesCredentials(dir):
			if !contains(b.RO, dir) {
				b.RO = append(b.RO, dir)
			}
		case isLink:
			t, err := os.Readlink(hop)
			if err != nil {
				return Binary{}, fmt.Errorf("%w: %s: %w", ErrUnavailable, p, err)
			}
			b.Symlinks = append(b.Symlinks, Link{Path: hop, Target: t})
		default:
			if !fi.Mode().IsRegular() || fi.Mode()&0o111 == 0 || credentialFiles[filepath.Base(hop)] {
				return Binary{}, fmt.Errorf("%w: %s resolves to %s, which is not a plain executable outside credentials", ErrUnavailable, p, hop)
			}
			b.Files = append(b.Files, hop)
		}
		if !isLink {
			return b, nil
		}
		t, err := os.Readlink(hop)
		if err != nil {
			return Binary{}, fmt.Errorf("%w: %s: %w", ErrUnavailable, p, err)
		}
		if !filepath.IsAbs(t) {
			t = filepath.Join(dir, t)
		}
		hop = filepath.Clean(t)
	}
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
func (c Config) CheckWorkspace(name, ws string) error {
	if err := c.CheckBind(ws); err != nil {
		return err
	}
	if within(ws, c.Home) {
		own := filepath.Join(c.workspacesDir(), name)
		if name == OrchestratorName {
			own = filepath.Join(c.Home, OrchestratorDir)
		}
		if !within(ws, own) {
			return fmt.Errorf("%w: workspace %s is protectorate state that sleeve %s does not own", ErrInvalid, ws, name)
		}
	}
	return nil
}

// CheckBind rejects a directory to bind read-write into a sandbox when it
// would shadow the sandbox or expose real credentials or the whole state
// directory: the secret-path rules every sandbox shares.
func (c Config) CheckBind(ws string) error {
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
	for _, d := range c.workspaceDenied() {
		if within(ws, d) {
			return bad("is inside " + d + ", which holds credentials or application data")
		}
		if within(d, ws) {
			return bad("contains " + d + ", which holds credentials or application data")
		}
	}
	if within(c.Home, ws) {
		return bad("contains the protectorate state directory")
	}
	return nil
}

var reserved = map[string]bool{
	"HOME": true, "PATH": true, "TERM": true, "USER": true, "LOGNAME": true, "SHELL": true, "LANG": true,
	"COLORTERM": true, "CLAUDE_CONFIG_DIR": true, "CLAUDE_SECURESTORAGE_CONFIG_DIR": true, "CODEX_HOME": true,
	"GROK_HOME": true, "PI_CODING_AGENT_DIR": true, "PI_CODING_AGENT_SESSION_DIR": true, sleevePathEnv: true,
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

// Box is one bwrap sandbox in a bounded cgroup: a PID, IPC and UTS
// namespace on a fresh root with the system directories and the tool dirs
// read-only, known secret files masked, and only the binds named here
// writable. Sleeves and node jobs are both boxes.
type Box struct {
	EmptyDirs      []string
	Mounts         []Mount
	WritableMounts []Mount

	Unit     string
	Hostname string
	Limits   Limits
	Nested   bool
	// Home is the host directory bound read-write at the user's home.
	Home string
	// RO, ROFiles and Symlinks add read-only directories, files and
	// symlinks at their host paths, after the tool dirs.
	RO       []string
	ROFiles  []string
	Symlinks []Link
	// RW are bound read-write at their host paths; Chdir is the working
	// directory inside.
	RW    []string
	Chdir string
	// Env is set after --clearenv. EnvFD makes bwrap read further
	// NUL-separated arguments (SetenvArgs) from fd 3, which keeps values out
	// of the process list.
	Env     [][2]string
	EnvFD   bool
	Command []string
}

// BoxArgv is the cgroup launcher and bwrap argv of b. It has no side effects;
// the caller validates the binds (CheckBind) and the limits.
func (c Config) BoxArgv(b Box) []string {
	argv := []string{"systemd-run", "--user", "--scope", "--quiet", "--collect", "--unit=" + b.Unit}
	if m := b.Limits.MemoryBytes; m != 0 {
		argv = append(argv, "-p", "MemoryMax="+strconv.FormatUint(m, 10), "-p", "MemorySwapMax=0")
	}
	if p := b.Limits.CPUPercent; p != 0 {
		argv = append(argv, "-p", "CPUQuota="+strconv.FormatUint(uint64(p), 10)+"%")
	}
	argv = append(argv, "-p", "TasksMax="+strconv.Itoa(c.TasksMax), "-p", "OOMPolicy=continue")
	if b.Nested {
		argv = append(argv, "-p", "Delegate=yes")
	}
	argv = append(argv, "--")
	if c.delegated {
		argv = nil
	}
	prefix := argv

	argv = []string{"bwrap", "--unshare-pid", "--unshare-ipc", "--unshare-uts", "--die-with-parent",
		"--hostname", b.Hostname, "--tmpfs", "/"}
	if c.delegated && !b.Nested {
		argv = append(argv, "--unshare-cgroup")
	}
	argv = append(argv, c.System...)
	argv = append(argv, "--dev", "/dev", "--proc", "/proc", "--tmpfs", "/tmp", "--tmpfs", "/var/tmp")
	if b.Home == "" {
		argv = append(argv, "--tmpfs", c.UserHome)
	} else {
		argv = append(argv, "--bind", b.Home, c.UserHome)
	}
	ro := append(append([]string(nil), c.ToolDirs...), b.RO...)
	for _, d := range ro {
		argv = append(argv, "--ro-bind", d, d)
	}
	for _, f := range b.ROFiles {
		argv = append(argv, "--ro-bind", f, f)
	}
	for _, k := range b.Symlinks {
		argv = append(argv, "--symlink", k.Target, k.Path)
	}
	for _, f := range c.masks(ro) {
		argv = append(argv, "--ro-bind", "/dev/null", f)
	}
	for _, d := range b.RW {
		argv = append(argv, "--bind", d, d)
	}
	for _, m := range b.Mounts {
		argv = append(argv, "--ro-bind", m.Source, m.Target)
	}
	for _, m := range b.WritableMounts {
		argv = append(argv, "--bind", m.Source, m.Target)
	}
	for _, p := range b.EmptyDirs {
		argv = append(argv, "--tmpfs", p)
	}
	if b.Nested {
		argv = append(argv, "--bind", filepath.Join(c.CgroupRoot, b.Unit+".scope", "delegated"), DelegatedCgroupRoot)
	}
	argv = append(argv, "--chdir", b.Chdir, "--clearenv")
	for _, kv := range b.Env {
		argv = append(argv, "--setenv", kv[0], kv[1])
	}
	if b.EnvFD {
		argv = append(argv, "--args", "3")
	}
	argv = append(append(argv, "--"), b.Command...)
	return append(prefix, c.scopeArgv(b, argv)...)
}

// SetenvArgs encodes env as bwrap --setenv arguments in the NUL-separated
// form of `bwrap --args`.
func SetenvArgs(env [][2]string) []byte {
	var out []byte
	for _, kv := range env {
		out = append(out, "--setenv\x00"+kv[0]+"\x00"+kv[1]+"\x00"...)
	}
	return out
}

// Plan builds the cgroup launcher and bwrap command for spec. It has no side
// effects; Layout.Apply prepares the host directories it refers to.
func (c Config) validateSpec(spec Spec) ([]Mount, error) {
	if err := ValidName(spec.Name); err != nil {
		return nil, err
	}
	if err := spec.Limits.Validate(); err != nil {
		return nil, err
	}
	if spec.Nested && (spec.Limits.CPUPercent == 0 || spec.Limits.MemoryBytes == 0) {
		return nil, fmt.Errorf("%w: nested sleeves require CPU and memory limits", ErrInvalid)
	}
	if err := checkEnv(spec.Env); err != nil {
		return nil, err
	}
	if err := c.CheckWorkspace(spec.Name, spec.Workspace); err != nil {
		return nil, err
	}
	for _, a := range spec.Args {
		if strings.ContainsRune(a, 0) {
			return nil, fmt.Errorf("%w: argument contains a NUL byte", ErrInvalid)
		}
	}
	mounts, err := c.ResolveMounts(spec.ReadOnly)
	if err != nil {
		return nil, err
	}
	for _, m := range mounts {
		if within(spec.Workspace, m.Target) {
			return nil, fmt.Errorf("%w: read-only mount overlaps workspace", ErrInvalid)
		}
	}
	return mounts, nil
}

func (c Config) launchEnv(spec Spec, lay Layout) [][2]string {
	env := [][2]string{
		{"HOME", c.UserHome},
		{"PATH", strings.Join(c.PathDirs, ":")},
		{sleevePathEnv, strings.Join(c.PathDirs, ":")},
		{"USER", c.User},
		{"LOGNAME", c.User},
		{"SHELL", "/bin/bash"},
		{"TERM", "xterm-256color"},
		{"COLORTERM", "truecolor"},
		{"LANG", "C.UTF-8"},
	}
	env = append(env, lay.Env...)
	n := 1
	if spec.Limits.CPUPercent > 0 {
		n = max(1, int(spec.Limits.CPUPercent/100))
	}
	for _, kv := range [][2]string{{"GOMAXPROCS", strconv.Itoa(n)}, {"GOFLAGS", "-p=" + strconv.Itoa(n)}, {"FLUTTER_TEST_CONCURRENCY", strconv.Itoa(n)}} {
		if _, ok := spec.Env[kv[0]]; !ok {
			env = append(env, kv)
		}
	}
	return env
}

func (c Config) Plan(spec Spec, bin Binary) (Launch, error) {
	mounts, err := c.validateSpec(spec)
	if err != nil {
		return Launch{}, err
	}
	lay := c.layout(spec.Name, spec.Harness, spec.Workspace)
	var emptyDirs []string
	if spec.BaseRepo != "" {
		// Mask sources live in the hub-owned sleeve directory, outside the
		// writable home, and are regular files: binds are mounted nodev.
		empty := filepath.Join(c.sleeveDir(spec.Name), "base-empty")
		lay.Seeds = append(lay.Seeds, Seed{empty, ""})
		base, err := c.baseMounts(spec.BaseRepo)
		if err != nil {
			return Launch{}, err
		}
		mounts = append(mounts, base...)
		for _, m := range base {
			if fi, err := os.Stat(filepath.Join(m.Target, "logs")); err == nil && fi.IsDir() {
				emptyDirs = append(emptyDirs, filepath.Join(m.Target, "logs"))
			}
			for _, f := range []string{"config", "config.worktree", "FETCH_HEAD"} {
				p := filepath.Join(m.Target, f)
				if _, err := os.Lstat(p); err == nil {
					if f == "config" && len(spec.BaseTip) == 64 {
						safe := filepath.Join(c.sleeveDir(spec.Name), "base-sha256.gitconfig")
						lay.Seeds = append(lay.Seeds, Seed{safe, "[core]\nrepositoryformatversion = 1\n[extensions]\nobjectFormat = sha256\n"})
						mounts = append(mounts, Mount{Source: safe, Target: p})
						continue
					}
					mounts = append(mounts, Mount{Source: empty, Target: p})
				}
			}
		}
	}
	unit := c.UnitName(spec.Name)
	var callerEnv [][2]string
	for _, k := range slices.Sorted(maps.Keys(spec.Env)) {
		callerEnv = append(callerEnv, [2]string{k, spec.Env[k]})
	}
	envArgs := SetenvArgs(callerEnv)
	command := []string{bin.Path}
	if spec.Harness == Shell && len(spec.Args) > 0 {
		command = spec.Args
	} else {
		command = append(command, spec.Args...)
	}
	argv := c.BoxArgv(Box{
		EmptyDirs: emptyDirs,
		Unit:      unit, Hostname: spec.Name, Limits: spec.Limits, Home: c.sleeveHome(spec.Name),
		Nested: spec.Nested,
		RO:     bin.RO, ROFiles: bin.Files, Symlinks: bin.Symlinks,
		Mounts:         mounts,
		WritableMounts: lay.CacheCopies,
		RW:             append(append([]string(nil), lay.Binds...), spec.Workspace), Chdir: spec.Workspace,
		Env: c.launchEnv(spec, lay), EnvFD: len(envArgs) > 0, Command: command,
	})
	launch := Launch{Argv: argv, Unit: unit, Layout: lay}
	if len(envArgs) > 0 {
		launch.EnvFile, launch.EnvArgs = c.envFile(spec.Name), envArgs
		launch.Argv = append([]string{"sh", "-c", envShim, "sh", launch.EnvFile}, argv...)
	}
	return launch, nil
}
