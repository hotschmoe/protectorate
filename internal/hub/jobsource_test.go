package hub

import (
	"context"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func newRepo(t *testing.T) string {
	t.Helper()
	repo := t.TempDir()
	gitIn(t, repo, "init", "-q")
	return repo
}

func commitFile(t *testing.T, repo, name string, data []byte) string {
	t.Helper()
	writeFile(t, filepath.Join(repo, name), data, 0o644)
	gitIn(t, repo, "add", ".")
	gitIn(t, repo, "commit", "-qm", "add "+name)
	return gitIn(t, repo, "rev-parse", "HEAD")
}

func bundleBytes(t *testing.T, repo, commit string, prereqs []string) []byte {
	t.Helper()
	r, finish, err := openBundle(context.Background(), repo, commit, prereqs)
	if err != nil {
		t.Fatal(err)
	}
	b, err := io.ReadAll(r)
	if err != nil {
		t.Fatal(err)
	}
	if err := finish(); err != nil {
		t.Fatal(err)
	}
	return b
}

func TestBundleIncremental(t *testing.T) {
	hubRepo := newRepo(t)
	c1 := commitFile(t, hubRepo, "a.bin", randomBytes(t, 300<<10))
	commitFile(t, hubRepo, "b.bin", randomBytes(t, 300<<10))
	c3 := commitFile(t, hubRepo, "c.txt", []byte("small change\n"))
	ctx := context.Background()

	nodeRepo := newRepo(t)
	fetch := func(data []byte, commit string) {
		t.Helper()
		path := filepath.Join(t.TempDir(), "x.bundle")
		writeFile(t, path, data, 0o644)
		gitIn(t, nodeRepo, "bundle", "verify", path)
		ref := "refs/pc/" + commit
		gitIn(t, nodeRepo, "fetch", "-q", path, "+"+ref+":"+ref)
		if got := gitIn(t, nodeRepo, "rev-parse", ref); got != commit {
			t.Fatalf("fetched %s, want %s", got, commit)
		}
	}

	full1 := bundleBytes(t, hubRepo, c1, nil)
	fetch(full1, c1)
	if len(full1) < 300<<10 {
		t.Fatalf("full bundle of c1 is %d bytes", len(full1))
	}

	full3 := bundleBytes(t, hubRepo, c3, nil)
	prereqs := usablePrerequisites(ctx, hubRepo, c3, []string{c1})
	if len(prereqs) != 1 || prereqs[0] != c1 {
		t.Fatalf("prereqs = %v", prereqs)
	}
	inc3 := bundleBytes(t, hubRepo, c3, prereqs)
	if len(inc3) >= len(full3) || len(inc3) < 300<<10 {
		t.Fatalf("incremental bundle %d bytes vs full %d: want only c2 and c3 objects", len(inc3), len(full3))
	}
	fetch(inc3, c3)

	next := commitFile(t, hubRepo, "d.txt", []byte("tiny\n"))
	tiny := bundleBytes(t, hubRepo, next, usablePrerequisites(ctx, hubRepo, next, []string{c3, c1}))
	if len(tiny) > 4<<10 {
		t.Fatalf("one-file bundle is %d bytes", len(tiny))
	}
	fetch(tiny, next)
	gitIn(t, nodeRepo, "checkout", "-q", "--detach", next)
	if got, err := os.ReadFile(filepath.Join(nodeRepo, "d.txt")); err != nil || string(got) != "tiny\n" {
		t.Fatalf("checked-out file = %q, %v", got, err)
	}
}

func TestUsablePrerequisites(t *testing.T) {
	repo := newRepo(t)
	var commits []string
	for i := range 12 {
		commits = append(commits, commitFile(t, repo, fmt.Sprintf("f%d", i), []byte{byte(i)}))
	}
	head := commits[len(commits)-1]
	ctx := context.Background()
	unknown := strings.Repeat("ab", 20)

	got := usablePrerequisites(ctx, repo, head, []string{unknown, head, "short", commits[2], commits[2]})
	if len(got) != 1 || got[0] != commits[2] {
		t.Fatalf("filtering = %v", got)
	}
	got = usablePrerequisites(ctx, repo, head, commits[:11])
	if len(got) != maxPrerequisites {
		t.Fatalf("cap = %d", len(got))
	}
	if got := usablePrerequisites(ctx, repo, head, nil); len(got) != 0 {
		t.Fatalf("no known commits = %v", got)
	}
}

func TestResolveRepoCommit(t *testing.T) {
	repo := newRepo(t)
	c1 := commitFile(t, repo, "a", []byte("1"))
	gitIn(t, repo, "branch", "feature")
	gitIn(t, repo, "tag", "v1")
	c2 := commitFile(t, repo, "b", []byte("2"))
	ctx := context.Background()

	for rev, want := range map[string]string{"": c2, "HEAD": c2, "feature": c1, "v1": c1, c1[:10]: c1, "HEAD~1": c1} {
		abs, got, err := resolveRepoCommit(ctx, repo, rev)
		if err != nil || got != want || abs != repo {
			t.Errorf("rev %q = %s, %v; want %s", rev, got, err, want)
		}
	}
	for _, rev := range []string{"nope", "--output=/tmp/x", "-h", "HEAD:a"} {
		if _, _, err := resolveRepoCommit(ctx, repo, rev); err == nil {
			t.Errorf("rev %q must be rejected", rev)
		}
	}
	if _, _, err := resolveRepoCommit(ctx, t.TempDir(), ""); err == nil {
		t.Error("a plain directory is not a repo")
	}
	if _, _, err := resolveRepoCommit(ctx, filepath.Join(repo, "missing"), ""); err == nil {
		t.Error("a missing directory is not a repo")
	}
}

func TestRepoKey(t *testing.T) {
	a, b := repoKey("/home/x/My Repo"), repoKey("/srv/My Repo")
	if a == b || !strings.HasPrefix(a, "my-repo-") || a != repoKey("/home/x/My Repo") {
		t.Fatalf("keys = %q %q", a, b)
	}
	if k := repoKey("/"); !strings.HasPrefix(k, "repo-") {
		t.Fatalf("root key = %q", k)
	}
	if k := repoKey("/" + strings.Repeat("x", 100)); len(k) > 50 {
		t.Fatalf("long key = %q", k)
	}
}
