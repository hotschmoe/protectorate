package node

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/fsutil"
)

// RestartKind is how a node agent restarts itself after an update.
type RestartKind string

const (
	RestartSystemd RestartKind = "systemd"
	RestartTask    RestartKind = "task"
	RestartTermux  RestartKind = "termux"
	// RestartExit leaves with a failure status for a supervisor that restarts
	// on failure (a systemd system unit, a Windows service).
	RestartExit   RestartKind = "exit"
	RestartReexec RestartKind = "re-exec"
)

const (
	// ExitRestart is the status of RestartExit (EX_TEMPFAIL).
	ExitRestart = 75
	// trialWindow is how long a new binary has to be reached by the hub
	// before it rolls itself back.
	trialWindow = 4 * time.Minute
	unitFile    = "protectorate-node.service"
	serviceName = "protectorate-node"
)

// Supervision is what starts this agent, as far as it can tell.
type Supervision struct {
	GOOS string
	// SystemdUnit: running under the systemd user unit `node install` writes.
	SystemdUnit bool
	// Supervised: a service manager restarts the agent when it fails.
	Supervised bool
	// Task: the Windows logon task is registered.
	Task bool
	// TermuxService: the termux-services run directory exists.
	TermuxService bool
}

// PlanRestart picks how an agent restarts: through its startup registration
// when it has one, else by re-executing itself.
func PlanRestart(s Supervision) RestartKind {
	switch {
	case s.GOOS == "linux" && s.SystemdUnit:
		return RestartSystemd
	case s.Supervised:
		return RestartExit
	case s.GOOS == "windows" && s.Task:
		return RestartTask
	case s.GOOS == "android" && s.TermuxService:
		return RestartTermux
	}
	return RestartReexec
}

// DetectSupervision inspects the environment. serviceHost says the process
// runs under the Windows service manager.
func DetectSupervision(serviceHost bool) Supervision {
	s := Supervision{GOOS: runtime.GOOS}
	switch runtime.GOOS {
	case "linux":
		if os.Getenv("INVOCATION_ID") == "" {
			break
		}
		home, _ := os.UserHomeDir()
		if _, err := os.Stat(filepath.Join(home, ".config", "systemd", "user", unitFile)); err == nil {
			s.SystemdUnit = true
		} else {
			s.Supervised = true
		}
	case "windows":
		s.Supervised = serviceHost
		s.Task = exec.Command("schtasks", "/Query", "/TN", serviceName).Run() == nil
	case "android":
		prefix := os.Getenv("PREFIX")
		if prefix == "" {
			prefix = "/data/data/com.termux/files/usr"
		}
		_, err := os.Stat(filepath.Join(prefix, "var", "service", serviceName))
		s.TermuxService = err == nil
	}
	return s
}

type opKind string

const (
	opCopy   opKind = "copy"
	opRename opKind = "rename"
)

// fileOp is one step of a binary swap. Renames replace an existing target.
type fileOp struct {
	Kind     opKind
	From, To string
}

// swapPlan puts staged in place of exe and keeps the old exe as exe.old. A
// running Windows exe cannot be overwritten but can be renamed away; elsewhere
// the old binary is copied aside and the new one renamed over atomically.
func swapPlan(goos, exe, staged string) []fileOp {
	old := exe + ".old"
	if goos == "windows" {
		return []fileOp{{opRename, exe, old}, {opRename, staged, exe}}
	}
	return []fileOp{{opCopy, exe, old}, {opRename, staged, exe}}
}

// rollbackPlan restores exe.old. On Windows the failed binary is renamed away
// first because it is the running one.
func rollbackPlan(goos, exe string) []fileOp {
	if goos == "windows" {
		return []fileOp{{opRename, exe, exe + ".failed"}, {opRename, exe + ".old", exe}}
	}
	return []fileOp{{opRename, exe + ".old", exe}}
}

