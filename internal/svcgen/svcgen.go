// Package svcgen renders service definitions for the protectorate hub and
// node agent. It only produces text; installing is the caller's decision.
package svcgen

import (
	"fmt"
	"strings"
)

const (
	RoleHub  = "hub"
	RoleNode = "node"
)

// Spec describes one service.
type Spec struct {
	Role   string
	Binary string
	// Args follow "<role> serve".
	Args []string
	Home string
	// TmuxSocket is exported as PROTECTORATE_TMUX_SOCKET when set (hub only).
	TmuxSocket string
	// User makes a system-level unit run as this account; empty means a user unit.
	User string
	// Path overrides the default PATH of the unit.
	Path string
	// Elevated makes a logon task run with the highest available privileges.
	Elevated bool
	// WinUser (DOMAIN\user) restricts a logon task to one account.
	WinUser string
}

// Name is the service name for the role.
func (s Spec) Name() string { return "protectorate-" + s.Role }

func (s Spec) validate() error {
	if s.Role != RoleHub && s.Role != RoleNode {
		return fmt.Errorf("role must be %s or %s, got %q", RoleHub, RoleNode, s.Role)
	}
	if s.Binary == "" {
		return fmt.Errorf("binary path is empty")
	}
	return nil
}

func (s Spec) command() []string {
	return append([]string{s.Binary, s.Role, "serve"}, s.Args...)
}

// quoteSystemd quotes one ExecStart word. Percent signs are doubled because
// systemd expands specifiers in ExecStart.
func quoteSystemd(w string) string {
	w = strings.ReplaceAll(w, "%", "%%")
	w = strings.ReplaceAll(w, "$", "$$")
	if w != "" && !strings.ContainsAny(w, " \t\"'\\;") {
		return w
	}
	w = strings.ReplaceAll(w, `\`, `\\`)
	w = strings.ReplaceAll(w, `"`, `\"`)
	return `"` + w + `"`
}

func escSpec(v string) string { return strings.ReplaceAll(v, "%", "%%") }

func quoteSystemdEnv(k, v string) string {
	v = strings.ReplaceAll(v, `\`, `\\`)
	v = strings.ReplaceAll(v, `"`, `\"`)
	return fmt.Sprintf("Environment=\"%s=%s\"\n", k, v)
}

// Systemd renders a unit file. With Spec.User set it is a system unit,
// otherwise a user unit.
func Systemd(s Spec) (string, error) {
	if err := s.validate(); err != nil {
		return "", err
	}
	words := make([]string, 0, 4+len(s.Args))
	for _, w := range s.command() {
		words = append(words, quoteSystemd(w))
	}
	path := s.Path
	if path == "" {
		if s.User == "" {
			path = "%h/.local/bin:%h/go/bin:/usr/local/bin:/usr/bin:/bin"
		} else {
			path = "/usr/local/bin:/usr/bin:/bin"
		}
	}

	var b strings.Builder
	b.WriteString("[Unit]\n")
	fmt.Fprintf(&b, "Description=Protectorate %s\n", s.Role)
	b.WriteString("After=network-online.target\nWants=network-online.target\n\n")
	b.WriteString("[Service]\n")
	if s.User != "" {
		fmt.Fprintf(&b, "User=%s\n", s.User)
	}
	if s.Home != "" {
		b.WriteString(quoteSystemdEnv("PROTECTORATE_HOME", escSpec(s.Home)))
	}
	if s.TmuxSocket != "" && s.Role == RoleHub {
		b.WriteString(quoteSystemdEnv("PROTECTORATE_TMUX_SOCKET", escSpec(s.TmuxSocket)))
	}
	b.WriteString(quoteSystemdEnv("PATH", path))
	fmt.Fprintf(&b, "ExecStart=%s\n", strings.Join(words, " "))
	b.WriteString("Restart=on-failure\nRestartSec=5\n")
	if s.Role == RoleHub {
		b.WriteString("# A SIGKILLed hub must not orphan the headscale child it supervises.\n")
		b.WriteString("KillMode=control-group\n")
		b.WriteString("# hub drain plus headscale SIGTERM grace (10s) before SIGKILL.\n")
		b.WriteString("TimeoutStopSec=45\n")
	} else {
		b.WriteString("KillMode=control-group\nTimeoutStopSec=20\n")
	}
	b.WriteString("\n[Install]\n")
	if s.User == "" {
		b.WriteString("WantedBy=default.target\n")
	} else {
		b.WriteString("WantedBy=multi-user.target\n")
	}
	return b.String(), nil
}

// SystemdNotes is the human instruction block for a unit; nothing here is
// ever executed by protectorate.
func SystemdNotes(s Spec, unitPath string) string {
	var b strings.Builder
	if s.User == "" {
		fmt.Fprintf(&b, "Install (not run for you):\n  mkdir -p ~/.config/systemd/user && cp %s ~/.config/systemd/user/\n", unitPath)
		fmt.Fprintf(&b, "  systemctl --user daemon-reload\n  systemctl --user enable --now %s.service\n", s.Name())
		b.WriteString("\nTo keep it running while you are logged out, run once as root:\n  sudo loginctl enable-linger $USER\n")
		if s.Role == RoleHub {
			b.WriteString("\nSleeves need the user manager (systemd-run --user), so prefer the user unit on a hub that runs sleeves.\n")
		}
	} else {
		fmt.Fprintf(&b, "Install (not run for you):\n  sudo cp %s /etc/systemd/system/\n", unitPath)
		fmt.Fprintf(&b, "  sudo systemctl daemon-reload\n  sudo systemctl enable --now %s.service\n", s.Name())
		if s.Role == RoleHub {
			b.WriteString("\nA system unit has no user bus: sleeves (systemd-run --user) need `loginctl enable-linger " + s.User + "`.\n")
		}
	}
	return b.String()
}
