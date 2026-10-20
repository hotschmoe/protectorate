package e2e

import (
	"debug/buildinfo"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestBinaryRaceReports(t *testing.T) {
	c := newCluster(t)
	if kind := os.Getenv("PROT_E2E_RACE_PROBE"); kind != "" {
		peer := os.Getenv("PROT_E2E_RACE_PEER")
		options := "halt_on_error=0 exitcode=0 atexit_sleep_ms=0 log_path=stderr"
		if kind == "terminal" {
			marker := filepath.Join(c.root, "terminal-completed")
			c.call("SessionService", "CreateSession", map[string]any{
				"name": "race-terminal", "command": []string{"sh", "-c", `GORACE="$1" "$2"; printf completed > "$3"`, "sh", options, peer, marker},
			})
			c.eventually(func() bool { _, err := os.Stat(marker); return err == nil })
		} else if kind == "node-job" {
			job := c.call("JobService", "SubmitJob", map[string]any{
				"target":  map[string]any{"node": "peer"},
				"source":  map[string]any{"artifact": map[string]any{"path": peer}},
				"command": []string{"./race-peer"},
				"env":     map[string]string{"GORACE": options},
				"timeout": "20s",
			})
			c.eventually(func() bool {
				v := c.call("JobService", "GetJob", map[string]any{"id": job["id"]})
				if v["state"] == "JOB_STATE_FAILED" {
					t.Fatalf("race peer job failed: %v", v)
				}
				return v["state"] == "JOB_STATE_SUCCEEDED"
			})
		} else {
			wrapper := filepath.Join(c.root, "race-wrapper")
			if err := os.WriteFile(wrapper, []byte("#!/bin/sh\nexport GORACE='"+options+"'\nexec \"$PROT_E2E_RACE_PEER\"\n"), 0700); err != nil {
				t.Fatal(err)
			}
			c.env = append(c.env, "PROT_E2E_RACE_PEER="+peer)
			c.binary = wrapper
			switch kind {
			case "hub", "node":
				p := c.start(kind+"-race-probe", c.env)
				if err := <-p.done; err != nil {
					t.Fatalf("race peer exit: %v", err)
				}
			case "CLI":
				if output := c.cli(); !strings.Contains(output, "race peer completed") {
					t.Fatalf("missing CLI completion: %s", output)
				}
			case "CLI-file":
				c.binary = peer
				c.env = append(c.env, "GORACE=halt_on_error=0 exitcode=0 atexit_sleep_ms=0 log_path="+filepath.Join(c.root, "race-CLI-helper"))
				if output := c.cli(); output != "race peer completed\n" {
					t.Fatalf("unexpected CLI output: %s", output)
				}
			default:
				t.Fatalf("unknown race probe %q", kind)
			}
		}
		t.Log("scenario assertions passed")
		return
	}
	info, err := buildinfo.ReadFile(c.binary)
	if err != nil {
		t.Fatalf("read binary build info: %v", err)
	}
	race := false
	for _, setting := range info.Settings {
		if setting.Key == "-race" && setting.Value == "true" {
			race = true
		}
	}
	if !race {
		t.Skip("race-report regression scenario requires a race-built binary (make test-race)")
	}
	peer := filepath.Join(c.root, "race-peer")
	cmd := exec.CommandContext(c.ctx, "go", "build", "-race", "-o", peer, "./testdata/race")
	if output, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("build racing peer: %v\n%s", err, output)
	}
	for _, probe := range []struct{ kind, report string }{
		{"hub", "data race in hub-race-probe-"},
		{"node", "data race in node-race-probe-"},
		{"CLI", "data race in process ["},
		{"CLI-file", "data race in race-CLI-helper."},
		{"node-job", "job.log:"},
		{"terminal", "data race in tmux session race-terminal"},
	} {
		t.Run(probe.kind, func(t *testing.T) {
			if probe.kind == "terminal" {
				if _, err := exec.LookPath("tmux"); err != nil {
					t.Skipf("terminal race probe requires tmux: %v", err)
				}
			}
			cmd := exec.CommandContext(c.ctx, os.Args[0], "-test.run=^TestBinaryRaceReports$", "-test.v")
			cmd.Env = append(os.Environ(), "PROT_E2E_BINARY="+c.binary, "PROT_E2E_RACE_PROBE="+probe.kind, "PROT_E2E_RACE_PEER="+peer)
			output, err := cmd.CombinedOutput()
			if err == nil || !strings.Contains(string(output), "scenario assertions passed") || !strings.Contains(string(output), probe.report) || !strings.Contains(string(output), "WARNING: DATA RACE") || !strings.Contains(string(output), "main.main.func") || !strings.Contains(string(output), "--- FAIL: TestBinaryRaceReports") {
				t.Fatalf("scenario must fail with its process name and complete race report despite passing assertions: %v\n%s", err, output)
			}
		})
	}
}
