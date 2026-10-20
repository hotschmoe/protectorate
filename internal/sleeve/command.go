package sleeve

import (
	"bytes"
	"context"
	"crypto/rand"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"github.com/hotschmoe/protectorate/internal/driver"
)

// readEnvFile recovers the caller env a sleeve was spawned with from its
// bwrap --args file, so later launches of the same sleeve keep it.
func (c Config) readEnvFile(name string) (map[string]string, error) {
	raw, err := os.ReadFile(c.envFile(name))
	if os.IsNotExist(err) {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("read sleeve env: %w", err)
	}
	parts := bytes.Split(bytes.TrimSuffix(raw, []byte{0}), []byte{0})
	if len(parts)%3 != 0 {
		return nil, fmt.Errorf("read sleeve env: malformed %s", c.envFile(name))
	}
	env := make(map[string]string, len(parts)/3)
	for i := 0; i < len(parts); i += 3 {
		if string(parts[i]) != "--setenv" {
			return nil, fmt.Errorf("read sleeve env: unexpected %q in %s", parts[i], c.envFile(name))
		}
		env[string(parts[i+1])] = string(parts[i+2])
	}
	return env, nil
}

// launch plans, and prepares the host side of, one run of info's harness
// with args, for a caller that supervises the process itself (a driver).
func (c Config) launch(info Info, args []string) (Launch, error) {
	bin, err := c.Locate(info.Harness)
	if err != nil {
		return Launch{}, err
	}
	env, err := c.readEnvFile(info.Name)
	if err != nil {
		return Launch{}, err
	}
	launch, err := c.Plan(Spec{
		BaseRepo: info.BaseRepo,
		BaseTip:  info.BaseTip,
		ReadOnly: info.ReadOnly,
		Name:     info.Name, Harness: info.Harness, Args: args, Workspace: info.Workspace, Env: env, Limits: info.Limits(),
	}, bin)
	if err != nil {
		return Launch{}, err
	}
	// systemd-run refuses a unit name that is still being torn down.
	c.waitGone(context.Background(), info.Name, 5*time.Second)
	if err := launch.Layout.Apply(context.Background()); err != nil {
		return Launch{}, fmt.Errorf("prepare sleeve layout: %w", err)
	}
	if err := c.writeEnvFile(info.Name, launch); err != nil {
		return Launch{}, err
	}
	return launch, nil
}

// Command returns the argv builder for a driver.Options.Command that runs
// the named sleeve's harness inside its sandbox and systemd scope, with the
// same mounts, credentials and limits the terminal path uses. The harness
// executable the driver names is replaced by the one Locate finds.
func (m *Manager) Command(name string) (func(bin string, args []string) ([]string, error), error) {
	info, err := m.Get(name)
	if err != nil {
		return nil, err
	}
	return func(_ string, args []string) ([]string, error) {
		l, err := m.cfg.launch(info, args)
		return l.Argv, err
	}, nil
}

const gitTimeout = 20 * time.Second

// CatalogOptions confines discovery to a temporary home and shared native auth.
// It creates no sleeve record, project mount, native thread or inference turn.
func (m *Manager) CatalogOptions() (driver.Options, error) {
	bin, err := m.cfg.Locate(Codex)
	if err != nil {
		return driver.Options{}, err
	}
	auth := m.cfg.authDir(Codex)
	fi, err := os.Stat(auth)
	if err != nil {
		return driver.Options{}, fmt.Errorf("codex auth directory unavailable (run protectorate login codex on the hub): %w", err)
	}
	if !fi.IsDir() {
		return driver.Options{}, fmt.Errorf("%w: codex auth path is not a directory", ErrInvalid)
	}
	codexHome := filepath.Join(m.cfg.UserHome, ".codex")
	links := append(append([]Link(nil), bin.Symlinks...), Link{Path: filepath.Join(codexHome, "auth.json"), Target: filepath.Join(auth, "auth.json")})
	return driver.Options{Command: func(_ string, args []string) ([]string, error) {
		return m.cfg.BoxArgv(Box{Unit: "prot-catalog-" + rand.Text(), Hostname: "catalog", Limits: Limits{CPUPercent: 100, MemoryBytes: 1 << 30},
			RO: bin.RO, ROFiles: bin.Files, Symlinks: links, RW: []string{auth}, Chdir: "/tmp",
			Env:     [][2]string{{"HOME", m.cfg.UserHome}, {"CODEX_HOME", codexHome}, {"PATH", strings.Join(m.cfg.PathDirs, ":")}},
			Command: append([]string{bin.Path}, args...),
		}), nil
	}}, nil
}

