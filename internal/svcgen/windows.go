package svcgen

import (
	"fmt"
	"html"
	"strings"
	"unicode/utf16"
)

const (
	TriggerLogon = "logon"
	TriggerBoot  = "boot"
)

func winQuote(w string) string {
	if w != "" && !strings.ContainsAny(w, " \t\"") {
		return w
	}
	return `"` + strings.ReplaceAll(w, `"`, `\"`) + `"`
}

func winJoin(words []string) string {
	q := make([]string, len(words))
	for i, w := range words {
		q[i] = winQuote(w)
	}
	return strings.Join(q, " ")
}

// TaskXML renders a Task Scheduler definition for `schtasks /Create /XML`.
// The file must be UTF-16 encoded; see EncodeUTF16LE.
func TaskXML(s Spec, trigger string) (string, error) {
	if err := s.validate(); err != nil {
		return "", err
	}
	var trig, principal string
	switch trigger {
	case TriggerLogon:
		trig = "<LogonTrigger><Enabled>true</Enabled></LogonTrigger>"
		principal = "<LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel>"
	case TriggerBoot:
		trig = "<BootTrigger><Enabled>true</Enabled></BootTrigger>"
		principal = "<UserId>S-1-5-18</UserId><RunLevel>HighestAvailable</RunLevel>"
	default:
		return "", fmt.Errorf("trigger must be %s or %s, got %q", TriggerLogon, TriggerBoot, trigger)
	}

	command := s.Binary
	args := strings.TrimSpace(winJoin(append([]string{s.Role, "serve"}, s.Args...)))
	if s.Home != "" {
		// cmd /c strips one outer quote pair, so the inner quotes stay plain.
		inner := fmt.Sprintf("set PROTECTORATE_HOME=%s&& \"%s\" %s", s.Home, s.Binary, args)
		command = "cmd.exe"
		args = "/c \"" + inner + "\""
	}

	var b strings.Builder
	b.WriteString("<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n")
	b.WriteString("<Task version=\"1.2\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\n")
	fmt.Fprintf(&b, "  <RegistrationInfo><Description>Protectorate %s</Description></RegistrationInfo>\n", s.Role)
	fmt.Fprintf(&b, "  <Triggers>%s</Triggers>\n", trig)
	fmt.Fprintf(&b, "  <Principals><Principal id=\"Author\">%s</Principal></Principals>\n", principal)
	b.WriteString("  <Settings>\n")
	b.WriteString("    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>\n")
	b.WriteString("    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>\n")
	b.WriteString("    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>\n")
	b.WriteString("    <StartWhenAvailable>true</StartWhenAvailable>\n")
	b.WriteString("    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>\n")
	b.WriteString("    <Hidden>true</Hidden>\n")
	b.WriteString("    <RestartOnFailure><Interval>PT1M</Interval><Count>999</Count></RestartOnFailure>\n")
	b.WriteString("  </Settings>\n")
	b.WriteString("  <Actions Context=\"Author\"><Exec>\n")
	fmt.Fprintf(&b, "    <Command>%s</Command>\n", html.EscapeString(command))
	fmt.Fprintf(&b, "    <Arguments>%s</Arguments>\n", html.EscapeString(args))
	b.WriteString("  </Exec></Actions>\n")
	b.WriteString("</Task>\n")
	return b.String(), nil
}

// TaskNotes explains how to register the task XML.
func TaskNotes(s Spec, trigger, xmlPath string) string {
	var b strings.Builder
	b.WriteString("Register (not run for you; boot triggers need an elevated prompt):\n")
	fmt.Fprintf(&b, "  schtasks /Create /TN %s /XML %s\n", winQuote(s.Name()), winQuote(xmlPath))
	fmt.Fprintf(&b, "  schtasks /Run /TN %s\n", winQuote(s.Name()))
	if trigger == TriggerLogon {
		b.WriteString("A logon task runs in the interactive session and may show a console window; use the Windows service for a headless node.\n")
	}
	return b.String()
}

// WindowsService returns the sc.exe commands that register `node serve` as a
// real service (the binary implements the service API on Windows).
func WindowsService(s Spec, home string) (string, error) {
	if err := s.validate(); err != nil {
		return "", err
	}
	if s.Role != RoleNode {
		return "", fmt.Errorf("only the node agent runs as a Windows service")
	}
	name := s.Name()
	bin := winJoin(s.command())
	var b strings.Builder
	b.WriteString("REM run from an elevated prompt\n")
	fmt.Fprintf(&b, "sc.exe create %s binPath= %s start= auto DisplayName= \"Protectorate node\"\n", name, winQuote(bin))
	fmt.Fprintf(&b, "sc.exe description %s \"Protectorate node agent\"\n", name)
	fmt.Fprintf(&b, "sc.exe failure %s reset= 86400 actions= restart/5000/restart/30000/restart/60000\n", name)
	if home != "" {
		fmt.Fprintf(&b, "reg.exe add HKLM\\SYSTEM\\CurrentControlSet\\Services\\%s /v Environment /t REG_MULTI_SZ /d \"PROTECTORATE_HOME=%s\"\n", name, home)
	}
	fmt.Fprintf(&b, "sc.exe start %s\n", name)
	return b.String(), nil
}

// EncodeUTF16LE encodes s with a byte order mark, as schtasks requires.
func EncodeUTF16LE(s string) []byte {
	u := utf16.Encode([]rune(s))
	out := make([]byte, 0, 2+2*len(u))
	out = append(out, 0xff, 0xfe)
	for _, c := range u {
		out = append(out, byte(c), byte(c>>8))
	}
	return out
}
