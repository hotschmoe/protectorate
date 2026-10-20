package sleeve

import (
	"bufio"
	"bytes"
	"context"
	"fmt"
	"io"
	"net/url"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"syscall"
)

type Directory struct {
	Name, Path, Branch, Remote, Reason, RequiredName string
	IsGit, Acceptable                                bool
}

func WorkspaceBranch(dir string) string {
	_, branch := gitDetails(dir)
	return branch
}

func (m *Manager) Browse(ctx context.Context, path string) (dirs []Directory, resolved, parent string, truncated bool, err error) {
	inside := func(p string) bool {
		for _, root := range m.cfg.WorkspaceRoots {
			if within(p, root) {
				return true
			}
		}
		return false
	}
	entry := func(p, name string) Directory {
		d := Directory{Name: name, Path: p}
		if within(p, m.cfg.workspacesDir()) && p != m.cfg.workspacesDir() {
			rel, _ := filepath.Rel(m.cfg.workspacesDir(), p)
			d.RequiredName = strings.Split(rel, string(filepath.Separator))[0]
		}
		if e := m.cfg.CheckWorkspace(cmpName("browser", d.RequiredName), p); e != nil {
			d.Reason = e.Error()
		} else {
			d.Acceptable = true
		}
		if fi, e := os.Stat(filepath.Join(p, ".git")); e == nil {
			d.IsGit = fi.IsDir() || fi.Mode().IsRegular()
		}
		d.Remote, d.Branch = gitDetails(p)
		return d
	}
	if path == "" {
		for _, root := range m.cfg.WorkspaceRoots {
			if err = ctx.Err(); err != nil {
				return
			}
			real, e := filepath.EvalSymlinks(root)
			if e != nil || real != root {
				continue
			}
			if len(dirs) == 500 {
				truncated = true
				break
			}
			dirs = append(dirs, entry(root, filepath.Base(root)))
		}
		return
	}
	if !filepath.IsAbs(path) {
		err = fmt.Errorf("%w: browse path must be absolute", ErrInvalid)
		return
	}
	resolved, err = filepath.EvalSymlinks(path)
	if err != nil {
		err = fmt.Errorf("%w: resolve browse path: %v", ErrInvalid, err)
		return
	}
	if !inside(resolved) {
		err = fmt.Errorf("%w: browse path is outside workspace roots", ErrInvalid)
		return
	}
	parent = filepath.Dir(resolved)
	if !inside(parent) {
		parent = ""
	}
	for _, root := range m.cfg.WorkspaceRoots {
		if resolved == root {
			parent = ""
			break
		}
	}
	f, e := os.Open(resolved)
	if e != nil {
		err = fmt.Errorf("open browse directory: %w", e)
		return
	}
	defer f.Close()
	// Bound scanning as well as results, including directories with mostly files.
	for scanned := 0; scanned < 10000; scanned++ {
		if err = ctx.Err(); err != nil {
			return
		}
		ents, readErr := f.ReadDir(1)
		if len(ents) == 0 {
			if readErr != nil && readErr != io.EOF {
				err = fmt.Errorf("read browse directory: %w", readErr)
			}
			break
		}
		if scanned == 9999 {
			truncated = true
		}
		e := ents[0]
		if strings.HasPrefix(e.Name(), ".") {
			continue
		}
		p, eerr := filepath.EvalSymlinks(filepath.Join(resolved, e.Name()))
		if eerr != nil || !inside(p) {
			continue
		}
		fi, eerr := os.Stat(p)
		if eerr != nil || !fi.IsDir() {
			continue
		}
		if len(dirs) == 500 {
			truncated = true
			break
		}
		dirs = append(dirs, entry(p, e.Name()))
	}
	sort.Slice(dirs, func(i, j int) bool { return dirs[i].Name < dirs[j].Name })
	return
}

// Workspace is a directory Spawn would accept as an existing workspace.
type Workspace struct {
	ReadOnly bool
	Name     string
	Path     string
	Remote   string
	Branch   string
	// Sleeve names the sleeve that uses the directory, running or exited.
	Sleeve string
	// Busy is true while that sleeve is running.
	Busy bool
	// RequiredName is the only sleeve name that may use a directory in the
	// hub's state directory.
	RequiredName string
}

