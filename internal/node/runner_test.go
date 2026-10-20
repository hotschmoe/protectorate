package node

import (
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
)

func TestWindowsCommandLine(t *testing.T) {
	tests := []struct {
		args []string
		want string
	}{
		{[]string{`C:\Go\bin\go.exe`, "build", "./..."}, `C:\Go\bin\go.exe build ./...`},
		{[]string{`C:\Program Files\Go\bin\go.exe`, "test"}, `"C:\Program Files\Go\bin\go.exe" test`},
		{[]string{"a", ""}, `a ""`},
		{[]string{"a", `say "hi"`}, `a "say \"hi\""`},
		{[]string{"a", `dir\`}, `a dir\`},
		{[]string{"a", `with space\`}, `a "with space\\"`},
		{[]string{"a", `x\"y`}, `a "x\\\"y"`},
		{[]string{"a", "tab\there"}, "a \"tab\there\""},
	}
	for _, tt := range tests {
		if got := windowsCommandLine(tt.args); got != tt.want {
			t.Errorf("windowsCommandLine(%q) = %s, want %s", tt.args, got, tt.want)
		}
	}
}

func TestMergeEnvAndBlock(t *testing.T) {
	base := []string{"Path=C:\\Windows", "USERPROFILE=C:\\Users\\protectorate-runner", "TEMP=C:\\t", "=C:=C:\\"}
	got := mergeEnv(base, [][2]string{{"PROTECTORATE_JOB_ID", "j1"}, {"PATH", "C:\\Go\\bin;C:\\Windows"}, {"temp", "D:\\t"}})
	want := []string{"PATH=C:\\Go\\bin;C:\\Windows", "PROTECTORATE_JOB_ID=j1", "temp=D:\\t", "USERPROFILE=C:\\Users\\protectorate-runner"}
	if !slices.Equal(got, want) {
		t.Fatalf("mergeEnv = %q, want %q", got, want)
	}
	block := envBlock(got)
	if n := len(block); block[n-1] != 0 || block[n-2] != 0 {
		t.Fatalf("block does not end in two NULs: %v", block[n-4:])
	}
	if back := splitEnvBlock(block); !slices.Equal(back, got) {
		t.Fatalf("round trip %q", back)
	}
	if b := envBlock(nil); len(b) != 2 || b[0] != 0 || b[1] != 0 {
		t.Fatalf("empty block %v", b)
	}
}

func TestCPURate(t *testing.T) {
	tests := []struct {
		pct   uint32
		cores int
		want  uint32
	}{
		{0, 8, 0},
		{100, 8, 1250},
		{200, 4, 5000},
		{800, 4, 10000},
		{1, 64, 1},
		{50, 0, 0},
	}
	for _, tt := range tests {
		if got := cpuRate(tt.pct, tt.cores); got != tt.want {
			t.Errorf("cpuRate(%d, %d) = %d, want %d", tt.pct, tt.cores, got, tt.want)
		}
	}
}

func TestRunnerPassword(t *testing.T) {
	random := make([]byte, 28)
	for i := range random {
		random[i] = byte(i * 37)
	}
	pw, err := runnerPassword(random)
	if err != nil || len(pw) != 32 {
		t.Fatalf("password %q %v", pw, err)
	}
	for _, class := range []string{"ABCDEFGHJKLMNPQRSTUVWXYZ", "abcdefghijkmnopqrstuvwxyz", "0123456789", "#"} {
		if !strings.ContainsAny(pw, class) {
			t.Errorf("password %q lacks one of %q", pw, class)
		}
	}
	if _, err := runnerPassword(random[:10]); err == nil {
		t.Error("short randomness accepted")
	}
}

func TestRunnerPlanText(t *testing.T) {
	dir := runnerJobDir(`C:\ProgramData`)
	if want := filepath.Join(`C:\ProgramData`, "protectorate", "runner"); dir != want {
		t.Fatalf("job dir %s", dir)
	}
	sddl := runnerDirSDDL("S-1-5-21-1-2-3-1001", "S-1-5-21-1-2-3-1002")
	if sddl != "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;S-1-5-21-1-2-3-1001)(A;OICI;0x1301bf;;;S-1-5-21-1-2-3-1002)" {
		t.Fatalf("sddl %s", sddl)
	}
	steps := strings.Join(RunnerSteps(`C:\Users\me\.protectorate`, dir), "\n")
	for _, want := range []string{"elevated", RunnerUser, "Users only", "SpecialAccounts", dir, "DPAPI", "runner.json", "restart the agent"} {
		if !strings.Contains(steps, want) {
			t.Errorf("steps do not say %q:\n%s", want, steps)
		}
	}
}

func TestReadRunnerConfig(t *testing.T) {
	tests := []struct {
		name    string
		content string
		wantDir string
	}{
		{"complete", `{"user":"protectorate-runner","job_dir":"C:\\ProgramData\\protectorate\\runner","password_dpapi":"AQID"}`, `C:\ProgramData\protectorate\runner`},
		{"no password", `{"user":"protectorate-runner","job_dir":"C:\\x"}`, ""},
		{"corrupt", `{`, ""},
		{"missing", "", ""},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			home := t.TempDir()
			if tt.content != "" {
				if err := os.WriteFile(filepath.Join(home, runnerFile), []byte(tt.content), 0o600); err != nil {
					t.Fatal(err)
				}
			}
			dir, ok := RunnerJobDir(home)
			if dir != tt.wantDir || ok != (tt.wantDir != "") {
				t.Fatalf("RunnerJobDir = %q, %v", dir, ok)
			}
		})
	}
}
