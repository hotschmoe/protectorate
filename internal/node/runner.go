package node

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"unicode/utf16"
)

// The Windows job runner: a dedicated low-privilege local account that runs
// sandboxed jobs inside a job object, without access to the agent user's
// profile. `node install --runner` creates it once, elevated; this file holds
// the parts that are plain data so they are tested on every platform.

// RunnerUser is the local account sandboxed Windows jobs run as.
const RunnerUser = "protectorate-runner"

// runnerFile is the runner record in the agent's state directory.
const runnerFile = "runner.json"

// runnerEntropy is the DPAPI entropy the runner password is sealed with.
const runnerEntropy = "protectorate-runner"

// runnerConfig is runner.json: the account, the job root the account and
// the agent share, and the account's password sealed with DPAPI for the
// agent's own user (CryptProtectData, user scope).
type runnerConfig struct {
	User     string `json:"user"`
	JobDir   string `json:"job_dir"`
	Password []byte `json:"password_dpapi"`
}

func readRunnerConfig(home string) (*runnerConfig, error) {
	raw, err := os.ReadFile(filepath.Join(home, runnerFile))
	if err != nil {
		return nil, err
	}
	var c runnerConfig
	if err := json.Unmarshal(raw, &c); err != nil {
		return nil, fmt.Errorf("parse %s: %w", runnerFile, err)
	}
	if c.User == "" || c.JobDir == "" || len(c.Password) == 0 {
		return nil, fmt.Errorf("%s is incomplete: run node install --runner again", runnerFile)
	}
	return &c, nil
}

// RunnerJobDir is the job directory `node install --runner` set up, when
// this agent has a runner: the agent and the runner account both reach it,
// and the agent's profile stays closed to the runner.
func RunnerJobDir(home string) (string, bool) {
	c, err := readRunnerConfig(home)
	if err != nil {
		return "", false
	}
	return c.JobDir, true
}

// runnerJobDir is where the install puts the shared job root.
func runnerJobDir(programData string) string {
	return filepath.Join(programData, "protectorate", "runner")
}

// RunnerDir is the shared job root on this machine.
func RunnerDir() string { return runnerJobDir(os.Getenv("ProgramData")) }

// RunnerSteps says exactly what `node install --runner` does, in order.
func RunnerSteps(home, jobDir string) []string {
	return []string{
		"check that this prompt is elevated (Run as administrator) and runs as the user the agent runs as",
		"generate a random 32-character password",
		"create the local account " + RunnerUser + " (a standard user: member of Users only, password never expires, cannot change it), or reset its password if it exists",
		`hide it from the sign-in screen: HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon\SpecialAccounts\UserList ` + RunnerUser + " = 0",
		"create " + jobDir + " with a protected ACL: SYSTEM, Administrators and you full control, " + RunnerUser + " modify, inherited; jobs and warm worktrees live there",
		"sign in as " + RunnerUser + " once (LogonUser) to check the password",
		"seal the password with DPAPI for your user and write it to " + filepath.Join(home, runnerFile),
		"nothing else: your profile, files and sessions are not shared with the account; restart the agent to use it",
	}
}

// runnerDirSDDL is the protected DACL of the shared job root: SYSTEM,
// Administrators and the agent's user full control, the runner modify
// (read, write, execute, delete; no ACL changes), inherited by files and
// directories.
func runnerDirSDDL(agentSID, runnerSID string) string {
	return "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;" + agentSID + ")(A;OICI;0x1301bf;;;" + runnerSID + ")"
}

// runnerPassword turns random bytes into a password every Windows
// complexity policy accepts: 32 characters with upper, lower, digit and
// symbol.
func runnerPassword(random []byte) (string, error) {
	const alphabet = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789"
	if len(random) < 28 {
		return "", errors.New("runner password needs 28 random bytes")
	}
	var b strings.Builder
	for _, x := range random[:28] {
		b.WriteByte(alphabet[int(x)%len(alphabet)])
	}
	b.WriteString("Aa9#")
	return b.String(), nil
}

// cpuRate is a job object hard cap for cpuPercent of one core on a machine
// with cores logical cores, in hundredths of a percent of the whole machine;
// 0 means no cap.
func cpuRate(cpuPercent uint32, cores int) uint32 {
	if cpuPercent == 0 || cores < 1 {
		return 0
	}
	rate := uint64(cpuPercent) * 100 / uint64(cores)
	return uint32(min(max(rate, 1), 10000))
}

// mergeEnv overlays job variables on base ("K=V" entries), matching names
// without regard to case as Windows does, and sorts the result the way an
// environment block must be.
func mergeEnv(base []string, job [][2]string) []string {
	idx := map[string]int{}
	var out []string
	set := func(k, kv string) {
		u := strings.ToUpper(k)
		if i, ok := idx[u]; ok {
			out[i] = kv
			return
		}
		idx[u] = len(out)
		out = append(out, kv)
	}
	for _, kv := range base {
		if k, _, ok := strings.Cut(kv, "="); ok && k != "" {
			set(k, kv)
		}
	}
	for _, kv := range job {
		set(kv[0], kv[0]+"="+kv[1])
	}
	sort.SliceStable(out, func(i, j int) bool { return strings.ToUpper(out[i]) < strings.ToUpper(out[j]) })
	return out
}

// envBlock encodes env as a CREATE_UNICODE_ENVIRONMENT block: UTF-16
// "K=V" strings, each NUL-terminated, then one more NUL.
func envBlock(env []string) []uint16 {
	var out []uint16
	for _, kv := range env {
		out = append(out, utf16.Encode([]rune(kv))...)
		out = append(out, 0)
	}
	if len(out) == 0 {
		out = append(out, 0)
	}
	return append(out, 0)
}

// splitEnvBlock reads a NUL-separated, double-NUL-terminated UTF-16
// environment block.
func splitEnvBlock(block []uint16) []string {
	var out []string
	start := 0
	for i, c := range block {
		if c != 0 {
			continue
		}
		if i == start {
			break
		}
		out = append(out, string(utf16.Decode(block[start:i])))
		start = i + 1
	}
	return out
}

// windowsCommandLine joins args for CreateProcess the way the Microsoft C
// runtime splits them again: arguments with spaces, tabs, quotes or nothing
// are quoted, and backslashes are doubled only where they precede a quote.
func windowsCommandLine(args []string) string {
	var b strings.Builder
	for i, a := range args {
		if i > 0 {
			b.WriteByte(' ')
		}
		if a != "" && !strings.ContainsAny(a, " \t\n\v\"") {
			b.WriteString(a)
			continue
		}
		b.WriteByte('"')
		slashes := 0
		for _, c := range a {
			switch c {
			case '\\':
				slashes++
				continue
			case '"':
				b.WriteString(strings.Repeat(`\`, 2*slashes+1))
			default:
				b.WriteString(strings.Repeat(`\`, slashes))
			}
			slashes = 0
			b.WriteRune(c)
		}
		b.WriteString(strings.Repeat(`\`, 2*slashes))
		b.WriteByte('"')
	}
	return b.String()
}