// Workspaces lists the directories under the hub's workspaces directory and
// the workspaces of known sleeves, keeping only those Spawn would accept.
// Git details are read from the files in .git, never by running git, so a
// repository an agent wrote to cannot execute anything on the hub.
func (m *Manager) Workspaces() []Workspace {
	byPath := map[string]*Workspace{}
	add := func(name, dir string) *Workspace {
		real, err := filepath.EvalSymlinks(dir)
		if err != nil {
			return nil
		}
		if fi, err := os.Stat(real); err != nil || !fi.IsDir() {
			return nil
		}
		w := byPath[real]
		if w != nil {
			return w
		}
		req := ""
		if within(real, m.cfg.Home) {
			req = filepath.Base(real)
		}
		if m.cfg.CheckWorkspace(cmpName(name, req), real) != nil {
			return nil
		}
		w = &Workspace{Name: filepath.Base(real), Path: real, RequiredName: req}
		w.Remote, w.Branch = gitDetails(real)
		byPath[real] = w
		return w
	}

	if ents, err := os.ReadDir(m.cfg.workspacesDir()); err == nil {
		for _, e := range ents {
			if strings.HasPrefix(e.Name(), ".") {
				continue
			}
			add(e.Name(), filepath.Join(m.cfg.workspacesDir(), e.Name()))
		}
	}
	for _, i := range m.List() {
		if i.Helper != "" || i.Name == OrchestratorName || i.Workspace == "" {
			continue
		}
		w := add(i.Name, i.Workspace)
		if w == nil {
			continue
		}
		if w.Sleeve == "" || i.State == Running {
			w.Sleeve = i.Name
			w.Busy = i.State == Running
		}
	}
	refs := append([]string(nil), m.cfg.ReferenceDirs...)
	for _, w := range byPath {
		refs = append(refs, filepath.Join(w.Path, "references"))
	}
	for _, p := range refs {
		mounts, err := m.cfg.ResolveMounts([]Mount{{Source: p}})
		if err != nil {
			continue
		}
		r := mounts[0].Source
		if byPath[r] == nil {
			byPath[r] = &Workspace{Name: filepath.Base(r), Path: r, ReadOnly: true}
		}
	}

	out := make([]Workspace, 0, len(byPath))
	for _, w := range byPath {
		out = append(out, *w)
	}
	sort.Slice(out, func(a, b int) bool {
		if out[a].Name != out[b].Name {
			return out[a].Name < out[b].Name
		}
		return out[a].Path < out[b].Path
	})
	return out
}

// cmpName is the sleeve name to check a directory against: the one it
// requires, else the name it was found under.
func cmpName(name, required string) string {
	if required != "" {
		return required
	}
	return name
}

// gitDetails reads the origin URL and the checked-out branch of a plain
// checkout. A worktree or submodule (.git as a file) reports neither.
func gitDetails(dir string) (remote, branch string) {
	root, err := os.OpenRoot(dir)
	if err != nil {
		return "", ""
	}
	defer root.Close()
	if fi, err := root.Stat(".git"); err != nil || !fi.IsDir() {
		return "", ""
	}
	if b, err := readGitMetadata(root, ".git/HEAD"); err == nil {
		if ref, ok := strings.CutPrefix(strings.TrimSpace(string(b)), "ref: refs/heads/"); ok {
			branch = ref
		}
	}
	config, err := readGitMetadata(root, ".git/config")
	if err != nil {
		return "", branch
	}
	return parseOrigin(config), branch
}

// gitOrigin reads the origin URL from a git directory with the same bounds
// as gitDetails.
func gitOrigin(gd string) string {
	root, err := os.OpenRoot(gd)
	if err != nil {
		return ""
	}
	defer root.Close()
	config, err := readGitMetadata(root, "config")
	if err != nil {
		return ""
	}
	return parseOrigin(config)
}

func parseOrigin(config []byte) string {
	in := false
	sc := bufio.NewScanner(bytes.NewReader(config))
	for sc.Scan() {
		line := strings.TrimSpace(sc.Text())
		if strings.HasPrefix(line, "[") {
			in = strings.EqualFold(strings.Join(strings.Fields(line), " "), `[remote "origin"]`)
			continue
		}
		if k, v, ok := strings.Cut(line, "="); ok && in && strings.EqualFold(strings.TrimSpace(k), "url") {
			return stripCredentials(strings.TrimSpace(v))
		}
	}
	return ""
}

func readGitMetadata(root *os.Root, name string) ([]byte, error) {
	f, err := root.OpenFile(name, os.O_RDONLY|syscall.O_NONBLOCK, 0)
	if err != nil {
		return nil, fmt.Errorf("open git metadata %s: %w", name, err)
	}
	defer f.Close()
	const maxBytes = 1 << 20
	fi, err := f.Stat()
	if err != nil {
		return nil, fmt.Errorf("stat git metadata %s: %w", name, err)
	}
	if !fi.Mode().IsRegular() || fi.Size() > maxBytes {
		return nil, fmt.Errorf("git metadata %s is not a bounded regular file", name)
	}
	b, err := io.ReadAll(io.LimitReader(f, maxBytes+1))
	if err != nil {
		return nil, fmt.Errorf("read git metadata %s: %w", name, err)
	}
	if len(b) > maxBytes {
		return nil, fmt.Errorf("git metadata %s exceeds %d bytes", name, maxBytes)
	}
	return b, nil
}

// stripCredentials drops the user info of an http(s) remote so a token
// embedded in it is never listed.
func stripCredentials(remote string) string {
	if !strings.Contains(remote, "://") {
		return remote
	}
	u, err := url.Parse(remote)
	if err != nil {
		return ""
	}
	if u.Scheme == "http" || u.Scheme == "https" {
		u.User = nil
	}
	return u.String()
}
