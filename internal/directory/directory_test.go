package directory

import (
	"context"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func TestParseManifests(t *testing.T) {
	tests := []struct {
		name  string
		parse func([]byte) (Manifest, error)
		src   string
		want  Manifest
		fails bool
	}{
		{
			name: "go.mod block and single requires", parse: parseGoMod,
			src: `module "github.com/acme/api" // the api

go 1.22

require github.com/acme/lib v1.2.3

require (
	github.com/acme/core v0.4.0 // indirect
	golang.org/x/sys v0.1.0
)

replace github.com/acme/core => ../core
`,
			want: Manifest{Kind: Go, Name: "go:github.com/acme/api", Deps: []string{"go:github.com/acme/core", "go:github.com/acme/lib", "go:golang.org/x/sys"}},
		},
		{name: "go.mod without module", parse: parseGoMod, src: "go 1.22\n", fails: true},
		{
			name: "package.json sections", parse: parsePackageJSON,
			src: `{"name":"@acme/web","version":"1.0.0","dependencies":{"@acme/ui":"^1","react":"18"},
"devDependencies":{"vite":"5"},"peerDependencies":{"@acme/theme":"*"},"scripts":{"x":"y"}}`,
			want: Manifest{Kind: NPM, Name: "npm:@acme/web", Deps: []string{"npm:@acme/theme", "npm:@acme/ui", "npm:react", "npm:vite"}},
		},
		{name: "package.json without a name", parse: parsePackageJSON, src: `{"dependencies":{"a":"1"}}`, want: Manifest{Kind: NPM, Deps: []string{"npm:a"}}},
		{name: "package.json malformed", parse: parsePackageJSON, src: `{`, fails: true},
		{
			name: "Cargo.toml tables, renames and subtables", parse: parseCargo,
			src: `[package]
name = "acme_server"
version = "0.1.0"

[dependencies]
serde = { version = "1", features = ["derive"] } # comment
acme-core = { path = "../core" }
tokio = "1"
renamed = { package = "Real_Name", version = "2" }

[dev-dependencies.pretty_assertions]
version = "1"

[target.'cfg(unix)'.dependencies]
libc = "0.2"

[build-dependencies]
cc = "1"
`,
			want: Manifest{Kind: Cargo, Name: "cargo:acme-server", Deps: []string{"cargo:acme-core", "cargo:cc", "cargo:libc", "cargo:pretty-assertions", "cargo:real-name", "cargo:serde", "cargo:tokio"}},
		},
		{
			name: "Cargo.toml workspace only", parse: parseCargo,
			src:  "[workspace]\nmembers = [\"a\", \"b\"]\n\n[workspace.dependencies]\nanyhow = \"1\"\n",
			want: Manifest{Kind: Cargo, Deps: []string{"cargo:anyhow"}},
		},
		{name: "Cargo.toml empty", parse: parseCargo, src: "[profile.release]\nlto = true\n", fails: true},
		{
			name: "pyproject PEP 621", parse: parsePyproject,
			src: `[build-system]
requires = ["setuptools>=61"]

[project]
name = "Acme_Tools"
dependencies = [
  "requests>=2.0",   # http
  "acme.core[extra] ; python_version > '3.8'",
  'numpy',
]

[project.optional-dependencies]
dev = ["pytest", "acme-lint==1.0"]
`,
			want: Manifest{Kind: PyPI, Name: "pypi:acme-tools", Deps: []string{"pypi:acme-core", "pypi:acme-lint", "pypi:numpy", "pypi:pytest", "pypi:requests"}},
		},
		{
			name: "pyproject poetry", parse: parsePyproject,
			src: `[tool.poetry]
name = "acme-web"

[tool.poetry.dependencies]
python = "^3.11"
Django = "^5"
acme_core = { path = "../core" }

[tool.poetry.group.dev.dependencies]
black = "*"
`,
			want: Manifest{Kind: PyPI, Name: "pypi:acme-web", Deps: []string{"pypi:acme-core", "pypi:black", "pypi:django"}},
		},
		{name: "pyproject with neither", parse: parsePyproject, src: "[tool.black]\nline-length = 100\n", fails: true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := tt.parse([]byte(tt.src))
			if tt.fails {
				if err == nil {
					t.Fatalf("parsed %+v, want an error", got)
				}
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			if !reflect.DeepEqual(got, tt.want) {
				t.Fatalf("got  %+v\nwant %+v", got, tt.want)
			}
		})
	}
}

func TestParseTOMLValues(t *testing.T) {
	doc := parseTOML("a = \"x # not a comment\" # real\nb = [\n 'p',\n \"q\", # c\n]\nc = \"\"\"\nline [ with bracket\n\"\"\"\nd = { k = \"v\", n = { z = 1 } }\n[t.u]\ne.f = 1\n")
	if got, _ := tomlString(doc[""]["a"]); got != "x # not a comment" {
		t.Errorf("a = %q", got)
	}
	if got := tomlStrings(doc[""]["b"]); !reflect.DeepEqual(got, []string{"p", "q"}) {
		t.Errorf("b = %q", got)
	}
	if got, _ := tomlString(doc[""]["c"]); got != "line [ with bracket\n" {
		t.Errorf("c = %q", got)
	}
	if got := tomlInline(doc[""]["d"]); len(got) != 2 || got["k"] != `"v"` {
		t.Errorf("d = %v", got)
	}
	if _, ok := doc[tablePath("t", "u", "e")]["f"]; !ok {
		t.Errorf("dotted key not filed under t.u.e: %v", doc)
	}
}

func TestNormalizeRemote(t *testing.T) {
	tests := []struct{ in, want string }{
		{"git@github.com:HotSchmoe/Protectorate.git", "github.com/hotschmoe/protectorate"},
		{"https://github.com/hotschmoe/protectorate.git", "github.com/hotschmoe/protectorate"},
		{"https://user:pw@github.com/hotschmoe/protectorate/", "github.com/hotschmoe/protectorate"},
		{"ssh://git@example.com:2222/team/repo.git", "example.com/team/repo"},
		{"git://host.example/a/b", "host.example/a/b"},
		{"/srv/git/repo.git", "/srv/git/repo"},
		{"file:///srv/git/repo.git", "/srv/git/repo"},
		{"  ", ""},
	}
	for _, tt := range tests {
		t.Run(tt.in, func(t *testing.T) {
			if got := NormalizeRemote(tt.in); got != tt.want {
				t.Fatalf("NormalizeRemote(%q) = %q, want %q", tt.in, got, tt.want)
			}
		})
	}
}

func write(t *testing.T, root, rel, content string) {
	t.Helper()
	p := filepath.Join(root, rel)
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestReadManifestsLayout(t *testing.T) {
	root := t.TempDir()
	write(t, root, "go.mod", "module example.com/mono\n")
	write(t, root, "web/package.json", `{"name":"web"}`)
	write(t, root, "packages/ui/package.json", `{"name":"ui"}`)
	write(t, root, "a/b/c/package.json", `{"name":"too-deep"}`)
	write(t, root, "node_modules/x/package.json", `{"name":"vendored"}`)
	write(t, root, ".hidden/package.json", `{"name":"hidden"}`)
	write(t, root, "svc/Cargo.toml", "[package]\nname = \"svc\"\n")
	write(t, root, "bad/package.json", "{")
	var got []string
	for _, m := range ReadManifests(root) {
		got = append(got, m.Path+"="+m.Name)
	}
	want := []string{"go.mod=go:example.com/mono", "packages/ui/package.json=npm:ui", "svc/Cargo.toml=cargo:svc", "web/package.json=npm:web"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("manifests %q, want %q", got, want)
	}
}

func mkScan(sleeve, path, remote string, manifests ...Manifest) Scan {
	return Scan{Sleeve: sleeve, Workspace: Workspace{Path: path, Remote: remote, Manifests: manifests}}
}

func TestEdgesAndLookup(t *testing.T) {
	lib := Manifest{Kind: Go, Name: "go:github.com/acme/lib"}
	api := Manifest{Kind: Go, Name: "go:github.com/acme/api", Deps: []string{"go:github.com/acme/lib", "go:golang.org/x/sys"}}
	cli := Manifest{Kind: Go, Name: "go:github.com/acme/cli", Deps: []string{"go:github.com/acme/api", "go:github.com/acme/lib"}}
	selfRef := Manifest{Kind: NPM, Name: "npm:inner", Deps: []string{"npm:inner"}}
	web := Manifest{Kind: NPM, Name: "npm:web", Deps: []string{"npm:inner", "npm:ui"}}
	d, err := Open(filepath.Join(t.TempDir(), "dir.json"))
	if err != nil {
		t.Fatal(err)
	}
	err = d.Update([]Scan{
		mkScan("libdev", "/w/lib", "github.com/acme/lib", lib),
		mkScan("apidev", "/w/api", "github.com/acme/api", api),
		mkScan("apidev2", "/w/api2", "github.com/acme/api", api),
		mkScan("clidev", "/w/cli", "github.com/acme/cli", cli),
		mkScan("webdev", "/w/web", "", web, selfRef),
		mkScan("other", "/w/other", "gitlab.com/else/lib"),
	})
	if err != nil {
		t.Fatal(err)
	}

	tests := []struct {
		query      string
		wantID     string
		owners     []string
		dependsOn  []string
		dependents []string
	}{
		{"github.com/acme/lib", "github.com/acme/lib", []string{"libdev"}, nil, []string{"github.com/acme/api", "github.com/acme/cli"}},
		{"acme/api", "github.com/acme/api", []string{"apidev", "apidev2"}, []string{"github.com/acme/lib"}, []string{"github.com/acme/cli"}},
		{"CLI", "github.com/acme/cli", []string{"clidev"}, []string{"github.com/acme/api", "github.com/acme/lib"}, nil},
		{"web", "path:/w/web", []string{"webdev"}, nil, nil},
		{"path:/w/web", "path:/w/web", []string{"webdev"}, nil, nil},
	}
	for _, tt := range tests {
		t.Run(tt.query, func(t *testing.T) {
			v, err := d.Get(tt.query)
			if err != nil {
				t.Fatal(err)
			}
			var owners, dep, dependents []string
			for _, o := range v.Owners {
				owners = append(owners, o.Sleeve)
			}
			for _, e := range v.DependsOn {
				dep = append(dep, e.Project)
			}
			for _, e := range v.Dependents {
				dependents = append(dependents, e.Project)
			}
			if v.ID != tt.wantID || !reflect.DeepEqual(owners, tt.owners) || !reflect.DeepEqual(dep, tt.dependsOn) || !reflect.DeepEqual(dependents, tt.dependents) {
				t.Fatalf("%s: owners %v dependsOn %v dependents %v", v.ID, owners, dep, dependents)
			}
		})
	}
	if v, _ := d.Get("api"); len(v.DependsOn) != 1 || !reflect.DeepEqual(v.DependsOn[0].Via, []string{"go:github.com/acme/lib"}) {
		t.Fatalf("edge via = %+v", v.DependsOn)
	}
	if v, _ := d.Get("web"); len(v.Deps) != 1 || v.Deps[0] != "npm:ui" {
		t.Fatalf("a project's own packages must not count as its dependencies: %v", v.Deps)
	}
	if got := d.ProjectsOf("apidev2"); !reflect.DeepEqual(got, []string{"github.com/acme/api"}) {
		t.Fatalf("ProjectsOf = %v", got)
	}

	if _, err := d.Get("lib"); !errors.Is(err, ErrAmbiguous) {
		t.Fatalf("lib matches two projects: %v", err)
	}
	if _, err := d.Get("nope"); !errors.Is(err, ErrNotFound) {
		t.Fatalf("unknown project: %v", err)
	}
}

func TestDirectoryPersistsAndKeepsAbsentProjects(t *testing.T) {
	file := filepath.Join(t.TempDir(), "dir.json")
	d, _ := Open(file)
	lib := Manifest{Kind: Go, Name: "go:example.com/lib"}
	app := Manifest{Kind: Go, Name: "go:example.com/app", Deps: []string{"go:example.com/lib"}}
	if err := d.Update([]Scan{mkScan("a", "/w/lib", "example.com/lib", lib), mkScan("b", "/w/app", "example.com/app", app)}); err != nil {
		t.Fatal(err)
	}
	if err := d.SetRefs("example.com/lib", RefState{Default: "main", DefaultTip: "abc", Tags: []Tag{{Name: "v1", Sha: "abc", Unix: 1}}}); err != nil {
		t.Fatal(err)
	}
	if err := d.SetRefs("example.com/none", RefState{}); !errors.Is(err, ErrNotFound) {
		t.Fatalf("SetRefs on unknown project: %v", err)
	}
	if err := d.Update([]Scan{mkScan("b", "/w/app", "example.com/app", app)}); err != nil {
		t.Fatal(err)
	}

	re, err := Open(file)
	if err != nil {
		t.Fatal(err)
	}
	v, err := re.Get("lib")
	if err != nil {
		t.Fatal(err)
	}
	if len(v.Owners) != 0 || len(v.Dependents) != 1 || v.Refs == nil || v.Refs.DefaultTip != "abc" {
		t.Fatalf("reloaded lib = %+v", v)
	}
}

func git(t *testing.T, dir string, args ...string) string {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir, "-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main"}, args...)...)
	cmd.Env = append(os.Environ(), "GIT_CONFIG_GLOBAL=/dev/null", "GIT_CONFIG_NOSYSTEM=1")
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
	return strings.TrimSpace(string(out))
}

func TestInspect(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git not installed")
	}
	dir := t.TempDir()
	git(t, dir, "init", "-q")
	write(t, dir, "go.mod", "module github.com/acme/thing\n\nrequire github.com/acme/lib v1.0.0\n")
	git(t, dir, "add", ".")
	git(t, dir, "commit", "-q", "-m", "init")
	git(t, dir, "remote", "add", "origin", "git@github.com:Acme/Thing.git")

	w := Inspect(context.Background(), dir)
	if w.Remote != "github.com/acme/thing" || w.Branch != "main" || len(w.Head) != 40 {
		t.Fatalf("inspect = %+v", w)
	}
	if len(w.Manifests) != 1 || w.Manifests[0].Name != "go:github.com/acme/thing" {
		t.Fatalf("manifests = %+v", w.Manifests)
	}

	plain := t.TempDir()
	write(t, plain, "package.json", `{"name":"solo"}`)
	w = Inspect(context.Background(), plain)
	if w.Remote != "" || w.Head != "" || len(w.Manifests) != 1 || ProjectID(w) != "path:"+plain {
		t.Fatalf("non-git inspect = %+v", w)
	}
}
