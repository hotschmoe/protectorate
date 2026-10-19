package main

import (
	"bytes"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func argvs(steps []installStep) [][]string {
	var out [][]string
	for _, s := range steps {
		if s.Kind == stepRun {
			out = append(out, s.Argv)
		}
	}
	return out
}

func find(steps []installStep, kind stepKind, dst string) *installStep {
	for i := range steps {
		if steps[i].Kind == kind && steps[i].Dst == dst {
			return &steps[i]
		}
	}
	return nil
}

func TestPlanInstall(t *testing.T) {
	tests := []struct {
		name     string
		in       installInput
		wantBin  string
		wantCopy bool
		wantRun  [][]string
		files    map[string]string
		wantErr  string
	}{
		{
			name: "windows",
			in: installInput{GOOS: "windows", Exe: `C:\Users\ops\Downloads\protectorate.exe`, UserHome: `C:\Users\ops`, LocalAppData: `C:\Users\ops\AppData\Local`,
				Home: `C:\Users\ops\.protectorate`, WinUser: `PC\ops`, Control: "http://hub:7480", Name: "pc"},
			wantBin:  `C:\Users\ops\AppData\Local\Programs\protectorate\protectorate.exe`,
			wantCopy: true,
			wantRun: [][]string{
				{"schtasks", "/End", "/TN", "protectorate-node"},
				{"schtasks", "/Create", "/TN", "protectorate-node", "/XML", `C:\Users\ops\.protectorate\protectorate-node.xml`, "/F"},
				{"schtasks", "/Run", "/TN", "protectorate-node"},
			},
			files: map[string]string{
				`C:\Users\ops\.protectorate\protectorate-node.xml`: "<RunLevel>LeastPrivilege</RunLevel>",
			},
		},
		{
			name: "windows elevated, binary already in place",
			in: installInput{GOOS: "windows", Exe: `C:\L\Programs\protectorate\protectorate.exe`, LocalAppData: `C:\L`,
				Home: `C:\h`, Control: "http://hub:7480", Name: "pc", Elevated: true},
			wantBin: `C:\L\Programs\protectorate\protectorate.exe`,
			wantRun: [][]string{
				{"schtasks", "/End", "/TN", "protectorate-node"},
				{"schtasks", "/Create", "/TN", "protectorate-node", "/XML", `C:\h\protectorate-node.xml`, "/F"},
				{"schtasks", "/Run", "/TN", "protectorate-node"},
			},
			files: map[string]string{`C:\h\protectorate-node.xml`: "<RunLevel>HighestAvailable</RunLevel>"},
		},
		{
			name:    "windows without LOCALAPPDATA",
			in:      installInput{GOOS: "windows", Exe: `C:\x.exe`, Home: `C:\h`, Control: "http://hub"},
			wantErr: "LOCALAPPDATA",
		},
		{
			name:     "linux",
			in:       installInput{GOOS: "linux", Exe: "/tmp/dl/protectorate", UserHome: "/home/ops", Home: "/home/ops/.protectorate", Control: "http://hub:7480", Name: "box"},
			wantBin:  "/home/ops/.local/bin/protectorate",
			wantCopy: true,
			wantRun: [][]string{
				{"systemctl", "--user", "daemon-reload"},
				{"systemctl", "--user", "enable", "protectorate-node.service"},
				{"systemctl", "--user", "restart", "protectorate-node.service"},
			},
			files: map[string]string{"/home/ops/.config/systemd/user/protectorate-node.service": "ExecStart=/home/ops/.local/bin/protectorate node serve"},
		},
		{
			name:    "linux already installed",
			in:      installInput{GOOS: "linux", Exe: "/home/ops/.local/bin/protectorate", UserHome: "/home/ops", Home: "/h", Control: "http://hub"},
			wantBin: "/home/ops/.local/bin/protectorate",
			wantRun: [][]string{
				{"systemctl", "--user", "daemon-reload"},
				{"systemctl", "--user", "enable", "protectorate-node.service"},
				{"systemctl", "--user", "restart", "protectorate-node.service"},
			},
			files: map[string]string{"/home/ops/.config/systemd/user/protectorate-node.service": "Environment=\"PROTECTORATE_HOME=/h\""},
		},
		{
			name: "termux",
			in: installInput{GOOS: "android", Exe: "/sdcard/p", UserHome: "/data/data/com.termux/files/home", Prefix: "/data/data/com.termux/files/usr",
				Home: "/data/data/com.termux/files/home/.protectorate", Control: "http://hub", Name: "phone"},
			wantBin:  "/data/data/com.termux/files/usr/bin/protectorate",
			wantCopy: true,
			wantRun:  [][]string{{"sv-enable", "protectorate-node"}},
			files: map[string]string{
				"/data/data/com.termux/files/usr/var/service/protectorate-node/run": "exec /data/data/com.termux/files/usr/bin/protectorate node serve",
				"/data/data/com.termux/files/home/.termux/boot/protectorate-node":   "termux-wake-lock",
			},
		},
		{name: "macos", in: installInput{GOOS: "darwin", Exe: "/p", Control: "http://hub"}, wantErr: "darwin"},
		{name: "no control", in: installInput{GOOS: "linux", Exe: "/p", UserHome: "/h"}, wantErr: "control URL"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			steps, err := planInstall(tt.in)
			if tt.wantErr != "" {
				if err == nil || !strings.Contains(err.Error(), tt.wantErr) {
					t.Fatalf("err = %v, want %q", err, tt.wantErr)
				}
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			if got := find(steps, stepCopy, tt.wantBin) != nil; got != tt.wantCopy {
				t.Errorf("copy to %s = %v, want %v", tt.wantBin, got, tt.wantCopy)
			}
			if got := argvs(steps); !reflect.DeepEqual(got, tt.wantRun) {
				t.Errorf("commands = %v, want %v", got, tt.wantRun)
			}
			for p, want := range tt.files {
				s := find(steps, stepWrite, p)
				if s == nil {
					t.Errorf("no write to %s", p)
				} else if !strings.Contains(string(s.Content), want) {
					t.Errorf("%s lacks %q:\n%s", p, want, s.Content)
				}
			}
			var enroll int
			for _, s := range steps {
				if s.Kind == stepEnroll {
					enroll++
				}
				for _, a := range s.Argv {
					if strings.Contains(a, "KEY") {
						t.Errorf("key leaked into %v", s.Argv)
					}
				}
			}
			if enroll != 1 {
				t.Errorf("enroll steps = %d", enroll)
			}
		})
	}
}

func TestWindowsTaskHasNoShellWrapper(t *testing.T) {
	steps, err := planInstall(installInput{GOOS: "windows", Exe: `C:\a.exe`, LocalAppData: `C:\L`, Home: `C:\h`, Control: "http://hub", ServeArgs: []string{"--watch", `D:\b`}})
	if err != nil {
		t.Fatal(err)
	}
	x := string(find(steps, stepWrite, `C:\h\protectorate-node.xml`).Content)
	for _, want := range []string{`<Command>%SystemRoot%\System32\conhost.exe</Command>`, `<Arguments>--headless C:\L\Programs\protectorate\protectorate.exe node serve --home C:\h --watch D:\b</Arguments>`} {
		if !strings.Contains(x, want) {
			t.Errorf("xml lacks %q:\n%s", want, x)
		}
	}
	if strings.Contains(x, "cmd.exe") || strings.Contains(x, "hide-console") {
		t.Error("cmd.exe wrapper or hide-console present")
	}
}

func TestPlanUninstallDeletesNothing(t *testing.T) {
	tests := []struct {
		name    string
		in      installInput
		wantRun [][]string
		mention string
	}{
		{"windows", installInput{GOOS: "windows", Exe: `C:\p.exe`, LocalAppData: `C:\L`, Home: `C:\h`, Name: "pc"},
			[][]string{{"schtasks", "/End", "/TN", "protectorate-node"}, {"schtasks", "/Delete", "/TN", "protectorate-node", "/F"}}, `C:\L\Programs\protectorate\protectorate.exe`},
		{"linux", installInput{GOOS: "linux", UserHome: "/home/o", Home: "/home/o/.protectorate", Name: "box"},
			[][]string{{"systemctl", "--user", "disable", "--now", "protectorate-node.service"}}, "/home/o/.config/systemd/user/protectorate-node.service"},
		{"termux", installInput{GOOS: "android", UserHome: "/h", Prefix: "/p", Home: "/h/.protectorate", Name: "ph"},
			[][]string{{"sv-disable", "protectorate-node"}}, "/p/var/service/protectorate-node"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			steps, err := planUninstall(tt.in)
			if err != nil {
				t.Fatal(err)
			}
			if got := argvs(steps); !reflect.DeepEqual(got, tt.wantRun) {
				t.Errorf("commands = %v, want %v", got, tt.wantRun)
			}
			var notes string
			for _, s := range steps {
				if s.Kind == stepCopy || s.Kind == stepWrite {
					t.Errorf("uninstall writes: %+v", s)
				}
				for _, a := range s.Argv {
					if a == "rm" || a == "del" || strings.HasPrefix(a, "Remove") {
						t.Errorf("uninstall deletes: %v", s.Argv)
					}
				}
				notes += s.Desc + "\n"
			}
			if !strings.Contains(notes, tt.mention) || !strings.Contains(notes, "node revoke "+tt.in.Name) {
				t.Errorf("notes lack %q or the revoke hint:\n%s", tt.mention, notes)
			}
		})
	}
}

