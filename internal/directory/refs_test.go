package directory

import (
	"context"
	"os/exec"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func TestChanges(t *testing.T) {
	always := func(_, _ string) bool { return true }
	never := func(_, _ string) bool { return false }
	tag := func(name, sha string, unix int64) Tag { return Tag{Name: name, Sha: sha, Unix: unix} }
	base := RefState{Default: "main", DefaultTip: "c1", Tags: []Tag{tag("v1.0.0", "c1", 1)}}

	tests := []struct {
		name string
		old  RefState
		now  RefState
		anc  func(a, b string) bool
		want []Change
	}{
		{"nothing new", base, base, always, nil},
		{
			"new tag at the new tip covers the branch advance",
			base, RefState{Default: "main", DefaultTip: "c2", Tags: []Tag{tag("v1.0.0", "c1", 1), tag("v1.1.0", "c2", 2)}}, always,
			[]Change{{Kind: "tag", Name: "v1.1.0", Tag: "v1.1.0", From: "c1", FromName: "v1.0.0", To: "c2"}},
		},
		{
			"branch advance without a tag",
			base, RefState{Default: "main", DefaultTip: "c3", Tags: base.Tags}, always,
			[]Change{{Kind: "branch", Name: "main", From: "c1", To: "c3"}},
		},
		{
			"new tag behind the tip, branch advanced too",
			base, RefState{Default: "main", DefaultTip: "c5", Tags: []Tag{tag("v1.0.0", "c1", 1), tag("v1.0.1", "c2", 2)}}, always,
			[]Change{{Kind: "tag", Name: "v1.0.1", Tag: "v1.0.1", From: "c1", FromName: "v1.0.0", To: "c2"}, {Kind: "branch", Name: "main", From: "c1", To: "c5"}},
		},
		{"rewound or rewritten branch is not announced", base, RefState{Default: "main", DefaultTip: "c9", Tags: base.Tags}, never, nil},
		{"default branch renamed", base, RefState{Default: "trunk", DefaultTip: "c9", Tags: base.Tags}, always, nil},
		{
			"first tag of a repo compares with the old tip",
			RefState{Default: "main", DefaultTip: "c1"}, RefState{Default: "main", DefaultTip: "c1", Tags: []Tag{tag("v0.1.0", "c1", 1)}}, always,
			[]Change{{Kind: "tag", Name: "v0.1.0", Tag: "v0.1.0", From: "c1", To: "c1"}},
		},
		{
			"a burst of tags is bounded to the latest three",
			RefState{Default: "main", DefaultTip: "c0"},
			RefState{Default: "main", DefaultTip: "c0", Tags: []Tag{tag("a", "1", 1), tag("b", "2", 2), tag("c", "3", 3), tag("d", "4", 4), tag("e", "5", 5)}}, always,
			[]Change{
				{Kind: "tag", Name: "c", Tag: "c", From: "2", FromName: "b", To: "3"},
				{Kind: "tag", Name: "d", Tag: "d", From: "3", FromName: "c", To: "4"},
				{Kind: "tag", Name: "e", Tag: "e", From: "4", FromName: "d", To: "5"},
			},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := Changes(tt.old, tt.now, tt.anc); !reflect.DeepEqual(got, tt.want) {
				t.Fatalf("got  %+v\nwant %+v", got, tt.want)
			}
		})
	}
}

func TestMerge(t *testing.T) {
	old := RefState{Default: "main", DefaultTip: "c5", Tags: []Tag{{"v1", "c1", 1}, {"v2", "c2", 2}}}
	behind := func(a, b string) bool { return a == "c3" && b == "c5" }
	got := Merge(old, RefState{Default: "main", DefaultTip: "c3", Tags: []Tag{{"v1", "c1", 1}}}, behind)
	want := RefState{Default: "main", DefaultTip: "c5", Tags: []Tag{{"v1", "c1", 1}, {"v2", "c2", 2}}}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("a stale clone must not roll the state back: %+v", got)
	}
	got = Merge(old, RefState{Default: "main", DefaultTip: "c9", Tags: old.Tags}, func(_, _ string) bool { return false })
	if got.DefaultTip != "c9" {
		t.Fatalf("a rewritten branch becomes the new baseline: %+v", got)
	}
}

func commit(t *testing.T, dir, file, content, msg string) string {
	t.Helper()
	write(t, dir, file, content)
	git(t, dir, "add", ".")
	git(t, dir, "commit", "-q", "-m", msg)
	return git(t, dir, "rev-parse", "HEAD")
}

func TestReadRefsAndDescribe(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git not installed")
	}
	ctx := context.Background()
	dir := t.TempDir()
	git(t, dir, "init", "-q")
	first := commit(t, dir, "a.go", "package a\n", "first")
	git(t, dir, "tag", "v1.0.0")
	commit(t, dir, "b.go", "package a\n", "second: add b")
	second := commit(t, dir, "a.go", "package a\n// changed\n", "third: change a")
	git(t, dir, "tag", "-a", "v1.1.0", "-m", "release")

	r, err := ReadRefs(ctx, dir)
	if err != nil {
		t.Fatal(err)
	}
	if r.Default != "main" || r.DefaultTip != second || len(r.Tags) != 2 || r.LatestTag() != "v1.1.0" {
		t.Fatalf("refs = %+v", r)
	}
	if r.Tags[0].Name != "v1.0.0" || r.Tags[0].Sha != first || r.Tags[1].Sha != second {
		t.Fatalf("an annotated tag must resolve to its commit: %+v", r.Tags)
	}
	if !IsAncestor(ctx, dir)(first, second) || IsAncestor(ctx, dir)(second, first) {
		t.Fatal("IsAncestor")
	}

	f, err := Describe(ctx, dir, first, second)
	if err != nil {
		t.Fatal(err)
	}
	if f.TotalCommits != 2 || len(f.Commits) != 2 || !strings.HasSuffix(f.Commits[0], "third: change a") ||
		!reflect.DeepEqual(f.Files, []string{"M a.go", "A b.go"}) || !strings.Contains(f.ShortStat, "2 files changed") {
		t.Fatalf("facts = %+v", f)
	}
	for _, want := range []string{"Commits (2):", "  " + f.Commits[0], "Changed files (2):", "  A b.go"} {
		if !strings.Contains(f.String(), want) {
			t.Errorf("rendered facts lack %q:\n%s", want, f.String())
		}
	}

	if f, err := Describe(ctx, dir, "", second); err != nil || len(f.Commits) != 3 || f.Files != nil {
		t.Fatalf("without a base only the log is listed: %+v %v", f, err)
	}
	if f, err := Describe(ctx, dir, strings.Repeat("0", 40), second); err != nil || len(f.Commits) != 3 {
		t.Fatalf("an unknown base falls back to the log: %+v %v", f, err)
	}

	// bounded
	many := t.TempDir()
	git(t, many, "init", "-q")
	base := commit(t, many, "seed", "x", "seed")
	for i := 0; i < maxFiles+10; i++ {
		write(t, many, filepath.Join("d", strings.Repeat("f", 1)+string(rune('a'+i%26))+string(rune('a'+i/26))), "x")
	}
	git(t, many, "add", ".")
	git(t, many, "commit", "-q", "-m", "many files")
	head := git(t, many, "rev-parse", "HEAD")
	f, err = Describe(ctx, many, base, head)
	if err != nil || f.TotalFiles != maxFiles+10 || len(f.Files) != maxFiles || !strings.Contains(f.String(), "first 40 shown") {
		t.Fatalf("bounded facts = %d/%d %v", len(f.Files), f.TotalFiles, err)
	}
}
