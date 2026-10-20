package main

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"strings"

	"github.com/hotschmoe/protectorate/internal/hub"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

var loginHarnesses = []sleeve.Harness{sleeve.Claude, sleeve.Codex, sleeve.Grok, sleeve.Pi}

func loginCmd(o *opts) *command {
	var home, name, platform string
	var status bool
	c := &command{
		Use:   "login [claude|codex|grok|pi] [-- login args...]",
		Short: "Connect this device to a hub, or log a harness in on the hub",
		Long: `With no harness, connect this CLI to --hub through device approval. Match the
displayed code on a trusted device, or run devices approve <code> on the hub.
The device credential is stored privately for that hub; the owner token is untouched.

With a harness, run on the hub machine. Runs the harness's own login flow in this terminal
with the environment that stores its credential in $PROTECTORATE_HOME/auth/<harness>,
the directory every sleeve of that harness reads. Nothing is copied or printed.
Arguments after -- go to the harness's login command (codex login -- --device-auth).
pi has no login command: it starts pi; type /login, then exit.
--status lists which harnesses have a credential (file presence only).`,
		Args: arbitraryArgs,
		RunE: func(cmd *command, args []string) error {
			if len(args) == 0 && !status {
				return runDeviceLogin(cmd, o, home, name, platform)
			}
			if name != "" || platform != "" {
				return errors.New("--name and --platform apply only to device login")
			}
			cfg, err := loginConfig(home)
			if err != nil {
				return err
			}
			var extra []string
			if dash := cmd.ArgsLenAtDash(); dash >= 0 {
				args, extra = args[:dash], args[dash:]
			}
			if status {
				if len(args) > 0 {
					return errors.New("--status takes no harness")
				}
				return printLoginStatus(cmd, cfg)
			}
			if len(args) != 1 {
				return errors.New("usage: login <claude|codex|grok|pi> [-- login args...]")
			}
			h, err := sleeve.ParseHarness(args[0])
			if err != nil {
				return err
			}
			return runLogin(cmd.Context(), cmd, cfg, h, extra)
		},
	}
	c.Flags().StringVar(&home, "home", "", "PROTECTORATE_HOME (default: resolved state dir)")
	c.Flags().BoolVar(&status, "status", false, "list which harnesses are logged in")
	c.Flags().StringVar(&name, "name", "", "device name (default hostname)")
	c.Flags().StringVar(&platform, "platform", "", "device platform (default operating system and architecture)")
	return c
}

func loginConfig(home string) (sleeve.Config, error) {
	if home == "" {
		var err error
		if home, err = hub.Home(); err != nil {
			return sleeve.Config{}, err
		}
	}
	return sleeve.Config{Home: home}, nil
}

func printLoginStatus(cmd *command, cfg sleeve.Config) error {
	for _, h := range loginHarnesses {
		ok, err := cfg.LoggedIn(h)
		if err != nil {
			return err
		}
		state := "missing"
		if ok {
			state = "logged in"
		}
		fmt.Fprintf(cmd.OutOrStdout(), "%-7s %s\n", h, state)
	}
	return nil
}

func loginEnv(l sleeve.Login) []string {
	env := os.Environ()
	for _, kv := range l.Env {
		env = append(env, kv[0]+"="+kv[1])
	}
	return env
}

func runLogin(ctx context.Context, cmd *command, cfg sleeve.Config, h sleeve.Harness, extra []string) error {
	l, err := cfg.LoginPlan(h, extra)
	if err != nil {
		return err
	}
	if err := l.Prepare(); err != nil {
		return err
	}
	bin, err := exec.LookPath(l.Argv[0])
	if err != nil {
		return fmt.Errorf("%s is not installed on this machine: %w", h, err)
	}
	if l.Hint != "" {
		fmt.Fprintln(cmd.ErrOrStderr(), l.Hint)
	}
	login := exec.CommandContext(ctx, bin, l.Argv[1:]...)
	login.Env = loginEnv(l)
	login.Stdin, login.Stdout, login.Stderr = os.Stdin, os.Stdout, os.Stderr
	if err := login.Run(); err != nil {
		return fmt.Errorf("%s: %w", strings.Join(l.Argv, " "), err)
	}
	fmt.Fprintln(cmd.OutOrStdout(), loginReadiness(ctx, cfg, h, l))
	return nil
}

// loginReadiness prefers the CLI's own status command and always requires the
// credential file in the shared dir; it never prints either's output.
func loginReadiness(ctx context.Context, cfg sleeve.Config, h sleeve.Harness, l sleeve.Login) string {
	ok, err := cfg.LoggedIn(h)
	switch {
	case err != nil:
		return fmt.Sprintf("%s: cannot check credential: %v", h, err)
	case !ok:
		return fmt.Sprintf("%s: not ready, no credential in the shared auth dir", h)
	}
	if len(l.Status) > 0 {
		st := exec.CommandContext(ctx, l.Status[0], l.Status[1:]...)
		st.Env = loginEnv(l)
		if err := st.Run(); err != nil {
			return fmt.Sprintf("%s: credential present but `%s` failed: %v", h, strings.Join(l.Status, " "), err)
		}
	}
	return fmt.Sprintf("%s: ready, sleeves will use the shared credential", h)
}
