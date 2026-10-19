package svcgen

import (
	"fmt"
	"strings"
)

const termuxPrefix = "/data/data/com.termux/files/usr"

func shQuote(w string) string {
	if w != "" && !strings.ContainsAny(w, " \t\n\"'\\$`;&|<>(){}*?[]#~!") {
		return w
	}
	return "'" + strings.ReplaceAll(w, "'", `'\''`) + "'"
}

// TermuxRun renders the termux-services (runit) run script.
func TermuxRun(s Spec) (string, error) {
	if err := s.validate(); err != nil {
		return "", err
	}
	words := make([]string, 0, 4+len(s.Args))
	for _, w := range s.command() {
		words = append(words, shQuote(w))
	}
	var b strings.Builder
	fmt.Fprintf(&b, "#!%s/bin/sh\nexec 2>&1\n", termuxPrefix)
	if s.Home != "" {
		fmt.Fprintf(&b, "export PROTECTORATE_HOME=%s\n", shQuote(s.Home))
	}
	fmt.Fprintf(&b, "exec %s\n", strings.Join(words, " "))
	return b.String(), nil
}

// TermuxBoot renders the Termux:Boot script that starts termux-services.
func TermuxBoot() string {
	return fmt.Sprintf("#!%s/bin/sh\ntermux-wake-lock\n. %s/etc/profile.d/start-services.sh\n", termuxPrefix, termuxPrefix)
}

// TermuxNotes explains how to enable a runit service.
func TermuxNotes(s Spec) string {
	return fmt.Sprintf("Setup (not run for you):\n  pkg install termux-services   # then restart Termux\n  sv-enable %s\nInstall the Termux:Boot app, open it once, and keep the boot script executable (chmod +x).\n", s.Name())
}