func TestResolveKey(t *testing.T) {
	dir := t.TempDir()
	file := filepath.Join(dir, "k")
	empty := filepath.Join(dir, "e")
	if err := os.WriteFile(file, []byte(" filekey\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(empty, nil, 0o600); err != nil {
		t.Fatal(err)
	}
	tests := []struct {
		name            string
		flag, file, env string
		want, wantErr   string
	}{
		{"flag", "k1", "", "envkey", "k1", ""},
		{"file", "", file, "envkey", "filekey", ""},
		{"env", "", "", " envkey\n", "envkey", ""},
		{"none", "", "", "", "", ""},
		{"both", "k1", file, "", "", "exclusive"},
		{"missing file", "", filepath.Join(dir, "nope"), "", "", "read key file"},
		{"empty file", "", empty, "", "", "empty"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := resolveKey(tt.flag, tt.file, tt.env)
			if tt.wantErr != "" {
				if err == nil || !strings.Contains(err.Error(), tt.wantErr) {
					t.Fatalf("err = %v, want %q", err, tt.wantErr)
				}
				return
			}
			if err != nil || got != tt.want {
				t.Fatalf("got %q, %v; want %q", got, err, tt.want)
			}
		})
	}
}

func TestInstallDryRunChangesNothing(t *testing.T) {
	home := t.TempDir()
	root := newRoot()
	var out bytes.Buffer
	root.SetOut(&out)
	root.SetErr(&bytes.Buffer{})
	root.SetArgs([]string{"node", "install", "--control", "http://hub:7480", "--key", "SECRETKEY", "--home", home, "--name", "n1", "--dry-run"})
	if err := root.Execute(); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(out.String(), "dry run") || strings.Contains(out.String(), "SECRETKEY") {
		t.Errorf("unexpected output:\n%s", out.String())
	}
	if entries, _ := os.ReadDir(home); len(entries) != 0 {
		t.Errorf("dry run wrote %v", entries)
	}
}

