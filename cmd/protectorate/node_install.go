package main

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"os/user"
	"path"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"time"

	"github.com/spf13/cobra"

	"github.com/hotschmoe/protectorate/internal/fsutil"
	"github.com/hotschmoe/protectorate/internal/hub"
	"github.com/hotschmoe/protectorate/internal/node"
	"github.com/hotschmoe/protectorate/internal/svcgen"
	"github.com/hotschmoe/protectorate/internal/tailnet"
)

const enrollKeyEnv = "PROTECTORATE_ENROLL_KEY"

type stepKind int

const (
	stepCopy stepKind = iota
	stepWrite
	stepEnroll
	stepRun
	stepNote
)

// installStep is one action of an install or uninstall. A plan is a list of
// steps and carries no secrets; the enrollment key is supplied at run time.
type installStep struct {
	Kind     stepKind
	Desc     string
	Src, Dst string
	Content  []byte
	Mode     os.FileMode
	UTF16    bool
	Argv     []string
	// Soft steps report failure and carry on.
	Soft bool
}

type installInput struct {
	GOOS         string
	Exe          string
	UserHome     string
	LocalAppData string
	Prefix       string
	WinUser      string
	Home         string
	Control      string
	// Linger is loginctl's Linger value for the user (linux): yes, no, or empty when unknown.
	Linger    string
	Name      string
	Elevated  bool
	ServeArgs []string
	// Self marks an uninstall the agent runs on itself.
	Self bool
}

