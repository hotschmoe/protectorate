package sleeve

import (
	"fmt"
	"os"
	"path/filepath"
	"time"
)

// Layout is the per-harness credential and config arrangement from
// ARCHITECTURE.md: shared auth directories bound read-write at their host
// path, per-sleeve config directories inside the sleeve's home.
type Layout struct {
	Dirs  []string
	Links []Link
	Seeds []Seed
	Binds []string
	Env   [][2]string
}

type Link struct{ Path, Target string }

// Seed is written only when the file does not exist yet.
type Seed struct {
	Path    string
	Content string
}

const claudeSeed = "{\"hasCompletedOnboarding\":true}\n"

// The sandbox is the boundary, so Codex runs without its own approval gate
// and sandbox; the user's config.toml, with its trust and model, is not copied.
const codexSeed = "approval_policy = \"never\"\nsandbox_mode = \"danger-full-access\"\n"

// profileSeed keeps the sleeve's PATH in login shells: /etc/profile resets
// PATH, and harnesses such as Codex run every command through `bash -lc`.
const profileSeed = "# Written by the protectorate hub: login shells keep the sleeve's PATH.\n" +
	"if [ -n \"$" + sleevePathEnv + "\" ]; then PATH=\"$" + sleevePathEnv + "\"; export PATH; fi\n"

const sleevePathEnv = "PROTECTORATE_SLEEVE_PATH"

// layout paths on the host live under the sleeve's home; the same paths
// relative to $HOME are what the harness sees.
func (c Config) layout(name string, h Harness) Layout {
	host := func(rel string) string { return filepath.Join(c.sleeveHome(name), rel) }
	inside := func(rel string) string { return filepath.Join(c.UserHome, rel) }
	l := Layout{Dirs: []string{c.sleeveHome(name)}, Seeds: []Seed{{host(".profile"), profileSeed}}}
	auth := c.authDir(h)
	shared := func() {
		l.Dirs = append(l.Dirs, auth)
		l.Binds = append(l.Binds, auth)
	}
	switch h {
	case Claude:
		shared()
		l.Dirs = append(l.Dirs, host(".claude"))
		l.Seeds = append(l.Seeds, Seed{host(".claude/.claude.json"), claudeSeed})
		l.Env = append(l.Env,
			[2]string{"CLAUDE_CONFIG_DIR", inside(".claude")},
			[2]string{"CLAUDE_SECURESTORAGE_CONFIG_DIR", auth},
			[2]string{"DISABLE_AUTOUPDATER", "1"})
	case Codex:
		shared()
		l.Dirs = append(l.Dirs, host(".codex"))
		l.Seeds = append(l.Seeds, Seed{host(".codex/config.toml"), codexSeed})
		l.Links = append(l.Links, Link{host(".codex/auth.json"), filepath.Join(auth, "auth.json")})
		l.Env = append(l.Env, [2]string{"CODEX_HOME", inside(".codex")})
	case Grok:
		shared()
		l.Dirs = append(l.Dirs, host(".grok"))
		l.Links = append(l.Links,
			Link{host(".grok/auth.json"), filepath.Join(auth, "auth.json")},
			Link{host(".grok/auth.json.lock"), filepath.Join(auth, "auth.json.lock")})
		l.Env = append(l.Env, [2]string{"GROK_HOME", inside(".grok")})
	case Pi:
		shared()
		l.Dirs = append(l.Dirs, host(".pi/sessions"))
		l.Env = append(l.Env,
			[2]string{"PI_CODING_AGENT_DIR", auth},
			[2]string{"PI_CODING_AGENT_SESSION_DIR", inside(".pi/sessions")})
	}
	return l
}

// Apply creates the directories, symlinks and seed files. It is idempotent
// and never deletes: a file that is in the way of a symlink is renamed aside.
func (l Layout) Apply() error {
	for _, d := range l.Dirs {
		if err := os.MkdirAll(d, 0o700); err != nil {
			return fmt.Errorf("create %s: %w", d, err)
		}
	}
	for _, s := range l.Seeds {
		f, err := os.OpenFile(s.Path, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o600)
		if os.IsExist(err) {
			continue
		}
		if err != nil {
			return fmt.Errorf("seed %s: %w", s.Path, err)
		}
		_, werr := f.WriteString(s.Content)
		if cerr := f.Close(); werr == nil {
			werr = cerr
		}
		if werr != nil {
			return fmt.Errorf("seed %s: %w", s.Path, werr)
		}
	}
	for _, k := range l.Links {
		if err := k.apply(); err != nil {
			return err
		}
	}
	return nil
}

func (k Link) apply() error {
	fi, err := os.Lstat(k.Path)
	switch {
	case os.IsNotExist(err):
	case err != nil:
		return fmt.Errorf("inspect %s: %w", k.Path, err)
	case fi.Mode()&os.ModeSymlink != 0:
		if t, err := os.Readlink(k.Path); err == nil && t == k.Target {
			return nil
		}
		fallthrough
	default:
		aside := fmt.Sprintf("%s.orphaned-%d", k.Path, time.Now().Unix())
		if err := os.Rename(k.Path, aside); err != nil {
			return fmt.Errorf("move %s aside: %w", k.Path, err)
		}
	}
	if err := os.Symlink(k.Target, k.Path); err != nil {
		return fmt.Errorf("link %s: %w", k.Path, err)
	}
	return nil
}
