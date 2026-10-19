package directory

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// Ecosystems a manifest can belong to.
const (
	Go    = "go"
	NPM   = "npm"
	Cargo = "cargo"
	PyPI  = "pypi"
)

// Manifest is what one package file says about its package: the name others
// depend on it by, and the names it depends on. Names are normalized and
// prefixed with the ecosystem ("go:github.com/a/b", "npm:left-pad") so
// packages of different ecosystems never collide.
type Manifest struct {
	Kind string   `json:"kind"`
	Path string   `json:"path"` // relative to the workspace
	Name string   `json:"name,omitempty"`
	Deps []string `json:"deps,omitempty"`
}

const (
	maxManifestBytes = 1 << 20
	maxScanDirs      = 300
	maxScanDepth     = 2
)

var skipDirs = map[string]bool{
	"node_modules": true, "vendor": true, "target": true, "dist": true, "build": true,
	"testdata": true, "__pycache__": true, "venv": true,
}

var manifestFiles = []struct {
	file  string
	parse func(src []byte) (Manifest, error)
}{
	{"go.mod", parseGoMod},
	{"package.json", parsePackageJSON},
	{"Cargo.toml", parseCargo},
	{"pyproject.toml", parsePyproject},
}

// ReadManifests finds the package manifests in root and, up to two levels
// down, its subdirectories (monorepo layouts). Unreadable or malformed files
// are skipped; there is no network access and no package manager is run.
func ReadManifests(root string) []Manifest {
	var out []Manifest
	visited := 0
	var walk func(rel string, depth int)
	walk = func(rel string, depth int) {
		dir := filepath.Join(root, rel)
		entries, err := os.ReadDir(dir)
		if err != nil {
			return
		}
		visited++
		for _, mf := range manifestFiles {
			src, err := readSmall(filepath.Join(dir, mf.file))
			if err != nil {
				continue
			}
			m, err := mf.parse(src)
			if err != nil {
				continue
			}
			m.Path = filepath.ToSlash(filepath.Join(rel, mf.file))
			out = append(out, m)
		}
		if depth >= maxScanDepth {
			return
		}
		for _, e := range entries {
			n := e.Name()
			if !e.IsDir() || strings.HasPrefix(n, ".") || skipDirs[n] || visited >= maxScanDirs {
				continue
			}
			walk(filepath.Join(rel, n), depth+1)
		}
	}
	walk("", 0)
	return out
}

func readSmall(path string) ([]byte, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	fi, err := f.Stat()
	if err != nil || !fi.Mode().IsRegular() || fi.Size() > maxManifestBytes {
		return nil, fmt.Errorf("skip %s", path)
	}
	return os.ReadFile(path)
}

func finish(m Manifest) Manifest {
	seen := map[string]bool{}
	deps := m.Deps[:0:0]
	for _, d := range m.Deps {
		if d != "" && !seen[d] {
			seen[d] = true
			deps = append(deps, d)
		}
	}
	sort.Strings(deps)
	m.Deps = deps
	return m
}

func parseGoMod(src []byte) (Manifest, error) {
	m := Manifest{Kind: Go}
	inRequire := false
	for _, line := range strings.Split(string(src), "\n") {
		if i := strings.Index(line, "//"); i >= 0 {
			line = line[:i]
		}
		f := strings.Fields(line)
		switch {
		case len(f) == 0:
		case inRequire:
			if f[0] == ")" {
				inRequire = false
			} else {
				m.Deps = append(m.Deps, Go+":"+unquoteGo(f[0]))
			}
		case f[0] == "module" && len(f) >= 2:
			m.Name = Go + ":" + unquoteGo(f[1])
		case f[0] == "require" && len(f) >= 2 && f[1] == "(":
			inRequire = true
		case f[0] == "require" && len(f) >= 2:
			m.Deps = append(m.Deps, Go+":"+unquoteGo(f[1]))
		}
	}
	if m.Name == "" {
		return Manifest{}, fmt.Errorf("go.mod without a module line")
	}
	return finish(m), nil
}

func unquoteGo(s string) string { return strings.Trim(s, "\"`") }