// pj joins path elements the way the target OS does, whatever OS plans.
func pj(goos string, elems ...string) string {
	if goos == "windows" {
		return strings.Join(elems, `\`)
	}
	return path.Join(elems...)
}

func (in installInput) binaryPath() (string, error) {
	switch in.GOOS {
	case "windows":
		if in.LocalAppData == "" {
			return "", errors.New("LOCALAPPDATA is not set")
		}
		return pj(in.GOOS, in.LocalAppData, "Programs", "protectorate", "protectorate.exe"), nil
	case "linux":
		return pj(in.GOOS, in.UserHome, ".local", "bin", "protectorate"), nil
	case "android":
		return pj(in.GOOS, in.Prefix, "bin", "protectorate"), nil
	}
	return "", fmt.Errorf("node install supports windows, linux and android (Termux), not %s", in.GOOS)
}

const (
	schtasks = "schtasks"
	taskName = "protectorate-node"
	unitName = "protectorate-node.service"
)

func (in installInput) spec(bin string) svcgen.Spec {
	return svcgen.Spec{Role: svcgen.RoleNode, Binary: bin, Args: in.ServeArgs, Home: in.Home, Elevated: in.Elevated, WinUser: in.WinUser}
}

// planInstall lists, in order, everything `node install` does.
func planInstall(in installInput) ([]installStep, error) {
	bin, err := in.binaryPath()
	if err != nil {
		return nil, err
	}
	if in.Control == "" {
		return nil, errors.New("control URL is required")
	}
	var steps []installStep
	if in.GOOS == "windows" {
		steps = append(steps, installStep{Kind: stepRun, Soft: true, Argv: []string{schtasks, "/End", "/TN", taskName},
			Desc: "stop the running agent, if any, so its binary can be replaced"})
	}
	if in.Exe == bin {
		steps = append(steps, installStep{Kind: stepNote, Desc: "binary already at " + bin})
	} else {
		steps = append(steps, installStep{Kind: stepCopy, Src: in.Exe, Dst: bin, Mode: 0o755, Desc: "copy the binary (replaces an older one)"})
	}
	steps = append(steps, installStep{Kind: stepEnroll, Dst: pj(in.GOOS, in.Home, "tsnet"),
		Desc: fmt.Sprintf("enroll as %s at %s (skipped when %s already holds an enrollment)", in.Name, in.Control, pj(in.GOOS, in.Home, "tsnet"))})

	s := in.spec(bin)
	switch in.GOOS {
	case "windows":
		xml, err := svcgen.TaskXML(s, svcgen.TriggerLogon)
		if err != nil {
			return nil, err
		}
		p := pj(in.GOOS, in.Home, taskName+".xml")
		steps = append(steps,
			installStep{Kind: stepWrite, Dst: p, Content: []byte(xml), Mode: 0o644, UTF16: true, Desc: "write the logon task definition"},
			installStep{Kind: stepRun, Argv: []string{schtasks, "/Create", "/TN", taskName, "/XML", p, "/F"}, Desc: "register the logon task for the current user (replaces an existing one)"},
			installStep{Kind: stepRun, Argv: []string{schtasks, "/Run", "/TN", taskName}, Desc: "start the agent now"},
		)
		if in.Elevated {
			steps = append(steps, installStep{Kind: stepNote, Desc: "--elevated: the task runs with the highest available privileges, so registering it needs an elevated prompt (Run as administrator); it exists to drive elevated windows, which input from a normal process cannot reach (UIPI)"})
		}
	case "linux":
		unit, err := svcgen.Systemd(s)
		if err != nil {
			return nil, err
		}
		steps = append(steps,
			installStep{Kind: stepWrite, Dst: pj(in.GOOS, in.UserHome, ".config", "systemd", "user", unitName), Content: []byte(unit), Mode: 0o644, Desc: "write the systemd user unit"},
			installStep{Kind: stepRun, Argv: []string{"systemctl", "--user", "daemon-reload"}, Desc: "reload the user manager"},
			installStep{Kind: stepRun, Argv: []string{"systemctl", "--user", "enable", unitName}, Desc: "start the agent at login"},
			installStep{Kind: stepRun, Argv: []string{"systemctl", "--user", "restart", unitName}, Desc: "start the agent now, or restart it onto the new binary"},
		)
		switch in.Linger {
		case "yes":
			steps = append(steps, installStep{Kind: stepNote, Desc: "linger is enabled"})
		case "no":
			steps = append(steps, installStep{Kind: stepNote, Desc: "to keep the agent running while you are logged out, run once as root: loginctl enable-linger $USER (not done for you)"})
		}
	case "android":
		run, err := svcgen.TermuxRun(s)
		if err != nil {
			return nil, err
		}
		steps = append(steps,
			installStep{Kind: stepWrite, Dst: pj(in.GOOS, in.Prefix, "var", "service", taskName, "run"), Content: []byte(run), Mode: 0o755, Desc: "write the termux-services run script"},
			installStep{Kind: stepWrite, Dst: pj(in.GOOS, in.UserHome, ".termux", "boot", taskName), Content: []byte(svcgen.TermuxBoot()), Mode: 0o755, Desc: "write the Termux:Boot script"},
			installStep{Kind: stepRun, Soft: true, Argv: []string{"sv-enable", taskName}, Desc: "enable the service (needs `pkg install termux-services` and a Termux restart)"},
			installStep{Kind: stepNote, Desc: "install the Termux:Boot app and open it once so the agent starts after a reboot"},
		)
	}
	return steps, nil
}

// planUninstall lists what `node uninstall` does: unregister and stop, never
// delete. It also returns what is left for the user to remove, which the
// closing note names. With in.Self the agent uninstalls itself: it only
// unregisters, then exits on its own, because stopping it through its
// startup registration would end it before the rest ran.
func planUninstall(in installInput) ([]installStep, []string, error) {
	bin, err := in.binaryPath()
	if err != nil {
		return nil, nil, err
	}
	var steps []installStep
	var left []string
	switch in.GOOS {
	case "windows":
		if !in.Self {
			steps = append(steps, installStep{Kind: stepRun, Soft: true, Argv: []string{schtasks, "/End", "/TN", taskName}, Desc: "stop the running agent"})
		}
		steps = append(steps, installStep{Kind: stepRun, Soft: true, Argv: []string{schtasks, "/Delete", "/TN", taskName, "/F"}, Desc: "unregister the logon task"})
		left = append(left, pj(in.GOOS, in.Home, taskName+".xml"))
	case "linux":
		if in.Self {
			steps = append(steps, installStep{Kind: stepRun, Soft: true, Argv: []string{"systemctl", "--user", "disable", unitName}, Desc: "disable the unit"})
		} else {
			steps = append(steps, installStep{Kind: stepRun, Soft: true, Argv: []string{"systemctl", "--user", "disable", "--now", unitName}, Desc: "stop the agent and disable the unit"})
		}
		left = append(left, pj(in.GOOS, in.UserHome, ".config", "systemd", "user", unitName))
	case "android":
		steps = append(steps, installStep{Kind: stepRun, Soft: true, Argv: []string{"sv-disable", taskName}, Desc: "stop the agent and disable the service"})
		left = append(left, pj(in.GOOS, in.Prefix, "var", "service", taskName), pj(in.GOOS, in.UserHome, ".termux", "boot", taskName))
	}
	left = append(left, bin, in.Home+" (enrollment and node state)")
	steps = append(steps, installStep{Kind: stepNote, Desc: "nothing was deleted. You may delete: " + strings.Join(left, ", ")})
	if !in.Self {
		steps = append(steps, installStep{Kind: stepNote, Desc: "to remove the node from the tailnet too, run on the hub: protectorate node rm " + in.Name})
	}
	return steps, left, nil
}

// agentUninstaller lets a running agent uninstall itself on the hub's
// request; stop ends the agent once the steps ran.
func agentUninstaller(w io.Writer, stop func()) func() ([]string, []string, func(), error) {
	return func() ([]string, []string, func(), error) {
		in, err := currentInstallInput()
		if err != nil {
			return nil, nil, nil, err
		}
		in.Self = true
		steps, left, err := planUninstall(in)
		if err != nil {
			return nil, nil, nil, err
		}
		var actions []string
		for _, s := range steps {
			if s.Kind != stepNote {
				actions = append(actions, s.Desc)
			}
		}
		actions = append(actions, "stop the agent")
		run := func() {
			fmt.Fprintln(w, "node agent: uninstalling on the hub's request")
			if err := runSteps(context.Background(), w, w, steps, nil); err != nil {
				fmt.Fprintf(w, "node agent: uninstall: %v\n", err)
			}
			stop()
		}
		return actions, left, run, nil
	}
}

// describe renders one step for a dry run or progress output.
func (s installStep) describe() string {
	switch s.Kind {
	case stepCopy:
		return fmt.Sprintf("copy %s -> %s\n    %s", s.Src, s.Dst, s.Desc)
	case stepWrite:
		body := strings.TrimRight(string(s.Content), "\n")
		return fmt.Sprintf("write %s (mode %04o)\n    %s\n      | %s", s.Dst, s.Mode, s.Desc, strings.ReplaceAll(body, "\n", "\n      | "))
	case stepEnroll:
		return "enroll\n    " + s.Desc
	case stepRun:
		soft := ""
		if s.Soft {
			soft = " (failure tolerated)"
		}
		return fmt.Sprintf("run %s%s\n    %s", strings.Join(s.Argv, " "), soft, s.Desc)
	}
	return "note: " + s.Desc
}

// resolveKey picks the enrollment key: --key, --key-file or the environment.
// Like other secrets, a file or the environment keeps it out of the process
// list.
func resolveKey(flag, file, env string) (string, error) {
	switch {
	case flag != "" && file != "":
		return "", errors.New("--key and --key-file are exclusive")
	case flag != "":
		return flag, nil
	case file != "":
		b, err := os.ReadFile(file)
		if err != nil {
			return "", fmt.Errorf("read key file: %w", err)
		}
		if k := strings.TrimSpace(string(b)); k != "" {
			return k, nil
		}
		return "", fmt.Errorf("key file %s is empty", file)
	}
	return strings.TrimSpace(env), nil
}

func currentInstallInput() (installInput, error) {
	exe, err := os.Executable()
	if err != nil {
		return installInput{}, fmt.Errorf("locate this executable: %w", err)
	}
	if r, err := filepath.EvalSymlinks(exe); err == nil {
		exe = r
	}
	home, err := hub.Home()
	if err != nil {
		return installInput{}, err
	}
	in := installInput{GOOS: runtime.GOOS, Exe: exe, Home: home, LocalAppData: os.Getenv("LOCALAPPDATA"), Prefix: envOr("PREFIX", "/data/data/com.termux/files/usr")}
	if in.UserHome, err = os.UserHomeDir(); err != nil {
		return in, fmt.Errorf("locate home dir: %w", err)
	}
	if u, err := user.Current(); err == nil && runtime.GOOS == "windows" {
		in.WinUser = u.Username
	}
	if runtime.GOOS == "linux" {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		out, _ := exec.CommandContext(ctx, "loginctl", "show-user", strconv.Itoa(os.Getuid()), "-p", "Linger").Output()
		in.Linger = lingerValue(string(out))
	}
	return in, nil
}

// lingerValue parses `loginctl show-user -p Linger` output.
func lingerValue(out string) string {
	switch strings.TrimSpace(out) {
	case "Linger=yes":
		return "yes"
	case "Linger=no":
		return "no"
	}
	return ""
}

func nodeInstallCmd() *cobra.Command {
	var control, key, keyFile, name, homeDir string
	var elevated, dryRun, runner bool
	c := &cobra.Command{
		Use:   "install --control <headscale url> --key <enrollment key> [flags] [-- serve-flags] | install --runner",
		Short: "Install this machine as a node: binary, enrollment, start at login",
		Long: `One command per machine. It copies this binary to a stable per-user place,
enrolls with the hub (node join), registers the agent to start at login and
starts it. Run it again to update; it is idempotent.

  Windows  %LOCALAPPDATA%\Programs\protectorate\protectorate.exe, a logon task for
           the current user (no elevation needed; --elevated asks for the highest
           privileges and needs an elevated prompt)
  Linux    ~/.local/bin/protectorate and a systemd user unit
  Termux   $PREFIX/bin/protectorate, termux-services and Termux:Boot scripts

The control URL and key are what 'node invite <name>' prints. The key is short
lived; pass it in env PROTECTORATE_ENROLL_KEY or --key-file to keep it out of
the process list. An existing enrollment is kept and needs no key.
--dry-run prints every action and changes nothing. Arguments after -- go to
'node serve'.

--runner (Windows, once, from an elevated prompt as the user the agent runs
as) instead creates the local account protectorate-runner that sandboxed jobs
run as, inside a job object, without access to your profile. It prints each
step as it goes (with --dry-run, only prints them). Restart the agent after.`,
		Args: cobra.ArbitraryArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			if homeDir != "" {
				if err := os.Setenv("PROTECTORATE_HOME", homeDir); err != nil {
					return fmt.Errorf("set home: %w", err)
				}
			}
			if runner {
				return installRunner(cmd.OutOrStdout(), dryRun)
			}
			if control == "" {
				return errors.New(`required flag(s) "control" not set`)
			}
			in, err := currentInstallInput()
			if err != nil {
				return err
			}
			in.Control, in.Name, in.Elevated, in.ServeArgs = control, name, elevated, args
			if in.Name == "" {
				in.Name = tailnet.SanitizeLabel(node.Hostname())
			}
			steps, err := planInstall(in)
			if err != nil {
				return err
			}
			w := cmd.OutOrStdout()
			if dryRun {
				fmt.Fprintln(w, "dry run: nothing will be changed")
				printSteps(w, steps)
				return nil
			}
			if elevated && in.GOOS == "windows" && !processElevated() {
				return errors.New("--elevated needs an elevated prompt: run this from a terminal opened with Run as administrator")
			}
			k, err := resolveKey(key, keyFile, os.Getenv(enrollKeyEnv))
			if err != nil {
				return err
			}
			if k == "" && !tailnet.Joined(filepath.Join(in.Home, "tsnet")) {
				return fmt.Errorf("an enrollment key is required: --key, --key-file or env %s", enrollKeyEnv)
			}
			return runSteps(cmd.Context(), w, cmd.ErrOrStderr(), steps, func(ctx context.Context) error {
				return enrollNode(ctx, in.Home, in.Control, k, in.Name)
			})
		},
	}
	f := c.Flags()
	f.StringVar(&control, "control", "", "headscale URL, as printed by `node invite`")
	f.StringVar(&key, "key", "", "enrollment key from `node invite` (prefer --key-file or env "+enrollKeyEnv+")")
	f.StringVar(&keyFile, "key-file", "", "file holding the enrollment key")
	f.StringVar(&name, "name", "", "node name (default: sanitised hostname)")
	f.StringVar(&homeDir, "home", "", "PROTECTORATE_HOME for this node (default: env, then ~/.protectorate)")
	f.BoolVar(&elevated, "elevated", false, "Windows: run the task with the highest privileges, to drive elevated windows; needs an elevated prompt")
	f.BoolVar(&dryRun, "dry-run", false, "print every action without doing it")
	f.BoolVar(&runner, "runner", false, "Windows, elevated, once: create the protectorate-runner account that sandboxed jobs run as")
	return c
}

func installRunner(w io.Writer, dryRun bool) error {
	home, err := hub.Home()
	if err != nil {
		return err
	}
	if dryRun {
		fmt.Fprintln(w, "dry run: nothing will be changed")
		for i, s := range node.RunnerSteps(home, node.RunnerDir()) {
			fmt.Fprintf(w, "%d. %s\n", i+1, s)
		}
		return nil
	}
	if err := node.InstallRunner(home, w); err != nil {
		return err
	}
	fmt.Fprintln(w, "done: restart the agent (sign out and in, or node install again) so jobs run as "+node.RunnerUser)
	return nil
}

func nodeUninstallCmd() *cobra.Command {
	var name, homeDir string
	var dryRun bool
	c := &cobra.Command{
		Use:   "uninstall",
		Short: "Stop the agent and unregister its startup; deletes nothing",
		Long: `Unregisters the logon task or unit and stops the agent. It never deletes a
file or directory; it prints the paths you may delete yourself.`,
		Args: cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			if homeDir != "" {
				if err := os.Setenv("PROTECTORATE_HOME", homeDir); err != nil {
					return fmt.Errorf("set home: %w", err)
				}
			}
			in, err := currentInstallInput()
			if err != nil {
				return err
			}
			in.Name = name
			if in.Name == "" {
				in.Name = tailnet.SanitizeLabel(node.Hostname())
			}
			steps, _, err := planUninstall(in)
			if err != nil {
				return err
			}
			w := cmd.OutOrStdout()
			if dryRun {
				fmt.Fprintln(w, "dry run: nothing will be changed")
				printSteps(w, steps)
				return nil
			}
			return runSteps(cmd.Context(), w, cmd.ErrOrStderr(), steps, nil)
		},
	}
	c.Flags().StringVar(&name, "name", "", "node name, for the revoke hint (default: sanitised hostname)")
	c.Flags().StringVar(&homeDir, "home", "", "PROTECTORATE_HOME of this node (default: env, then ~/.protectorate)")
	c.Flags().BoolVar(&dryRun, "dry-run", false, "print every action without doing it")
	return c
}

func printSteps(w io.Writer, steps []installStep) {
	for i, s := range steps {
		fmt.Fprintf(w, "%d. %s\n", i+1, s.describe())
	}
}

func enrollNode(ctx context.Context, home, control, key, name string) error {
	dir := filepath.Join(home, "tsnet")
	ctx, cancel := context.WithTimeout(ctx, 90*time.Second)
	defer cancel()
	if err := os.MkdirAll(home, 0o700); err != nil {
		return fmt.Errorf("create %s: %w", home, err)
	}
	return tailnet.Join(ctx, dir, control, key, name)
}

// runSteps performs a plan. enroll is skipped when this machine is already
// enrolled.
func runSteps(ctx context.Context, w, ew io.Writer, steps []installStep, enroll func(context.Context) error) error {
	for i, s := range steps {
		fmt.Fprintf(w, "%d/%d %s\n", i+1, len(steps), strings.SplitN(s.describe(), "\n", 2)[0])
		var err error
		switch s.Kind {
		case stepCopy:
			err = copyBinary(s.Src, s.Dst, s.Mode)
		case stepWrite:
			err = writeStepFile(s)
		case stepEnroll:
			if tailnet.Joined(s.Dst) {
				fmt.Fprintln(w, "    already enrolled, kept")
			} else if enroll != nil {
				err = enroll(ctx)
			}
		case stepRun:
			err = runArgv(ctx, w, s.Argv)
		}
		if err == nil {
			continue
		}
		if s.Soft {
			fmt.Fprintf(ew, "    warning: %v\n", err)
			continue
		}
		return fmt.Errorf("%s: %w", strings.SplitN(s.describe(), "\n", 2)[0], err)
	}
	return nil
}

func copyBinary(src, dst string, mode os.FileMode) error {
	b, err := os.ReadFile(src)
	if err != nil {
		return fmt.Errorf("read %s: %w", src, err)
	}
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return fmt.Errorf("create %s: %w", filepath.Dir(dst), err)
	}
	if err := fsutil.WriteFile(dst, b, mode); err != nil {
		return fmt.Errorf("write %s: %w", dst, err)
	}
	return nil
}

func writeStepFile(s installStep) error {
	content := s.Content
	if s.UTF16 {
		content = svcgen.EncodeUTF16LE(string(content))
	}
	if err := os.MkdirAll(filepath.Dir(s.Dst), 0o755); err != nil {
		return fmt.Errorf("create %s: %w", filepath.Dir(s.Dst), err)
	}
	if err := fsutil.WriteFile(s.Dst, content, s.Mode); err != nil {
		return fmt.Errorf("write %s: %w", s.Dst, err)
	}
	return nil
}

func runArgv(ctx context.Context, w io.Writer, argv []string) error {
	out, err := exec.CommandContext(ctx, argv[0], argv[1:]...).CombinedOutput()
	if err != nil {
		return fmt.Errorf("%s: %w: %s", strings.Join(argv, " "), err, strings.TrimSpace(string(out)))
	}
	if o := strings.TrimSpace(string(out)); o != "" {
		fmt.Fprintln(w, "    "+strings.ReplaceAll(o, "\n", "\n    "))
	}
	return nil
}
