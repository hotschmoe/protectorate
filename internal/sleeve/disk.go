package sleeve

import (
	"context"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"time"
)

var cacheDirs = []string{".cache/go-build", ".cache/go-mod", ".cache/pub", ".cache/flutter", ".cache/overlays"}

func (m *Manager) discoverHomes() {
	if dirs, err := os.ReadDir(filepath.Join(m.cfg.Home, "sleeves")); err == nil {
		for _, dir := range dirs {
			name := dir.Name()
			if !dir.IsDir() || ValidName(name) != nil || m.sleeves[name] != nil {
				continue
			}
			if fi, err := os.Lstat(m.cfg.sleeveHome(name)); err == nil && fi.IsDir() {
				m.sleeves[name] = &entry{Info: Info{Name: name, State: Exited, Killed: true, Created: fi.ModTime()}}
			}
		}
	}
}

func homeSize(ctx context.Context, home string) (total, caches, goBuild uint64, err error) {
	err = filepath.WalkDir(home, func(path string, d fs.DirEntry, walkErr error) error {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if errors.Is(walkErr, os.ErrNotExist) {
			return nil
		}
		if walkErr != nil {
			return walkErr
		}
		if !d.Type().IsRegular() {
			return nil
		}
		info, e := d.Info()
		if errors.Is(e, os.ErrNotExist) {
			return nil
		}
		if e != nil {
			return e
		}
		n := uint64(info.Size())
		total += n
		rel, _ := filepath.Rel(home, path)
		for _, cache := range cacheDirs {
			if rel == cache || strings.HasPrefix(rel, cache+string(filepath.Separator)) {
				caches += n
				if cache == cacheDirs[0] || within(rel, ".cache/overlays/go-build") {
					goBuild += n
				}
				break
			}
		}
		return nil
	})
	return
}

type PruneOptions struct {
	Yes, Caches, Homes, Workspaces, Running bool
}

type DiskEntry struct {
	Sleeve, Path, Kind, Reason string
	Bytes                      uint64
	Eligible, Removed          bool
}

// Plain directories in workspaces/ are the hub's clone namespace. Everything
// else, including edit-in-place owner checkouts, is outside prune's authority.
func (m *Manager) Prune(ctx context.Context, opts PruneOptions) ([]DiskEntry, error) {
	if !opts.Caches && !opts.Homes && !opts.Workspaces {
		opts.Caches = true
	}
	var out []DiskEntry
	for _, category := range []string{"sleeves", "workspaces"} {
		parent := filepath.Join(m.cfg.Home, category)
		root, err := openDiskRoot(parent)
		if errors.Is(err, os.ErrNotExist) {
			continue
		}
		if err != nil {
			return out, fmt.Errorf("open prune %s: %w", category, err)
		}
		entries, err := fs.ReadDir(root.FS(), ".")
		if err != nil {
			root.Close()
			return out, fmt.Errorf("list prune %s: %w", category, err)
		}
		for _, entry := range entries {
			name := entry.Name()
			if !entry.IsDir() || ValidName(name) != nil {
				continue
			}
			if err := m.reserve(ctx, name); err != nil {
				root.Close()
				return out, err
			}
			rows, err := m.pruneEntry(ctx, root, parent, category, name, opts)
			m.release(name)
			out = append(out, rows...)
			if err != nil {
				root.Close()
				return out, err
			}
		}
		root.Close()
	}
	return out, nil
}

func openDiskRoot(path string) (*os.Root, error) {
	real, err := filepath.EvalSymlinks(path)
	if err != nil {
		return nil, err
	}
	if real != filepath.Clean(path) {
		return nil, fmt.Errorf("symlinked disk path %q", path)
	}
	return os.OpenRoot(path)
}

func (m *Manager) diskBusyReason(path, name string) string {
	m.mu.Lock()
	defer m.mu.Unlock()
	for _, e := range m.sleeves {
		if e.State == Running && (e.Name == name || within(e.Workspace, path) || within(path, e.Workspace)) {
			return "running sleeve " + e.Name
		}
		if e.State == Running {
			for _, mount := range e.ReadOnly {
				if within(path, mount.Source) || within(mount.Source, path) {
					return "mounted by running sleeve " + e.Name
				}
			}
		}
	}
	for _, scope := range m.cfg.scopeNames() {
		if scope == name {
			return "live scope " + name
		}
	}
	return ""
}