// runOps performs ops in order. When one fails, completed renames are undone
// so exe is never left missing.
func runOps(ops []fileOp) error {
	for i, op := range ops {
		var err error
		switch op.Kind {
		case opCopy:
			var b []byte
			if b, err = os.ReadFile(op.From); err == nil {
				err = fsutil.WriteFile(op.To, b, 0o755)
			}
		case opRename:
			err = fsutil.Rename(op.From, op.To)
		}
		if err == nil {
			continue
		}
		for j := i - 1; j >= 0; j-- {
			if ops[j].Kind == opRename {
				_ = fsutil.Rename(ops[j].To, ops[j].From)
			}
		}
		return fmt.Errorf("%s %s -> %s: %w", op.Kind, op.From, op.To, err)
	}
	return nil
}

const (
	phaseTrial      = "trial"
	phaseConfirmed  = "confirmed"
	phaseRolledBack = "rolledback"
)

// updateState is the marker next to the binary. It is never deleted, only
// rewritten.
type updateState struct {
	Phase    string    `json:"phase"`
	From     string    `json:"from"`
	To       string    `json:"to"`
	Attempts int       `json:"attempts"`
	Deadline time.Time `json:"deadline"`
	Reason   string    `json:"reason,omitempty"`
}

type startup int

const (
	startPlain startup = iota
	// startTrial: the first run of a new binary; count it and arm the watchdog.
	startTrial
	// startRollback: the new binary was started before and never confirmed,
	// or ran out of time: restore the previous one.
	startRollback
)

// decideStartup is the rollback state machine's start-of-process step.
func decideStartup(st *updateState, running string, now time.Time) startup {
	switch {
	case st == nil, st.Phase != phaseTrial, running != st.To:
		return startPlain
	case st.Attempts >= 1, now.After(st.Deadline):
		return startRollback
	}
	return startTrial
}

// Updater swaps the agent's own binary on the hub's request and rolls a bad
// one back.
type Updater struct {
	Files *Files
	// Exe is the running binary's path, resolved before any swap.
	Exe     string
	Version string
	// Supervision says how a restart happens.
	Supervision Supervision
	// Restart asks the agent to shut down and then restart as kind. It must
	// not block.
	Restart func(RestartKind)
	// RunVersion reports the version of a candidate binary; default runs it.
	RunVersion func(ctx context.Context, path string) (string, error)
	Now        func() time.Time

	mu        sync.Mutex
	confirmed bool
	trial     bool
}

func (u *Updater) now() time.Time {
	if u.Now != nil {
		return u.Now()
	}
	return time.Now()
}

func (u *Updater) statePath() string { return u.Exe + ".update" }

func (u *Updater) load() *updateState {
	b, err := os.ReadFile(u.statePath())
	if err != nil {
		return nil
	}
	var st updateState
	if json.Unmarshal(b, &st) != nil {
		return nil
	}
	return &st
}

func (u *Updater) save(st *updateState) error {
	b, err := json.Marshal(st)
	if err != nil {
		return err
	}
	return fsutil.WriteFile(u.statePath(), b, 0o644)
}

// Start runs at process start. It returns true when it rolled back and the
// process must not serve: the restart was already requested.
func (u *Updater) Start() (rolledBack bool, err error) {
	u.mu.Lock()
	defer u.mu.Unlock()
	st := u.load()
	switch decideStartup(st, u.Version, u.now()) {
	case startTrial:
		st.Attempts++
		if err := u.save(st); err != nil {
			return false, fmt.Errorf("record update trial: %w", err)
		}
		u.trial = true
		go u.watchdog(st.Deadline)
	case startRollback:
		return true, u.rollbackLocked(st, "new binary never confirmed")
	}
	return false, nil
}

func (u *Updater) watchdog(deadline time.Time) {
	time.Sleep(max(0, deadline.Sub(u.now())))
	u.mu.Lock()
	defer u.mu.Unlock()
	if u.confirmed {
		return
	}
	if st := u.load(); st != nil && st.Phase == phaseTrial {
		_ = u.rollbackLocked(st, "hub did not reach the new binary in time")
	}
}

func (u *Updater) rollbackLocked(st *updateState, reason string) error {
	if err := runOps(rollbackPlan(runtime.GOOS, u.Exe)); err != nil {
		return fmt.Errorf("roll back to %s: %w", st.From, err)
	}
	st.Phase, st.Reason = phaseRolledBack, reason
	if err := u.save(st); err != nil {
		return err
	}
	u.confirmed = true
	u.Restart(PlanRestart(u.Supervision))
	return nil
}

