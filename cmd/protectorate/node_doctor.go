package main

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"time"

	"github.com/spf13/cobra"

	"github.com/hotschmoe/protectorate/internal/desk"
	"github.com/hotschmoe/protectorate/internal/tailnet"
)

type level int

const (
	levelOK level = iota
	levelInfo
	levelWarn
	levelFail
)

func (l level) String() string { return [...]string{"ok", "info", "warn", "FAIL"}[l] }

type check struct {
	Name   string
	Level  level
	Detail string
}

// unattendedCaveat is printed on Windows, where desk commands exist.
const unattendedCaveat = "Unattended use: desk input fails on a locked screen and on the logon screen, " +
	"so a machine agents will drive must stay logged in and unlocked. Options: a dedicated account that is " +
	"always logged in (auto-logon), and no lock on idle (screen saver without \"on resume, display logon screen\", " +
	"sleep set to never). This command changes none of it."

func writeDoctor(w io.Writer, goos string, checks []check) (failed int) {
	for _, c := range checks {
		fmt.Fprintf(w, "%-5s %-14s %s\n", c.Level, c.Name, c.Detail)
		if c.Level == levelFail {
			failed++
		}
	}
	if goos == "windows" {
		fmt.Fprintf(w, "\n%s\n", unattendedCaveat)
	}
	return failed
}

func sessionCheck(ready error, goos string) check {
	switch {
	case goos != "windows":
		return check{"session", levelInfo, "desk commands are Windows only; nothing to check on " + goos}
	case ready != nil:
		return check{"session", levelFail, ready.Error()}
	}
	return check{"session", levelOK, "interactive desktop session (desk commands can run)"}
}

func screenCheck(goos string, ready error, name string, err error) check {
	if goos != "windows" {
		return check{"screen", levelInfo, "not applicable"}
	}
	if ready != nil {
		return check{"screen", levelInfo, "not checked: this process is not in an interactive session"}
	}
	switch desk.ClassifyInputDesktop(name, err) {
	case desk.ScreenUnlocked:
		return check{"screen", levelOK, "unlocked"}
	case desk.ScreenLocked:
		return check{"screen", levelFail, "locked or showing the logon screen (input desktop is not the user's); desk input fails until it is unlocked"}
	}
	d := name
	if err != nil {
		d = err.Error()
	}
	return check{"screen", levelWarn, "state unknown: " + d}
}

// otherPIDs lists the PIDs in `tasklist /FO CSV /NH` output other than self.
func otherPIDs(csv string, self int) []int {
	var pids []int
	for _, line := range strings.Split(csv, "\n") {
		f := strings.Split(strings.TrimSpace(line), `","`)
		if len(f) < 2 {
			continue
		}
		if pid, err := strconv.Atoi(strings.Trim(f[1], `"`)); err == nil && pid != self {
			pids = append(pids, pid)
		}
	}
	return pids
}

func windowsStartupCheck(registered bool, pids []int) check {
	switch {
	case !registered:
		return check{"startup", levelFail, "logon task " + taskName + " is not registered: run `protectorate node install`"}
	case len(pids) == 0:
		return check{"startup", levelWarn, "logon task registered, but no agent process is running (it starts at the next logon, or: schtasks /Run /TN " + taskName + ")"}
	}
	return check{"startup", levelOK, fmt.Sprintf("logon task registered, agent running (pid %d)", pids[0])}
}

func systemdStartupCheck(enabled, active, linger string) check {
	enabled, active, linger = strings.TrimSpace(enabled), strings.TrimSpace(active), strings.TrimSpace(linger)
	switch {
	case enabled != "enabled":
		return check{"startup", levelFail, "systemd user unit is not enabled (" + orDash(enabled) + "): run `protectorate node install`"}
	case active != "active":
		return check{"startup", levelWarn, "unit enabled but " + orDash(active) + ": systemctl --user status " + unitName}
	case linger != "Linger=yes":
		return check{"startup", levelWarn, "unit enabled and active, but linger is off, so it stops when you log out: sudo loginctl enable-linger $USER"}
	}
	return check{"startup", levelOK, "systemd user unit enabled and active, linger on"}
}

