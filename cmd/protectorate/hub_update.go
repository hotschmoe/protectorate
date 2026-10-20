package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"sync/atomic"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/fsutil"
	"github.com/hotschmoe/protectorate/internal/hub"
	"github.com/hotschmoe/protectorate/internal/node"
	"github.com/hotschmoe/protectorate/internal/release"
)

type hubTrial struct {
	Phase           string    `json:"phase"`
	Exe             string    `json:"exe"`
	Args            []string  `json:"args"`
	Dir             string    `json:"release_dir"`
	PreviousRelease string    `json:"previous_release"`
	Version         string    `json:"version"`
	PreviousVersion string    `json:"previous_version"`
	Channel         string    `json:"channel"`
	URL             string    `json:"url"`
	TokenFile       string    `json:"token_file"`
	Started         time.Time `json:"started"`
	Deadline        time.Time `json:"deadline"`
	PID             int       `json:"pid"`
	PIDStart        string    `json:"pid_start"`
	GuardPID        int       `json:"guard_pid"`
	Supervised      bool      `json:"supervised"`
	Error           string    `json:"error,omitempty"`
}

func saveHubTrial(path string, st *hubTrial) error {
	b, err := json.Marshal(st)
	if err != nil {
		return fmt.Errorf("encode hub trial: %w", err)
	}
	if err := fsutil.WriteFile(path, b, 0600); err != nil {
		return fmt.Errorf("save hub trial: %w", err)
	}
	return nil
}
func loadHubTrial(path string) (*hubTrial, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read hub trial: %w", err)
	}
	var st hubTrial
	if err := json.Unmarshal(b, &st); err != nil {
		return nil, fmt.Errorf("decode hub trial: %w", err)
	}
	return &st, nil
}

func prepareHubUpdate(ctx context.Context, h *hub.Hub, tokFile string, timeout time.Duration, stop context.CancelFunc, wantRestart *atomic.Bool, path string, r release.Info) (func(), error) {
	if runtime.GOOS != "linux" {
		return nil, errors.New("hub self-update requires Linux")
	}
	home, err := hub.Home()
	if err != nil {
		return nil, err
	}
	statePath := filepath.Join(home, "hub-update.json")
	if st, err := loadHubTrial(statePath); err == nil {
		if st.Phase == "trial" {
			return nil, errors.New("previous hub update is still awaiting health confirmation")
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		return nil, fmt.Errorf("inspect previous hub update: %w", err)
	}
	pidStart, _, err := hubProcessIdentity(os.Getpid())
	if err != nil {
		return nil, fmt.Errorf("record hub process identity: %w", err)
	}
	exe, err := filepath.EvalSymlinks(h.SelfExe)
	if err != nil {
		return nil, fmt.Errorf("resolve hub executable symlink: %w", err)
	}
	if relative, err := filepath.Rel(h.Releases.Dir, exe); err == nil && relative != ".." && !strings.HasPrefix(relative, ".."+string(filepath.Separator)) {
		return nil, errors.New("hub executable is inside the immutable release cache; install a copy outside it before self-updating")
	}
	probeCtx, cancel := context.WithTimeout(ctx, 15*time.Second)
	defer cancel()
	out, err := exec.CommandContext(probeCtx, path, "version", "--json").Output()
	if err != nil {
		return nil, fmt.Errorf("candidate version: %w", err)
	}
	var v struct{ Version, OS, Arch string }
	if err := json.Unmarshal(out, &v); err != nil {
		return nil, fmt.Errorf("parse candidate version: %w", err)
	}
	if v.Version != r.Version || v.OS != runtime.GOOS || v.Arch != runtime.GOARCH {
		return nil, fmt.Errorf("candidate reports %s %s/%s; expected %s %s/%s", v.Version, v.OS, v.Arch, r.Version, runtime.GOOS, runtime.GOARCH)
	}
	old, err := os.ReadFile(exe)
	if err != nil {
		return nil, fmt.Errorf("read previous hub: %w", err)
	}
	if err := fsutil.WriteFile(exe+".previous", old, 0755); err != nil {
		return nil, fmt.Errorf("save previous hub: %w", err)
	}
	next, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read candidate hub: %w", err)
	}
	// Versioned release files remain immutable; the installed executable is a copy.
	if err := fsutil.WriteFile(exe+".next", next, 0755); err != nil {
		return nil, fmt.Errorf("stage hub executable: %w", err)
	}
	st := &hubTrial{Phase: "trial", Exe: exe, Args: append([]string{}, os.Args[1:]...), Dir: h.Releases.Dir, PreviousRelease: h.Releases.Current().Version, Version: r.Version, PreviousVersion: h.Version, Channel: r.Channel, URL: h.SelfURL, TokenFile: tokFile, Started: time.Now(), Deadline: time.Now().Add(timeout), PID: os.Getpid(), PIDStart: pidStart, Supervised: os.Getenv("INVOCATION_ID") != ""}
	if err := saveHubTrial(statePath, st); err != nil {
		return nil, err
	}
	abort := func(cause error) (func(), error) {
		st.Phase = "aborted"
		st.Error = cause.Error()
		_ = saveHubTrial(statePath, st)
		return nil, cause
	}
	if err := fsutil.Rename(exe+".next", exe); err != nil {
		return abort(fmt.Errorf("install hub executable: %w", err))
	}
	if err := h.Releases.Activate(r.Version); err != nil {
		_ = fsutil.Rename(exe+".previous", exe)
		return abort(err)
	}
	guardArgs := []string{"hub", "update-guard", "--state", statePath}
	if st.Supervised {
		// A separate unit survives the hub unit's control-group cleanup.
		out, err = exec.CommandContext(ctx, "systemd-run", "--user", "--collect", "--unit", fmt.Sprintf("protectorate-update-%d-%d", st.PID, st.Started.UnixNano()), exe+".previous", guardArgs[0], guardArgs[1], guardArgs[2], guardArgs[3]).CombinedOutput()
		if err != nil {
			err = fmt.Errorf("start update watchdog unit: %w: %s", err, strings.TrimSpace(string(out)))
		}
	} else {
		err = node.StartDetached(exe+".previous", guardArgs)
	}
	if err != nil {
		_ = fsutil.Rename(exe+".previous", exe)
		_ = h.Releases.Restore(st.PreviousRelease)
		return abort(fmt.Errorf("arm hub rollback watchdog: %w", err))
	}
	return func() { wantRestart.Store(true); stop() }, nil
}

