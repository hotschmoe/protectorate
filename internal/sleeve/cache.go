package sleeve

import (
	"context"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

type Overlay struct {
	Lower, Upper, Work, Target string
}

func (c *Config) probeRuntime() {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	if p, err := exec.LookPath("bwrap"); err == nil {
		c.BwrapPath, _ = filepath.Abs(p)
		if b, err := exec.CommandContext(ctx, c.BwrapPath, "--version").Output(); err == nil {
			c.BwrapVersion = strings.TrimSpace(string(b))
		}
		if b, err := exec.CommandContext(ctx, c.BwrapPath, "--help").Output(); err == nil {
			c.Overlays = strings.Contains(string(b), "--overlay-src ") && strings.Contains(string(b), "--overlay ")
		}
	}
	var env struct{ GOCACHE, GOMODCACHE string }
	cmd := exec.CommandContext(ctx, "go", "env", "-json", "GOCACHE", "GOMODCACHE")
	cmd.Dir = c.UserHome
	if b, err := cmd.Output(); err == nil && json.Unmarshal(b, &env) == nil {
		for src, dst := range map[string]*string{env.GOCACHE: &c.GoCache, env.GOMODCACHE: &c.GoModCache} {
			if filepath.IsAbs(src) && !c.touchesCredentials(src) {
				if p, err := filepath.EvalSymlinks(src); err == nil && !c.touchesCredentials(p) {
					if fi, err := os.Stat(p); err == nil && fi.IsDir() {
						*dst = p
					}
				}
			}
		}
	}
}

func (c Config) bwrap() string {
	if c.BwrapPath != "" {
		return c.BwrapPath
	}
	return "bwrap"
}