func output(ctx context.Context, argv ...string) (string, error) {
	out, err := exec.CommandContext(ctx, argv[0], argv[1:]...).Output()
	return string(out), err
}

func startupCheck(ctx context.Context, in installInput) check {
	switch in.GOOS {
	case "windows":
		_, qerr := output(ctx, schtasks, "/Query", "/TN", taskName)
		img := filepath.Base(in.Exe)
		csv, _ := output(ctx, "tasklist", "/FI", "IMAGENAME eq "+img, "/FO", "CSV", "/NH")
		return windowsStartupCheck(qerr == nil, otherPIDs(csv, os.Getpid()))
	case "linux":
		enabled, _ := output(ctx, "systemctl", "--user", "is-enabled", unitName)
		active, _ := output(ctx, "systemctl", "--user", "is-active", unitName)
		linger, _ := output(ctx, "loginctl", "show-user", strconv.Itoa(os.Getuid()), "-p", "Linger")
		return systemdStartupCheck(enabled, active, linger)
	case "android":
		for _, p := range []string{pj(in.GOOS, in.Prefix, "var", "service", taskName, "run"), pj(in.GOOS, in.UserHome, ".termux", "boot", taskName)} {
			if _, err := os.Stat(p); err != nil {
				return check{"startup", levelFail, "missing " + p + ": run `protectorate node install`"}
			}
		}
		return check{"startup", levelOK, "termux-services and Termux:Boot scripts present (sv status " + taskName + " shows if it runs)"}
	}
	return check{"startup", levelInfo, "no startup registration for " + in.GOOS}
}

func hubCheck(ctx context.Context, url string, explicit bool) check {
	if !explicit {
		return check{"hub", levelInfo, "not checked: pass --hub URL (or env PROTECTORATE_HUB) to test reachability from here"}
	}
	ctx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return check{"hub", levelFail, fmt.Sprintf("bad hub URL %q: %v", url, err)}
	}
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		return check{"hub", levelFail, fmt.Sprintf("cannot reach %s: %v", url, err)}
	}
	res.Body.Close()
	return check{"hub", levelOK, fmt.Sprintf("%s answered HTTP %d", url, res.StatusCode)}
}

func enrollCheck(home string) check {
	if tailnet.Joined(filepath.Join(home, "tsnet")) {
		return check{"enrollment", levelOK, "enrolled (" + filepath.Join(home, "tsnet") + ")"}
	}
	return check{"enrollment", levelWarn, "not enrolled in a tailnet: run `protectorate node install` (or node join); a token-mode agent is not checked here"}
}

func nodeDoctorCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "doctor",
		Short: "Check this machine as a node: session, screen lock, startup, hub",
		Long: `Reports whether the agent can run desk commands here: whether this process is
in an interactive desktop session, whether the screen is locked, whether startup is
registered, and whether the hub answers. Exits nonzero when a check fails.
It changes nothing.`,
		Args: cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			in, err := currentInstallInput()
			if err != nil {
				return err
			}
			ctx := cmd.Context()
			var screenName string
			var screenErr error
			var ready error
			if in.GOOS == "windows" {
				ready = desk.New().Ready()
				screenName, screenErr = desk.InputDesktop()
			}
			explicit := cmd.Flags().Changed("hub") || os.Getenv("PROTECTORATE_HUB") != ""
			checks := []check{
				{"platform", levelInfo, runtime.GOOS + "/" + runtime.GOARCH + ", protectorate " + version},
				sessionCheck(ready, in.GOOS),
				screenCheck(in.GOOS, ready, screenName, screenErr),
				startupCheck(ctx, in),
				enrollCheck(in.Home),
				hubCheck(ctx, o.hubURL, explicit),
			}
			if n := writeDoctor(cmd.OutOrStdout(), in.GOOS, checks); n > 0 {
				return errors.New(strconv.Itoa(n) + " check(s) failed")
			}
			return nil
		},
	}
}
