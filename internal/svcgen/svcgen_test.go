package svcgen

import (
	"bytes"
	"flag"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

var update = flag.Bool("update", false, "rewrite golden files")

func golden(t *testing.T, name, got string) {
	t.Helper()
	p := filepath.Join("testdata", name)
	if *update {
		if err := os.MkdirAll("testdata", 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte(got), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	want, err := os.ReadFile(p)
	if err != nil {
		t.Fatal(err)
	}
	if string(want) != got {
		t.Errorf("%s differs from golden:\n%s", name, got)
	}
}

var (
	hubSpec = Spec{Role: RoleHub, Binary: "/usr/local/bin/protectorate", Home: "/home/ops/.protectorate",
		TmuxSocket: "protectorate", Args: []string{"--listen", "0.0.0.0:7470", "--web", "/srv/web root"}}
	nodeSpec = Spec{Role: RoleNode, Binary: "/usr/local/bin/protectorate", Home: "/home/ops/.protectorate",
		Args: []string{"--watch", "/var/cache/build"}}
	winSpec = Spec{Role: RoleNode, Binary: `C:\Program Files\protectorate\protectorate.exe`,
		Home: `C:\ProgramData\protectorate`, Args: []string{"--watch", `D:\build cache`}}
	termuxSpec = Spec{Role: RoleNode, Binary: "/data/data/com.termux/files/usr/bin/protectorate",
		Home: "/data/data/com.termux/files/home/.protectorate"}
)

func TestGolden(t *testing.T) {
	sysHub := hubSpec
	sysHub.User = "ops"
	elevated := winSpec
	elevated.Elevated, elevated.WinUser = true, `OFFICE\ops`
	cases := []struct {
		name string
		gen  func() (string, error)
	}{
		{"node-logon-elevated.xml", func() (string, error) { return TaskXML(elevated, TriggerLogon) }},
		{"hub-user.service", func() (string, error) { return Systemd(hubSpec) }},
		{"hub-system.service", func() (string, error) { return Systemd(sysHub) }},
		{"node-user.service", func() (string, error) { return Systemd(nodeSpec) }},
		{"node-logon.xml", func() (string, error) { return TaskXML(nodeSpec, TriggerLogon) }},
		{"node-boot-home.xml", func() (string, error) { return TaskXML(winSpec, TriggerBoot) }},
		{"node-service.cmd", func() (string, error) { return WindowsService(winSpec, winSpec.Home) }},
		{"termux-run", func() (string, error) { return TermuxRun(termuxSpec) }},
		{"termux-boot", func() (string, error) { return TermuxBoot(), nil }},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			got, err := c.gen()
			if err != nil {
				t.Fatal(err)
			}
			golden(t, c.name, got)
		})
	}
}

func TestValidation(t *testing.T) {
	if _, err := Systemd(Spec{Role: "x", Binary: "b"}); err == nil {
		t.Error("bad role accepted")
	}
	if _, err := Systemd(Spec{Role: RoleHub}); err == nil {
		t.Error("empty binary accepted")
	}
	if _, err := TaskXML(nodeSpec, "weekly"); err == nil {
		t.Error("bad trigger accepted")
	}
	if _, err := TaskXML(hubSpec, TriggerLogon); err == nil {
		t.Error("hub accepted as scheduled task")
	}
	if _, err := WindowsService(hubSpec, ""); err == nil {
		t.Error("hub accepted as windows service")
	}
}

func TestUTF16(t *testing.T) {
	got := EncodeUTF16LE("a\u00e9")
	want := []byte{0xff, 0xfe, 'a', 0, 0xe9, 0}
	if !bytes.Equal(got, want) {
		t.Errorf("got %x, want %x", got, want)
	}
}

func TestSystemdVerify(t *testing.T) {
	if _, err := exec.LookPath("systemd-analyze"); err != nil {
		t.Skip("systemd-analyze not available")
	}
	sysHub := hubSpec
	sysHub.User = "root"
	for name, s := range map[string]Spec{"hub": hubSpec, "node": nodeSpec} {
		unit, err := Systemd(s)
		if err != nil {
			t.Fatal(err)
		}
		dir := t.TempDir()
		p := filepath.Join(dir, s.Name()+".service")
		if err := os.WriteFile(p, []byte(unit), 0o644); err != nil {
			t.Fatal(err)
		}
		bin := filepath.Join(dir, "protectorate")
		unit = strings.ReplaceAll(unit, s.Binary, bin)
		if err := os.WriteFile(bin, []byte("#!/bin/sh\n"), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte(unit), 0o644); err != nil {
			t.Fatal(err)
		}
		out, err := exec.Command("systemd-analyze", "verify", "--user", p).CombinedOutput()
		if err != nil || len(bytes.TrimSpace(out)) > 0 {
			t.Errorf("%s: systemd-analyze: %v\n%s", name, err, out)
		}
	}
}