func hubUpdateGuardCmd() *command {
	var state string
	c := &command{Use: "update-guard", Short: "Monitor a hub update trial", Hidden: true, Args: noArgs, RunE: func(cmd *command, _ []string) error { return guardHubUpdate(cmd.Context(), state) }}
	c.Flags().StringVar(&state, "state", "", "private hub trial state")
	return c
}
func guardHubUpdate(ctx context.Context, path string) error {
	st, err := loadHubTrial(path)
	if err != nil {
		return err
	}
	if st.Phase != "trial" {
		return nil
	}
	st.GuardPID = os.Getpid()
	if err := saveHubTrial(path, st); err != nil {
		return err
	}
	b, err := os.ReadFile(st.TokenFile)
	if err != nil {
		return fmt.Errorf("read health credential: %w", err)
	}
	cl := protectoratev1connect.NewHubServiceClient(&http.Client{Timeout: time.Second, Transport: bearer{tok: strings.TrimSpace(string(b)), next: http.DefaultTransport}}, st.URL)
	tick := time.NewTicker(200 * time.Millisecond)
	defer tick.Stop()
	source, err := release.New("hotschmoe/protectorate", st.Channel, "", st.Dir)
	if err != nil {
		return err
	}
	for time.Now().Before(st.Deadline) {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-tick.C:
		}
		if now, e := loadHubTrial(path); e == nil && now.Phase != "trial" {
			return nil
		}
		res, err := cl.GetStatus(ctx, connect.NewRequest(&pb.GetStatusRequest{}))
		if err == nil && res.Msg.Version == st.Version && res.Msg.StartedAt != nil && res.Msg.StartedAt.AsTime().After(st.Started) {
			pruneErr := source.Prune(st.PreviousRelease)
			st.Phase = "confirmed"
			if err := saveHubTrial(path, st); err != nil {
				return err
			}
			return pruneErr
		}
	}
	// The watchdog is the previous known-good binary, independent of candidate startup.
	st.Phase = "rolledback"
	if err := fsutil.Rename(st.Exe+".previous", st.Exe); err != nil {
		return fmt.Errorf("restore hub binary: %w", err)
	}
	if err := source.Restore(st.PreviousRelease); err != nil {
		return fmt.Errorf("restore release pointer: %w", err)
	}
	previous, _ := os.Readlink(filepath.Join(source.Dir, "previous"))
	if err := source.Prune(previous); err != nil {
		st.Error = fmt.Sprintf("prune after rollback: %v", err)
	}
	if st.Supervised {
		out, err := exec.CommandContext(ctx, "systemctl", "--user", "restart", "protectorate-hub.service").CombinedOutput()
		if err != nil {
			return fmt.Errorf("restart previous hub: %w: %s", err, out)
		}
		return saveHubTrial(path, st)
	}
	if err := stopHubCandidate(ctx, st); err != nil {
		return fmt.Errorf("stop failed hub before rollback: %w", err)
	}
	if err := saveHubTrial(path, st); err != nil {
		return err
	}
	return reexecHub(st.Exe, st.Args)
}

