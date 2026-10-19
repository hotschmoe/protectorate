package sleeve

import (
	"fmt"
	"os"
	"path/filepath"
)

// Login is how to run a harness's own login flow so that its credential lands
// in the shared auth directory every sleeve reads.
type Login struct {
	Dirs []string
	Env  [][2]string
	Argv []string
	// Status is the harness's own non-refreshing readiness command, if any.
	Status []string
	// CredFile is the file whose presence means the harness is logged in.
	CredFile string
	// Hint is printed before Argv runs.
	Hint string
}

// LoginPlan builds the login for h. Codex and Grok write straight into the
// shared directory: through a symlink a temp+rename save would silently leave
// the credential in a scratch home instead.
func (c Config) LoginPlan(h Harness, extra []string) (Login, error) {
	auth := c.authDir(h)
	l := Login{Dirs: []string{auth}}
	switch h {
	case Claude:
		scratch := filepath.Join(c.Home, "login", string(h))
		l.Dirs = append(l.Dirs, scratch)
		l.Env = [][2]string{{"CLAUDE_CONFIG_DIR", scratch}, {"CLAUDE_SECURESTORAGE_CONFIG_DIR", auth}}
		l.Argv = append([]string{"claude", "auth", "login"}, extra...)
		l.Status = []string{"claude", "auth", "status", "--text"}
		l.CredFile = ".credentials.json"
	case Codex:
		l.Env = [][2]string{{"CODEX_HOME", auth}}
		l.Argv = append([]string{"codex", "login"}, extra...)
		l.Status = []string{"codex", "login", "status"}
		l.CredFile = "auth.json"
	case Grok:
		l.Env = [][2]string{{"GROK_HOME", auth}}
		l.Argv = append([]string{"grok", "login"}, extra...)
		l.CredFile = "auth.json"
	case Pi:
		if len(extra) > 0 {
			return Login{}, fmt.Errorf("%w: pi takes no login arguments", ErrInvalid)
		}
		l.Env = [][2]string{{"PI_CODING_AGENT_DIR", auth}}
		l.Argv = []string{"pi"}
		l.CredFile = "auth.json"
		l.Hint = "pi has no login command: type /login in its prompt, finish, then exit pi"
	default:
		return Login{}, fmt.Errorf("%w: %q has no login", ErrInvalid, h)
	}
	return l, nil
}

// Prepare creates the login directories.
func (l Login) Prepare() error {
	for _, d := range l.Dirs {
		if err := os.MkdirAll(d, 0o700); err != nil {
			return fmt.Errorf("create %s: %w", d, err)
		}
	}
	return nil
}

// LoggedIn reports from file presence only whether h has a credential.
func (c Config) LoggedIn(h Harness) (bool, error) {
	l, err := c.LoginPlan(h, nil)
	if err != nil {
		return false, err
	}
	fi, err := os.Stat(filepath.Join(c.authDir(h), l.CredFile))
	switch {
	case os.IsNotExist(err):
		return false, nil
	case err != nil:
		return false, fmt.Errorf("check %s credential: %w", h, err)
	}
	return fi.Mode().IsRegular() && fi.Size() > 0, nil
}
