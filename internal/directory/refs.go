package directory

import (
	"context"
	"fmt"
	"sort"
	"strconv"
	"strings"
)

const (
	maxTagChanges = 3
	maxCommits    = 20
	maxFiles      = 40
	maxSubject    = 120
)

// ReadRefs reads the release state of the repository in dir: the default
// branch and its tip, and every tag with the commit it points at. It runs
// local, read-only git commands only.
func ReadRefs(ctx context.Context, dir string) (RefState, error) {
	var r RefState
	if out, err := Git(ctx, dir, "symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"); err == nil {
		r.Default = strings.TrimPrefix(strings.TrimSpace(out), "origin/")
	}
	out, err := Git(ctx, dir, "for-each-ref", "--format=%(refname)%00%(objectname)%00%(*objectname)%00%(creatordate:unix)", "refs/tags", "refs/heads", "refs/remotes/origin")
	if err != nil {
		return RefState{}, err
	}
	heads := map[string]string{}
	remotes := map[string]string{}
	for _, line := range strings.Split(strings.TrimSpace(out), "\n") {
		f := strings.Split(line, "\x00")
		if len(f) != 4 {
			continue
		}
		name, sha := f[0], f[1]
		switch {
		case strings.HasPrefix(name, "refs/tags/"):
			if f[2] != "" {
				sha = f[2]
			}
			unix, _ := strconv.ParseInt(f[3], 10, 64)
			r.Tags = append(r.Tags, Tag{Name: strings.TrimPrefix(name, "refs/tags/"), Sha: sha, Unix: unix})
		case strings.HasPrefix(name, "refs/heads/"):
			heads[strings.TrimPrefix(name, "refs/heads/")] = sha
		case strings.HasPrefix(name, "refs/remotes/origin/"):
			remotes[strings.TrimPrefix(name, "refs/remotes/origin/")] = sha
		}
	}
	sort.Slice(r.Tags, func(i, j int) bool {
		if r.Tags[i].Unix != r.Tags[j].Unix {
			return r.Tags[i].Unix < r.Tags[j].Unix
		}
		return r.Tags[i].Name < r.Tags[j].Name
	})
	if r.Default == "" {
		for _, n := range []string{"main", "master"} {
			if heads[n] != "" || remotes[n] != "" {
				r.Default = n
				break
			}
		}
	}
	if r.Default != "" {
		r.DefaultTip = heads[r.Default]
		if r.DefaultTip == "" {
			r.DefaultTip = remotes[r.Default]
		}
	}
	return r, nil
}

// Change is one release event between two observations of a repository.
type Change struct {
	// Kind is "tag" or "branch".
	Kind string
	// Name is the tag, or the default branch.
	Name string
	// From and To are the commits compared; FromName is the ref that names
	// From, such as the previous tag, or "".
	From, To string
	FromName string
	Tag      string
}

// Changes compares two observations: every new tag, and an advance of the
// default branch that is not already covered by a new tag at its tip. A
// repository is only compared with its own earlier state; isAncestor reports
// whether the old tip is an ancestor of the new one, so a rewound or
// re-created branch is not announced.
func Changes(old, now RefState, isAncestor func(old, new string) bool) []Change {
	known := map[string]bool{}
	for _, t := range old.Tags {
		known[t.Name] = true
	}
	prev, prevName := old.DefaultTip, ""
	if len(old.Tags) > 0 {
		last := old.Tags[len(old.Tags)-1]
		prev, prevName = last.Sha, last.Name
	}
	var out []Change
	covered := false
	for _, t := range now.Tags {
		if known[t.Name] {
			continue
		}
		out = append(out, Change{Kind: "tag", Name: t.Name, Tag: t.Name, From: prev, FromName: prevName, To: t.Sha})
		prev, prevName = t.Sha, t.Name
		covered = covered || (t.Sha == now.DefaultTip && now.DefaultTip != "")
	}
	if len(out) > maxTagChanges {
		out = out[len(out)-maxTagChanges:]
	}
	if !covered && old.Default != "" && old.Default == now.Default && old.DefaultTip != "" &&
		now.DefaultTip != "" && now.DefaultTip != old.DefaultTip && isAncestor(old.DefaultTip, now.DefaultTip) {
		out = append(out, Change{Kind: "branch", Name: now.Default, From: old.DefaultTip, To: now.DefaultTip})
	}
	return out
}

