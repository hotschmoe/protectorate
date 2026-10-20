package e2e

import (
	"bytes"
	"context"
	"fmt"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
)

func (c *cluster) raceEnv(name string) string {
	return "GORACE=halt_on_error=1 atexit_sleep_ms=0 log_path=" + filepath.Join(c.root, "race-"+name)
}

func (c *cluster) checkRace(name string, output []byte) {
	c.owner.Helper()
	if bytes.Contains(output, []byte("WARNING: DATA RACE")) {
		c.owner.Errorf("data race in %s:\n%s", name, output)
	}
}

func (c *cluster) commandOutput(cmd *exec.Cmd) ([]byte, error) {
	c.t.Helper()
	output, err := cmd.CombinedOutput()
	c.checkRace(fmt.Sprintf("process %v", cmd.Args), output)
	return output, err
}

func (c *cluster) checkTmuxRaces(ctx context.Context) {
	socket := "e2e-" + filepath.Base(c.root)
	cmd := exec.CommandContext(ctx, "tmux", "-L", socket, "list-panes", "-a", "-F", "#{pane_id} #{session_name}")
	cmd.Env = c.env
	output, err := cmd.Output()
	if err != nil {
		return
	}
	for line := range strings.SplitSeq(strings.TrimSpace(string(output)), "\n") {
		pane, session, ok := strings.Cut(line, " ")
		if !ok {
			c.owner.Errorf("read tmux pane identity: %q", line)
			continue
		}
		cmd := exec.CommandContext(ctx, "tmux", "-L", socket, "capture-pane", "-p", "-J", "-S", "-", "-t", pane)
		cmd.Env = c.env
		output, err := cmd.CombinedOutput()
		if err != nil {
			c.owner.Errorf("capture tmux session %s pane %s: %v\n%s", session, pane, err, output)
			continue
		}
		c.checkRace("tmux session "+session+" pane "+pane, output)
	}
}

func (c *cluster) checkRaceLogs() {
	c.owner.Helper()
	err := filepath.WalkDir(c.root, func(path string, entry fs.DirEntry, err error) error {
		if err != nil {
			return fmt.Errorf("visit process logs: %w", err)
		}
		if !entry.Type().IsRegular() {
			return nil
		}
		_, pidErr := strconv.ParseUint(strings.TrimPrefix(filepath.Ext(entry.Name()), "."), 10, 64)
		isReport := filepath.Dir(path) == c.root && strings.HasPrefix(entry.Name(), "race-") && pidErr == nil
		isChat := entry.Name() == "chat.jsonl" || strings.HasPrefix(entry.Name(), "chat.jsonl.")
		if !strings.HasSuffix(entry.Name(), ".log") && !isReport && !isChat {
			return nil
		}
		output, err := os.ReadFile(path)
		if err != nil {
			return fmt.Errorf("read process log %s: %w", path, err)
		}
		name, err := filepath.Rel(c.root, path)
		if err != nil {
			return fmt.Errorf("name process log %s: %w", path, err)
		}
		c.checkRace(name, output)
		return nil
	})
	if err != nil {
		c.owner.Errorf("check process race reports: %v", err)
	}
}