func (m *Manager) pruneEntry(ctx context.Context, root *os.Root, parent, category, name string, opts PruneOptions) ([]DiskEntry, error) {
	rel, kind, selected := name, "workspace", opts.Workspaces
	if category == "sleeves" {
		rel, kind, selected = filepath.Join(name, "home"), "home", opts.Homes
	}
	path := filepath.Join(parent, rel)
	busy := m.diskBusyReason(path, name)
	if busy != "" && !opts.Running {
		return nil, nil
	}
	var out []DiskEntry
	add := func(rel, kind string, selected bool) error {
		p := filepath.Join(parent, rel)
		if _, err := root.Lstat(rel); errors.Is(err, os.ErrNotExist) {
			return nil
		}
		row := DiskEntry{Sleeve: name, Path: p, Kind: kind}
		tree, err := openRelativeDiskRoot(root, rel)
		if err != nil {
			row.Reason = err.Error()
			out = append(out, row)
			return nil
		}
		tree.Close()
		n, err := dirSize(ctx, p)
		if err != nil {
			row.Reason = fmt.Sprintf("cannot measure: %v", err)
			out = append(out, row)
			return ctx.Err()
		}
		row.Bytes = n
		switch {
		case busy != "":
			row.Reason = busy + "; running data is always kept"
		case !selected:
			row.Reason = "not selected"
		default:
			row.Eligible = true
			row.Reason = "regenerable cache"
			if kind == "home" {
				row.Reason = "whole home, including auth, sessions, history and config"
			}
			if kind == "workspace" {
				if err := verifyPruneWorkspace(ctx, p, m.cfg.workspacesDir()); err != nil {
					row.Eligible, row.Reason = false, err.Error()
				} else {
					row.Reason = "clean; all branch tips and HEAD verified outside clone"
				}
			}
			if row.Eligible && opts.Yes {
				if err := removeDiskTree(ctx, root, rel); err != nil {
					return fmt.Errorf("prune %s: %w", p, err)
				}
				row.Removed = true
			}
		}
		out = append(out, row)
		return nil
	}
	if category == "sleeves" && !opts.Homes {
		for _, cache := range cacheDirs {
			if err := add(filepath.Join(rel, cache), "cache", opts.Caches); err != nil {
				return out, err
			}
		}
	}
	if err := add(rel, kind, selected); err != nil {
		return out, err
	}
	if category == "sleeves" && opts.Yes {
		m.mu.Lock()
		if e := m.sleeves[name]; e != nil {
			e.Usage.DiskSampled = time.Time{}
		}
		m.mu.Unlock()
	}
	return out, nil
}

func openRelativeDiskRoot(root *os.Root, path string) (*os.Root, error) {
	part := ""
	for _, component := range strings.Split(filepath.ToSlash(path), "/") {
		part = filepath.Join(part, component)
		fi, err := root.Lstat(part)
		if err != nil {
			return nil, err
		}
		if !fi.IsDir() || fi.Mode()&os.ModeSymlink != 0 {
			return nil, fmt.Errorf("keep non-directory or symlink %s", part)
		}
	}
	return root.OpenRoot(path)
}

func removeDiskTree(ctx context.Context, root *os.Root, path string) error {
	tree, err := openRelativeDiskRoot(root, path)
	if err != nil {
		return err
	}
	defer tree.Close()
	// Downloaded Go module directories are read-only. Root confines chmod and
	// removal even if a path is replaced; directory symlinks are never followed.
	if err := fs.WalkDir(tree.FS(), ".", func(p string, d fs.DirEntry, err error) error {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if err != nil {
			return err
		}
		if d.IsDir() {
			return tree.Chmod(p, 0700)
		}
		return nil
	}); err != nil {
		return err
	}
	return root.RemoveAll(path)
}

func (m *Manager) DropCaches(ctx context.Context, name string) error {
	if err := ValidName(name); err != nil {
		return err
	}
	if err := m.reserve(ctx, name); err != nil {
		return err
	}
	defer m.release(name)
	root, err := openDiskRoot(filepath.Join(m.cfg.Home, "sleeves"))
	if errors.Is(err, os.ErrNotExist) {
		return nil
	}
	if err != nil {
		return fmt.Errorf("open sleeve homes: %w", err)
	}
	defer root.Close()
	_, err = m.pruneEntry(ctx, root, filepath.Join(m.cfg.Home, "sleeves"), "sleeves", name, PruneOptions{Yes: true, Caches: true})
	return err
}
