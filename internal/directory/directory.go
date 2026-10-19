// Package directory knows which projects the hub's sleeves work on and how
// they depend on each other. It reads only local files and git metadata: the
// origin remote, branch and HEAD of a workspace and the names in its package
// manifests (go.mod, package.json, Cargo.toml, pyproject.toml). A project is
// identified by its normalized git remote, or by its path without one; project
// A depends on project B when one of A's dependencies is a package B provides.
package directory

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strings"
	"sync"

	"github.com/hotschmoe/protectorate/internal/fsutil"
)

var (
	ErrNotFound  = errors.New("no such project")
	ErrAmbiguous = errors.New("ambiguous project")
)

// Owner is a sleeve working on a project, with the state of its checkout.
type Owner struct {
	Sleeve string
	Path   string
	Branch string
	Head   string
}

// RefState is the last release state seen for a project: its default branch
// tip and its tags. It is what dependency notices are computed against.
type RefState struct {
	Default    string            `json:"default,omitempty"`
	DefaultTip string            `json:"default_tip,omitempty"`
	Tags       map[string]string `json:"tags,omitempty"`
	LatestTag  string            `json:"latest_tag,omitempty"`
}

// Project is one repository or directory the sleeves work in. Provides and
// Deps come from the manifests; Deps leaves out what the project provides
// itself. Owners are the live sleeves and are not persisted.
type Project struct {
	ID       string    `json:"id"`
	Name     string    `json:"name"`
	Display  string    `json:"display"`
	Remote   string    `json:"remote,omitempty"`
	Provides []string  `json:"provides,omitempty"`
	Deps     []string  `json:"deps,omitempty"`
	Refs     *RefState `json:"refs,omitempty"`
	Owners   []Owner   `json:"-"`
}

// Edge points at another project and names the packages that link them.
type Edge struct {
	Project string
	Via     []string
}

// View is a project with its edges resolved against the whole directory.
type View struct {
	Project
	DependsOn  []Edge
	Dependents []Edge
}

// Scan is one sleeve's inspected workspace.
type Scan struct {
	Sleeve string
	Workspace
}

// ProjectID names the project a workspace belongs to.
func ProjectID(w Workspace) string {
	if w.Remote != "" {
		return w.Remote
	}
	return "path:" + filepath.Clean(w.Path)
}

func names(id, remote string) (name, display string) {
	p := remote
	if remote == "" {
		p = strings.TrimPrefix(id, "path:")
		name = filepath.Base(p)
		return name, name
	}
	segs := strings.Split(p, "/")
	name = segs[len(segs)-1]
	if len(segs) >= 3 {
		return name, path.Join(segs[len(segs)-2], name)
	}
	return name, p
}

// Directory holds the projects seen so far. Projects stay after their last
// sleeve is gone, with no owners, so messages and notices can still address
// them.
type Directory struct {
	file string

	mu       sync.Mutex
	projects map[string]*Project
	saved    []byte
}

type diskFormat struct {
	Projects []*Project `json:"projects"`
}

// Open loads the directory kept in file, which may not exist yet.
func Open(file string) (*Directory, error) {
	d := &Directory{file: file, projects: map[string]*Project{}}
	raw, err := os.ReadFile(file)
	if errors.Is(err, os.ErrNotExist) {
		return d, nil
	}
	if err != nil {
		return nil, fmt.Errorf("read directory: %w", err)
	}
	var f diskFormat
	if err := json.Unmarshal(raw, &f); err != nil {
		return nil, fmt.Errorf("parse %s: %w", file, err)
	}
	for _, p := range f.Projects {
		d.projects[p.ID] = p
	}
	d.saved = raw
	return d, nil
}

// Update replaces every project's owners with those in scans and refreshes
// their package names. Projects with no scan keep their last known data and
// lose their owners.
func (d *Directory) Update(scans []Scan) error {
	groups := map[string][]Scan{}
	for _, s := range scans {
		id := ProjectID(s.Workspace)
		groups[id] = append(groups[id], s)
	}
	d.mu.Lock()
	defer d.mu.Unlock()
	for _, p := range d.projects {
		p.Owners = nil
	}
	for id, g := range groups {
		sort.Slice(g, func(i, j int) bool { return g[i].Sleeve < g[j].Sleeve })
		p := d.projects[id]
		if p == nil {
			p = &Project{ID: id, Remote: g[0].Remote}
			p.Name, p.Display = names(id, g[0].Remote)
			d.projects[id] = p
		}
		provides, deps := map[string]bool{}, map[string]bool{}
		for _, s := range g {
			p.Owners = append(p.Owners, Owner{Sleeve: s.Sleeve, Path: s.Path, Branch: s.Branch, Head: s.Head})
			for _, m := range s.Manifests {
				if m.Name != "" {
					provides[m.Name] = true
				}
				for _, dep := range m.Deps {
					deps[dep] = true
				}
			}
		}
		for n := range provides {
			delete(deps, n)
		}
		p.Provides, p.Deps = sortedKeys(provides), sortedKeys(deps)
	}
	return d.saveLocked()
}