func TestDoctor(t *testing.T) {
	var b bytes.Buffer
	n := writeDoctor(&b, "windows", []check{
		sessionCheck(nil, "windows"),
		screenCheck("windows", nil, "Winlogon", nil),
		windowsStartupCheck(true, []int{12}),
	})
	if n != 1 {
		t.Errorf("failed = %d, want 1\n%s", n, b.String())
	}
	for _, want := range []string{"locked", "agent running (pid 12)", "stay logged in and unlocked"} {
		if !strings.Contains(b.String(), want) {
			t.Errorf("output lacks %q:\n%s", want, b.String())
		}
	}
	if c := screenCheck("windows", errors.New("session 0"), "", errors.New("x")); c.Level != levelInfo {
		t.Errorf("screen in a non-interactive session = %v", c)
	}
	b.Reset()
	if n := writeDoctor(&b, "linux", []check{sessionCheck(nil, "linux"), screenCheck("linux", nil, "", nil)}); n != 0 || strings.Contains(b.String(), "Unattended") {
		t.Errorf("linux output:\n%s", b.String())
	}
}

func TestDoctorParsers(t *testing.T) {
	csv := "\"protectorate.exe\",\"100\",\"Console\",\"1\",\"20,000 K\"\r\n\"protectorate.exe\",\"200\",\"Console\",\"1\",\"9,000 K\"\r\n"
	if got := otherPIDs(csv, 100); !reflect.DeepEqual(got, []int{200}) {
		t.Errorf("otherPIDs = %v", got)
	}
	if got := otherPIDs("INFO: No tasks are running which match the specified criteria.\r\n", 1); got != nil {
		t.Errorf("otherPIDs on no match = %v", got)
	}
	tests := []struct {
		name                    string
		enabled, active, linger string
		want                    level
	}{
		{"all good", "enabled\n", "active\n", "Linger=yes\n", levelOK},
		{"no linger", "enabled", "active", "Linger=no", levelWarn},
		{"inactive", "enabled", "inactive", "Linger=yes", levelWarn},
		{"not installed", "", "", "", levelFail},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := systemdStartupCheck(tt.enabled, tt.active, tt.linger).Level; got != tt.want {
				t.Errorf("level = %v, want %v", got, tt.want)
			}
		})
	}
	if windowsStartupCheck(false, nil).Level != levelFail || windowsStartupCheck(true, nil).Level != levelWarn {
		t.Error("windows startup levels wrong")
	}
}

func TestUpdateHelpers(t *testing.T) {
	cells := []struct{ node, hub, want string }{
		{"v2", "v2", "v2"},
		{"v1", "v2", "v1 (!)"},
		{"", "v2", "-"},
		{"v1", "", "v1"},
	}
	for _, tt := range cells {
		if got := versionCell(tt.node, tt.hub); got != tt.want {
			t.Errorf("versionCell(%q,%q) = %q, want %q", tt.node, tt.hub, got, tt.want)
		}
	}
}