func parsePackageJSON(src []byte) (Manifest, error) {
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(src, &raw); err != nil {
		return Manifest{}, fmt.Errorf("parse package.json: %w", err)
	}
	m := Manifest{Kind: NPM}
	var name string
	if json.Unmarshal(raw["name"], &name) == nil && name != "" {
		m.Name = NPM + ":" + name
	}
	for _, sec := range []string{"dependencies", "devDependencies", "peerDependencies", "optionalDependencies"} {
		var deps map[string]json.RawMessage
		if json.Unmarshal(raw[sec], &deps) != nil {
			continue
		}
		for n := range deps {
			m.Deps = append(m.Deps, NPM+":"+n)
		}
	}
	return finish(m), nil
}

func cargoName(s string) string {
	return Cargo + ":" + strings.ToLower(strings.ReplaceAll(s, "_", "-"))
}

func parseCargo(src []byte) (Manifest, error) {
	doc := parseTOML(string(src))
	m := Manifest{Kind: Cargo}
	if raw, ok := doc[tablePath("package")]["name"]; ok {
		if n, ok := tomlString(raw); ok {
			m.Name = cargoName(n)
		}
	}
	for _, path := range doc.tables() {
		i := depTableAt(path, "dependencies", "dev-dependencies", "build-dependencies")
		if i < 0 {
			continue
		}
		tbl := doc[tablePath(path...)]
		if i == len(path)-1 {
			for k, raw := range tbl {
				name := k
				if p, ok := tomlString(tomlInline(raw)["package"]); ok {
					name = p
				}
				m.Deps = append(m.Deps, cargoName(name))
			}
			continue
		}
		name := path[i+1]
		if p, ok := tomlString(tbl["package"]); ok {
			name = p
		}
		m.Deps = append(m.Deps, cargoName(name))
	}
	if m.Name == "" && len(m.Deps) == 0 {
		return Manifest{}, fmt.Errorf("Cargo.toml without a package or dependencies")
	}
	return finish(m), nil
}

// depTableAt returns the index in path of the first segment named one of
// names when path is a dependency table or a table below one, else -1. It
// matches [dependencies], [target.<cfg>.dependencies], [workspace.dependencies]
// and [dependencies.<name>].
func depTableAt(path []string, names ...string) int {
	for i, seg := range path {
		for _, n := range names {
			if seg != n {
				continue
			}
			if i == 0 || path[0] == "target" || path[0] == "workspace" {
				return i
			}
		}
	}
	return -1
}

var pyReq = regexp.MustCompile(`^\s*([A-Za-z0-9][A-Za-z0-9._-]*)`)

var pyNorm = regexp.MustCompile(`[-_.]+`)

func pyName(s string) string { return PyPI + ":" + pyNorm.ReplaceAllString(strings.ToLower(s), "-") }

func parsePyproject(src []byte) (Manifest, error) {
	doc := parseTOML(string(src))
	m := Manifest{Kind: PyPI}
	for _, tbl := range []string{tablePath("project"), tablePath("tool", "poetry")} {
		if raw, ok := doc[tbl]["name"]; ok && m.Name == "" {
			if n, ok := tomlString(raw); ok {
				m.Name = pyName(n)
			}
		}
	}
	addReqs := func(raw string) {
		for _, r := range tomlStrings(raw) {
			if g := pyReq.FindStringSubmatch(r); g != nil {
				m.Deps = append(m.Deps, pyName(g[1]))
			}
		}
	}
	addReqs(doc[tablePath("project")]["dependencies"])
	for _, t := range doc.tables() {
		switch {
		case len(t) == 2 && t[0] == "project" && t[1] == "optional-dependencies",
			len(t) == 1 && t[0] == "dependency-groups":
			for _, raw := range doc[tablePath(t...)] {
				addReqs(raw)
			}
		case len(t) >= 3 && t[0] == "tool" && t[1] == "poetry" && (t[len(t)-1] == "dependencies" || t[len(t)-1] == "dev-dependencies"):
			for k := range doc[tablePath(t...)] {
				if k != "python" {
					m.Deps = append(m.Deps, pyName(k))
				}
			}
		}
	}
	if m.Name == "" && len(m.Deps) == 0 {
		return Manifest{}, fmt.Errorf("pyproject.toml without a project or dependencies")
	}
	return finish(m), nil
}