// Touch records that the hub reached this agent, which confirms a trial.
func (u *Updater) Touch() {
	u.mu.Lock()
	defer u.mu.Unlock()
	if !u.trial || u.confirmed {
		return
	}
	if st := u.load(); st != nil && st.Phase == phaseTrial {
		st.Phase = phaseConfirmed
		if u.save(st) != nil {
			return
		}
	}
	u.confirmed = true
}

type versionInfo struct {
	Version string `json:"version"`
	OS      string `json:"os"`
	Arch    string `json:"arch"`
}

func runVersion(ctx context.Context, path string) (string, error) {
	ctx, cancel := context.WithTimeout(ctx, 15*time.Second)
	defer cancel()
	out, err := exec.CommandContext(ctx, path, "version", "--json").Output()
	if err != nil {
		return "", fmt.Errorf("run %s version: %w", path, err)
	}
	var v versionInfo
	if err := json.Unmarshal(out, &v); err != nil {
		return "", fmt.Errorf("parse version output %q: %w", strings.TrimSpace(string(out)), err)
	}
	if v.OS != runtime.GOOS || v.Arch != runtime.GOARCH {
		return "", fmt.Errorf("binary is for %s/%s, this node is %s/%s", v.OS, v.Arch, runtime.GOOS, runtime.GOARCH)
	}
	if v.Version == "" {
		return "", errors.New("binary reports no version")
	}
	return v.Version, nil
}

func failed(format string, args ...any) error {
	return connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf(format, args...))
}

// Apply installs the staged binary and schedules the restart.
func (u *Updater) Apply(ctx context.Context, req *pb.ApplyUpdateRequest) (*pb.ApplyUpdateResponse, error) {
	u.mu.Lock()
	defer u.mu.Unlock()
	if u.Files == nil {
		return nil, failed("this node serves no file transfers, so it has nothing staged")
	}
	if st := u.load(); st != nil && st.Phase == phaseTrial {
		return nil, failed("update to %s is still on trial; wait for it to be confirmed or rolled back", st.To)
	}
	staged, err := u.Files.resolve(req.Path)
	if err != nil {
		return nil, err
	}
	b, err := os.ReadFile(staged)
	if err != nil {
		return nil, connect.NewError(connect.CodeNotFound, fmt.Errorf("read staged binary: %w", err))
	}
	sum := sha256.Sum256(b)
	if uint64(len(b)) != req.Size || !strings.EqualFold(hex.EncodeToString(sum[:]), req.Sha256) {
		return nil, failed("staged file is %d bytes, sha256 %x; expected %d bytes, sha256 %s", len(b), sum, req.Size, req.Sha256)
	}
	cand := u.Exe + ".new"
	if err := fsutil.WriteFile(cand, b, 0o755); err != nil {
		return nil, fmt.Errorf("stage next to the binary: %w", err)
	}
	rv := u.RunVersion
	if rv == nil {
		rv = runVersion
	}
	ver, err := rv(ctx, cand)
	if err != nil {
		return nil, failed("new binary does not run: %v", err)
	}
	if err := runOps(swapPlan(runtime.GOOS, u.Exe, cand)); err != nil {
		return nil, fmt.Errorf("swap binary: %w", err)
	}
	st := &updateState{Phase: phaseTrial, From: u.Version, To: ver, Deadline: u.now().Add(trialWindow)}
	if err := u.save(st); err != nil {
		_ = runOps(rollbackPlan(runtime.GOOS, u.Exe))
		return nil, fmt.Errorf("record update trial: %w", err)
	}
	kind := PlanRestart(u.Supervision)
	if u.Restart != nil {
		restart := u.Restart
		go func() {
			time.Sleep(500 * time.Millisecond)
			restart(kind)
		}()
	}
	return &pb.ApplyUpdateResponse{Version: ver, PreviousVersion: u.Version, Restart: string(kind)}, nil
}
