package sleeve

import (
	"bufio"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"syscall"
)

func validBranch(s string) bool {
	if s == "" || s == "HEAD" || strings.HasPrefix(s, "-") || strings.HasSuffix(s, ".") || strings.Contains(s, "..") || strings.Contains(s, "@{") {
		return false
	}
	for _, r := range s {
		if r <= ' ' || r == 127 || strings.ContainsRune(`~^:?*[\`, r) {
			return false
		}
	}
	for _, part := range strings.Split(s, "/") {
		if part == "" || strings.HasPrefix(part, ".") || strings.HasSuffix(part, ".lock") {
			return false
		}
	}
	return true
}

func gitDirs(dir string) []string {
	if linked := linkedGitDirs(dir); len(linked) > 0 {
		return linked
	}
	gd := filepath.Join(dir, ".git")
	if fi, err := os.Stat(gd); err == nil && fi.IsDir() {
		return []string{gd}
	}
	return nil
}

func (c Config) baseMounts(dir string) ([]Mount, error) {
	if err := c.CheckWorkspace("base", dir); err != nil {
		return nil, err
	}
	dirs := gitDirs(dir)
	if len(dirs) == 0 {
		return nil, fmt.Errorf("%w: base %s is not a git checkout", ErrInvalid, dir)
	}
	var mounts []Mount
	for _, d := range dirs {
		mounts = append(mounts, Mount{Source: d, Target: d})
	}
	return c.ResolveMounts(mounts)
}

// BranchTip reads loose and packed refs without starting repository programs.
func BranchTip(dir, branch string) (string, error) {
	if !validBranch(branch) {
		return "", fmt.Errorf("%w: invalid base branch %q", ErrInvalid, branch)
	}
	dirs := gitDirs(dir)
	if len(dirs) == 0 {
		return "", fmt.Errorf("%w: base is not a git checkout", ErrInvalid)
	}
	root, err := os.OpenRoot(dirs[len(dirs)-1])
	if err != nil {
		return "", fmt.Errorf("open base git directory: %w", err)
	}
	defer root.Close()
	ref := "refs/heads/" + branch
	var sha string
	if raw, err := readGitMetadata(root, ref); err == nil {
		sha = strings.TrimSpace(string(raw))
	} else if !errors.Is(err, os.ErrNotExist) {
		return "", fmt.Errorf("read base branch %s: %w", branch, err)
	} else {
		f, err := root.OpenFile("packed-refs", os.O_RDONLY|syscall.O_NONBLOCK, 0)
		if err == nil {
			defer f.Close()
			if fi, err := f.Stat(); err != nil || !fi.Mode().IsRegular() {
				return "", fmt.Errorf("%w: base packed-refs is not a regular file", ErrInvalid)
			}
			sc := bufio.NewScanner(io.LimitReader(f, 16<<20))
			for sc.Scan() {
				fields := strings.Fields(sc.Text())
				if len(fields) == 2 && fields[1] == ref {
					sha = fields[0]
					break
				}
			}
			if err := sc.Err(); err != nil {
				return "", fmt.Errorf("read packed base refs: %w", err)
			}
		} else if !errors.Is(err, os.ErrNotExist) {
			return "", fmt.Errorf("open packed base refs: %w", err)
		}
	}
	if len(sha) != 40 && len(sha) != 64 {
		return "", fmt.Errorf("%w: base branch %s has no commit", ErrInvalid, branch)
	}
	for _, r := range sha {
		if !strings.ContainsRune("0123456789abcdef", r) {
			return "", fmt.Errorf("%w: invalid base tip", ErrInvalid)
		}
	}
	return sha, nil
}

func (m *Manager) SetBaseTip(name, from, to string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	e := m.sleeves[name]
	if e == nil {
		return ErrNotFound
	}
	if e.BaseTip != from {
		return nil
	}
	e.BaseTip = to
	if err := m.saveLocked(); err != nil {
		e.BaseTip = from
		return fmt.Errorf("persist base tip: %w", err)
	}
	return nil
}