// Merge returns the state to remember after observing now: tags are never
// forgotten, since another clone of the project may simply lack them, and a
// default tip that is behind the remembered one (a stale clone) is ignored.
func Merge(old, now RefState, isAncestor func(old, new string) bool) RefState {
	seen := map[string]bool{}
	for _, t := range now.Tags {
		seen[t.Name] = true
	}
	for _, t := range old.Tags {
		if !seen[t.Name] {
			now.Tags = append(now.Tags, t)
		}
	}
	sort.Slice(now.Tags, func(i, j int) bool {
		if now.Tags[i].Unix != now.Tags[j].Unix {
			return now.Tags[i].Unix < now.Tags[j].Unix
		}
		return now.Tags[i].Name < now.Tags[j].Name
	})
	if old.Default == now.Default && old.DefaultTip != "" && now.DefaultTip != old.DefaultTip && isAncestor(now.DefaultTip, old.DefaultTip) {
		now.DefaultTip = old.DefaultTip
	}
	return now
}

// IsAncestor reports whether a is an ancestor of b in the repository in dir.
func IsAncestor(ctx context.Context, dir string) func(a, b string) bool {
	return func(a, b string) bool {
		_, err := Git(ctx, dir, "merge-base", "--is-ancestor", a, b)
		return err == nil
	}
}

// Facts describes what changed between two commits, bounded.
type Facts struct {
	Commits      []string // "abc1234 subject", newest first
	TotalCommits int
	Files        []string // "M path"
	TotalFiles   int
	ShortStat    string
}

// Describe lists the commits and changed files from..to. With no usable from
// it lists only the latest commits at to.
func Describe(ctx context.Context, dir, from, to string) (Facts, error) {
	var f Facts
	rng := to
	if from != "" {
		rng = from + ".." + to
		if _, err := Git(ctx, dir, "cat-file", "-e", from+"^{commit}"); err != nil {
			from, rng = "", to
		}
	}
	out, err := Git(ctx, dir, "log", "--format=%h%x00%s", "-n", strconv.Itoa(maxCommits), rng)
	if err != nil {
		return Facts{}, err
	}
	for _, line := range strings.Split(strings.TrimSpace(out), "\n") {
		if sha, subj, ok := strings.Cut(line, "\x00"); ok {
			f.Commits = append(f.Commits, sha+" "+bound(subj, maxSubject))
		}
	}
	if from == "" {
		f.TotalCommits = len(f.Commits)
		return f, nil
	}
	if n, err := Git(ctx, dir, "rev-list", "--count", rng); err == nil {
		f.TotalCommits, _ = strconv.Atoi(strings.TrimSpace(n))
	}
	names, err := Git(ctx, dir, "diff", "--no-textconv", "--no-ext-diff", "--name-status", from, to)
	if err != nil {
		return f, nil
	}
	for _, line := range strings.Split(strings.TrimSpace(names), "\n") {
		if line == "" {
			continue
		}
		f.TotalFiles++
		if len(f.Files) < maxFiles {
			f.Files = append(f.Files, strings.Join(strings.Fields(line), " "))
		}
	}
	if st, err := Git(ctx, dir, "diff", "--no-textconv", "--no-ext-diff", "--shortstat", from, to); err == nil {
		f.ShortStat = strings.TrimSpace(st)
	}
	return f, nil
}

func bound(s string, n int) string {
	if r := []rune(s); len(r) > n {
		return string(r[:n-1]) + "..."
	}
	return s
}

// Short abbreviates a commit id for display.
func Short(sha string) string {
	if len(sha) > 7 {
		return sha[:7]
	}
	return sha
}

// String renders facts as the lines of a notice.
func (f Facts) String() string {
	var b strings.Builder
	if len(f.Commits) > 0 {
		fmt.Fprintf(&b, "Commits (%d", f.TotalCommits)
		if f.TotalCommits > len(f.Commits) {
			fmt.Fprintf(&b, ", newest %d shown", len(f.Commits))
		}
		b.WriteString("):\n")
		for _, c := range f.Commits {
			b.WriteString("  " + c + "\n")
		}
	}
	if len(f.Files) > 0 {
		fmt.Fprintf(&b, "Changed files (%d", f.TotalFiles)
		if f.TotalFiles > len(f.Files) {
			fmt.Fprintf(&b, ", first %d shown", len(f.Files))
		}
		b.WriteString("):\n")
		for _, c := range f.Files {
			b.WriteString("  " + c + "\n")
		}
		if f.ShortStat != "" {
			b.WriteString("  " + f.ShortStat + "\n")
		}
	}
	return b.String()
}