// Git runs a read-only git command on the named sleeve's workspace in a
// throwaway sandbox: no network, no home, nothing writable but a tmpfs, the
// workspace (and a linked worktree's repository) bound read-only. A
// workspace is written by a sandboxed agent, and its repository config can
// name programs git runs (filters, textconv, fsmonitor, signature helpers);
// in here they can neither read the user's files nor change anything.
func (m *Manager) Git(ctx context.Context, name, dir string, args ...string) (string, error) {
	argv, err := m.cfg.gitArgv(name, dir, args)
	if err != nil {
		return "", err
	}
	ctx, cancel := context.WithTimeout(ctx, gitTimeout)
	defer cancel()
	cmd := exec.CommandContext(ctx, argv[0], argv[1:]...)
	cmd.Env = []string{"PATH=/usr/bin:/bin"}
	var stdout, stderr bytes.Buffer
	cmd.Stdout, cmd.Stderr = &stdout, &stderr
	if err := cmd.Run(); err != nil {
		return "", fmt.Errorf("git %s: %w: %s", args[0], err, strings.TrimSpace(stderr.String()))
	}
	return stdout.String(), nil
}

func (c Config) gitArgv(name, dir string, args []string) ([]string, error) {
	if len(args) == 0 {
		return nil, fmt.Errorf("%w: git needs a command", ErrInvalid)
	}
	if err := c.CheckWorkspace(name, dir); err != nil {
		return nil, err
	}
	binds := []string{dir}
	for _, d := range linkedGitDirs(dir) {
		if within(d, dir) {
			continue
		}
		if err := c.CheckWorkspace(name, d); err != nil {
			return nil, fmt.Errorf("%w: the repository of %s is at %s, which is not readable: %w", ErrInvalid, dir, d, err)
		}
		binds = append(binds, d)
	}
	argv := []string{"bwrap", "--unshare-all", "--die-with-parent", "--new-session", "--tmpfs", "/"}
	argv = append(argv, c.System...)
	argv = append(argv, "--dev", "/dev", "--proc", "/proc", "--tmpfs", "/tmp")
	for _, d := range binds {
		argv = append(argv, "--ro-bind", d, d)
	}
	argv = append(argv, "--chdir", dir, "--clearenv",
		"--setenv", "PATH", "/usr/bin:/bin", "--setenv", "HOME", "/tmp", "--setenv", "LANG", "C.UTF-8",
		"--setenv", "GIT_CONFIG_NOSYSTEM", "1", "--setenv", "GIT_OPTIONAL_LOCKS", "0", "--setenv", "GIT_TERMINAL_PROMPT", "0",
		"--", "git", "--no-pager", "-c", "safe.directory=*", "-c", "core.fsmonitor=false", "-c", "core.hooksPath=/dev/null",
		"-c", "core.pager=cat", "-c", "diff.external=")
	return append(argv, args...), nil
}

// linkedGitDirs returns the git and common directory a workspace's .git file
// points to (a linked worktree or a submodule), resolved, or nothing.
func linkedGitDirs(dir string) []string {
	raw, err := os.ReadFile(filepath.Join(dir, ".git"))
	if err != nil {
		return nil
	}
	gd, ok := strings.CutPrefix(strings.TrimSpace(string(raw)), "gitdir:")
	if !ok {
		return nil
	}
	gd = strings.TrimSpace(gd)
	if !filepath.IsAbs(gd) {
		gd = filepath.Join(dir, gd)
	}
	var out []string
	if r, err := filepath.EvalSymlinks(gd); err == nil {
		out = append(out, r)
		if c, err := os.ReadFile(filepath.Join(r, "commondir")); err == nil {
			cd := strings.TrimSpace(string(c))
			if !filepath.IsAbs(cd) {
				cd = filepath.Join(r, cd)
			}
			if rc, err := filepath.EvalSymlinks(cd); err == nil {
				out = append(out, rc)
			}
		}
	}
	return out
}