func sortedKeys(m map[string]bool) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	sort.Strings(out)
	return out
}

func (d *Directory) saveLocked() error {
	list := make([]*Project, 0, len(d.projects))
	for _, p := range d.projects {
		list = append(list, p)
	}
	sort.Slice(list, func(i, j int) bool { return list[i].ID < list[j].ID })
	raw, err := json.MarshalIndent(diskFormat{Projects: list}, "", " ")
	if err != nil {
		return fmt.Errorf("encode directory: %w", err)
	}
	if bytes.Equal(raw, d.saved) {
		return nil
	}
	if err := os.MkdirAll(filepath.Dir(d.file), 0o700); err != nil {
		return fmt.Errorf("create %s: %w", filepath.Dir(d.file), err)
	}
	if err := fsutil.WriteFile(d.file, raw, 0o600); err != nil {
		return fmt.Errorf("write directory: %w", err)
	}
	d.saved = raw
	return nil
}

// SetRefs records the release state of a project.
func (d *Directory) SetRefs(id string, r RefState) error {
	d.mu.Lock()
	defer d.mu.Unlock()
	p := d.projects[id]
	if p == nil {
		return fmt.Errorf("%w: %s", ErrNotFound, id)
	}
	p.Refs = &r
	return d.saveLocked()
}

func (d *Directory) viewLocked(p *Project) View {
	v := View{Project: *p}
	v.Owners = append([]Owner(nil), p.Owners...)
	for _, q := range d.projects {
		if q.ID == p.ID {
			continue
		}
		if via := intersect(p.Deps, q.Provides); len(via) > 0 {
			v.DependsOn = append(v.DependsOn, Edge{Project: q.ID, Via: via})
		}
		if via := intersect(q.Deps, p.Provides); len(via) > 0 {
			v.Dependents = append(v.Dependents, Edge{Project: q.ID, Via: via})
		}
	}
	sort.Slice(v.DependsOn, func(i, j int) bool { return v.DependsOn[i].Project < v.DependsOn[j].Project })
	sort.Slice(v.Dependents, func(i, j int) bool { return v.Dependents[i].Project < v.Dependents[j].Project })
	return v
}

// intersect returns the names in both sorted lists.
func intersect(a, b []string) []string {
	var out []string
	i, j := 0, 0
	for i < len(a) && j < len(b) {
		switch {
		case a[i] == b[j]:
			out = append(out, a[i])
			i++
			j++
		case a[i] < b[j]:
			i++
		default:
			j++
		}
	}
	return out
}

// List returns every project with its edges, ordered by id.
func (d *Directory) List() []View {
	d.mu.Lock()
	defer d.mu.Unlock()
	out := make([]View, 0, len(d.projects))
	for _, p := range d.projects {
		out = append(out, d.viewLocked(p))
	}
	sort.Slice(out, func(i, j int) bool { return out[i].ID < out[j].ID })
	return out
}

// Get resolves a project by id, then by "owner/repo", then by bare name,
// ignoring case. A name shared by several projects is ambiguous.
func (d *Directory) Get(query string) (View, error) {
	d.mu.Lock()
	defer d.mu.Unlock()
	q := strings.ToLower(strings.TrimSpace(query))
	if p := d.projects[q]; p != nil {
		return d.viewLocked(p), nil
	}
	if p := d.projects[strings.TrimSpace(query)]; p != nil {
		return d.viewLocked(p), nil
	}
	for _, field := range []func(*Project) string{
		func(p *Project) string { return p.Display },
		func(p *Project) string { return p.Name },
	} {
		var hits []*Project
		for _, p := range d.projects {
			if strings.EqualFold(field(p), query) {
				hits = append(hits, p)
			}
		}
		switch len(hits) {
		case 0:
		case 1:
			return d.viewLocked(hits[0]), nil
		default:
			ids := make([]string, len(hits))
			for i, p := range hits {
				ids[i] = p.ID
			}
			sort.Strings(ids)
			return View{}, fmt.Errorf("%w %q: matches %s", ErrAmbiguous, query, strings.Join(ids, ", "))
		}
	}
	return View{}, fmt.Errorf("%w: %q (see `protectorate who`)", ErrNotFound, query)
}

// ProjectsOf returns the ids of the projects a sleeve currently works on.
func (d *Directory) ProjectsOf(sleeve string) []string {
	d.mu.Lock()
	defer d.mu.Unlock()
	var out []string
	for _, p := range d.projects {
		for _, o := range p.Owners {
			if o.Sleeve == sleeve {
				out = append(out, p.ID)
				break
			}
		}
	}
	sort.Strings(out)
	return out
}