func hubProcessIdentity(pid int) (string, string, error) {
	b, err := os.ReadFile(fmt.Sprintf("/proc/%d/stat", pid))
	if err != nil {
		return "", "", err
	}
	end := strings.LastIndexByte(string(b), ')')
	if end < 0 {
		return "", "", errors.New("process stat has no command field")
	}
	fields := strings.Fields(string(b[end+1:]))
	if len(fields) < 20 {
		return "", "", errors.New("process stat has no start time")
	}
	return fields[19], fields[0], nil
}

func stopHubCandidate(ctx context.Context, st *hubTrial) error {
	if st.PID <= 0 || st.PIDStart == "" {
		return errors.New("hub trial has no process identity")
	}
	process, err := os.FindProcess(st.PID)
	if err != nil {
		return fmt.Errorf("find candidate process: %w", err)
	}
	defer process.Release()
	alive := func() (bool, error) {
		start, state, err := hubProcessIdentity(st.PID)
		if errors.Is(err, os.ErrNotExist) {
			return false, nil
		}
		if err != nil {
			return false, fmt.Errorf("read candidate process identity: %w", err)
		}
		return start == st.PIDStart && state != "Z" && state != "X", nil
	}
	wait := func(timeout time.Duration) (bool, error) {
		deadline := time.NewTimer(timeout)
		defer deadline.Stop()
		tick := time.NewTicker(50 * time.Millisecond)
		defer tick.Stop()
		for {
			running, err := alive()
			if err != nil || !running {
				return !running, err
			}
			select {
			case <-ctx.Done():
				return false, ctx.Err()
			case <-deadline.C:
				return false, nil
			case <-tick.C:
			}
		}
	}
	if running, err := alive(); err != nil || !running {
		return err
	}
	if err := process.Signal(os.Interrupt); err != nil && !errors.Is(err, os.ErrProcessDone) {
		return fmt.Errorf("interrupt candidate: %w", err)
	}
	if exited, err := wait(10 * time.Second); err != nil || exited {
		return err
	}
	if err := process.Kill(); err != nil && !errors.Is(err, os.ErrProcessDone) {
		return fmt.Errorf("kill unresponsive candidate: %w", err)
	}
	if exited, err := wait(3 * time.Second); err != nil {
		return err
	} else if !exited {
		return errors.New("candidate did not exit after being killed")
	}
	return nil
}

func hubUpdateCmd(o *opts) *command {
	var channel, version string
	c := &command{Use: "update", Short: "Fetch a verified GitHub release and restart the hub", Args: noArgs, RunE: func(cmd *command, _ []string) error {
		hc, err := o.httpClient()
		if err != nil {
			return err
		}
		cl := protectoratev1connect.NewReleaseServiceClient(hc, o.hubURL)
		stream, err := cl.UpdateHub(cmd.Context(), connect.NewRequest(&pb.UpdateHubRequest{Channel: channel, Version: version}))
		if err != nil {
			return err
		}
		defer stream.Close()
		target, previous := "", ""
		restarting := false
		for stream.Receive() {
			p := stream.Msg()
			restarting = restarting || p.Phase == pb.UpdatePhase_UPDATE_PHASE_RESTARTING
			target, previous = p.Version, p.PreviousVersion
			if err := o.out(cmd).print(p, func(w io.Writer) {
				if p.TotalBytes == 0 || p.SentBytes == p.TotalBytes {
					fmt.Fprintf(w, "%s\t%s\n", p.Phase, p.Message)
				}
			}); err != nil {
				return err
			}
			if p.Phase == pb.UpdatePhase_UPDATE_PHASE_SKIPPED {
				return nil
			}
		}
		if err := stream.Err(); err != nil && !restarting {
			return err
		}
		if target == "" {
			return errors.New("hub update ended without a target version")
		}
		status := protectoratev1connect.NewHubServiceClient(hc, o.hubURL)
		deadline := time.NewTimer(3 * time.Minute)
		defer deadline.Stop()
		tick := time.NewTicker(time.Second)
		defer tick.Stop()
		sawDown := false
		for {
			select {
			case <-cmd.Context().Done():
				return cmd.Context().Err()
			case <-deadline.C:
				return errors.New("hub did not confirm update within three minutes")
			case <-tick.C:
			}
			probeCtx, cancel := context.WithTimeout(cmd.Context(), 2*time.Second)
			res, err := status.GetStatus(probeCtx, connect.NewRequest(&pb.GetStatusRequest{}))
			cancel()
			if err != nil {
				sawDown = true
				continue
			}
			if res.Msg.Version == target {
				return o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprintln(w, "hub reports "+target) })
			}
			if sawDown && res.Msg.Version == previous {
				return fmt.Errorf("hub rolled back to %s", previous)
			}
		}
	}}
	c.Flags().StringVar(&channel, "channel", "", "release channel override (stable or dev)")
	c.Flags().StringVar(&version, "version", "", "specific GitHub release tag")
	return c
}
