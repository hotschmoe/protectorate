package sleeve

import (
	"context"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"io"
	"io/fs"
	"log"
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

func (c Config) goCacheLower() string { return filepath.Join(c.Home, "cache", "go-build-lower") }

// PrepareCaches publishes one immutable shared lower, never a live Go cache.
func (c Config) PrepareCaches(ctx context.Context) error {
	log.Printf("sleeve runtime: bwrap=%s version=%q overlays=%t", c.bwrap(), c.BwrapVersion, c.Overlays)
	if !c.Overlays || c.GoCache == "" {
		return nil
	}
	lower := c.goCacheLower()
	if fi, err := os.Stat(lower); err == nil && fi.IsDir() {
		return nil
	}
	if err := os.MkdirAll(filepath.Dir(lower), 0700); err != nil {
		return fmt.Errorf("create shared Go cache parent: %w", err)
	}
	stage, err := os.MkdirTemp(filepath.Dir(lower), ".go-build-")
	if err != nil {
		return fmt.Errorf("stage shared Go cache: %w", err)
	}
	defer os.RemoveAll(stage)
	log.Printf("snapshot Go cache %s into %s (reflink when supported)", c.GoCache, lower)
	cmd := exec.CommandContext(ctx, "cp", "-a", "--reflink=auto", "--", c.GoCache+"/.", stage)
	if b, err := cmd.CombinedOutput(); err != nil {
		return fmt.Errorf("snapshot host Go cache: %w: %s", err, b)
	}
	if err := freezeGoCache(ctx, stage); err != nil {
		return fmt.Errorf("freeze shared Go cache: %w", err)
	}
	if err := os.Rename(stage, lower); err != nil {
		return fmt.Errorf("publish shared Go cache: %w", err)
	}
	return nil
}

func freezeGoCache(ctx context.Context, dir string) error {
	// Go refreshes old mtimes on cache hits, which would copy up whole archives.
	future := time.Now().AddDate(100, 0, 0)
	return filepath.WalkDir(dir, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if err := ctx.Err(); err != nil {
			return err
		}
		parent := filepath.Base(filepath.Dir(path))
		if d.Type()&os.ModeSymlink != 0 {
			if (len(d.Name()) == 66 && (strings.HasSuffix(d.Name(), "-a") || strings.HasSuffix(d.Name(), "-d"))) || (len(parent) == 66 && strings.HasSuffix(parent, "-d")) {
				return quarantineGoCache(path)
			}
			return nil
		}
		if d.Type().IsRegular() {
			name := d.Name()
			if len(name) == 66 && strings.HasSuffix(name, "-a") {
				data, err := os.ReadFile(path)
				if err != nil {
					return err
				}
				fields := strings.Fields(string(data))
				if len(fields) != 5 || fields[0] != "v1" || fields[1] != name[:64] {
					return os.Rename(path, path+".invalid")
				}
			}
			if len(parent) == 66 && strings.HasSuffix(parent, "-d") {
				name = parent
			}
			if len(name) == 66 && strings.HasSuffix(name, "-d") {
				f, err := os.Open(path)
				if err != nil {
					return err
				}
				h := sha256.New()
				_, readErr := io.Copy(h, f)
				closeErr := f.Close()
				if readErr != nil {
					return readErr
				}
				if closeErr != nil {
					return closeErr
				}
				if fmt.Sprintf("%x", h.Sum(nil)) != name[:64] {
					return quarantineGoCache(path)
				}
			}
		}
		return os.Chtimes(path, future, future)
	})
}

func quarantineGoCache(path string) error {
	parent := filepath.Dir(path)
	if len(filepath.Base(parent)) == 66 && strings.HasSuffix(parent, "-d") {
		if err := os.Rename(parent, parent+".invalid"); err != nil {
			return err
		}
		return fs.SkipDir
	}
	return os.Rename(path, path+".invalid")
}
