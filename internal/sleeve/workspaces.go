package sleeve

import (
	"bufio"
	"net/url"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

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
	gd := filepath.Join(dir, ".git")
	if fi, err := os.Stat(gd); err != nil || !fi.IsDir() {
		return "", ""
	}
	if b, err := os.ReadFile(filepath.Join(gd, "HEAD")); err == nil {
		if ref, ok := strings.CutPrefix(strings.TrimSpace(string(b)), "ref: refs/heads/"); ok {
			branch = ref
		}
	}
	f, err := os.Open(filepath.Join(gd, "config"))
	if err != nil {
		return "", branch
	}
	defer f.Close()
	in := false
	sc := bufio.NewScanner(f)
	for sc.Scan() {
		line := strings.TrimSpace(sc.Text())
		if strings.HasPrefix(line, "[") {
			in = strings.EqualFold(strings.Join(strings.Fields(line), " "), `[remote "origin"]`)
			continue
		}
		if k, v, ok := strings.Cut(line, "="); ok && in && strings.EqualFold(strings.TrimSpace(k), "url") {
			return stripCredentials(strings.TrimSpace(v)), branch
		}
	}
	return "", branch
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
