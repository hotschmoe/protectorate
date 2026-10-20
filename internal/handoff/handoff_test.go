package handoff

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func put(t *testing.T, path string, data []byte) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, data, 0o644); err != nil {
		t.Fatal(err)
	}
}

func fixture(t *testing.T, name string) []byte {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join("testdata", name))
	if err != nil {
		t.Fatal(err)
	}
	return raw
}

func TestReadTranscript(t *testing.T) {
	tests := []struct {
		harness string
		id      string
		path    string
		want    []Turn
	}{
		{"claude", "c0ffee-1", ".claude/projects/-w/c0ffee-1.jsonl", []Turn{
			{"user", "add a Foo function to lib"}, {"assistant", "I will add Foo."},
			{"user", "now export it"}, {"assistant", "Exported Foo.\nTests pass."},
		}},
		{"codex", "019a-77", ".codex/sessions/2026/10/20/rollout-2026-10-20T01-23-41-019a-77.jsonl", []Turn{
			{"user", "add a Foo function to lib"}, {"assistant", "Added Foo."},
		}},
		{"grok", "01a1-g", ".grok/sessions/%2Fw/01a1-g/chat_history.jsonl", []Turn{
			{"user", "add a Foo function to lib"}, {"assistant", "I'll add it."}, {"assistant", "Added Foo."},
		}},
		{"pi", "01a1-p", ".pi/sessions/2026-10-19T17-23-42-509Z_01a1-p.jsonl", []Turn{
			{"user", "add a Foo function to lib"}, {"assistant", "Added Foo."},
		}},
	}
	for _, tc := range tests {
		t.Run(tc.harness, func(t *testing.T) {
			home := t.TempDir()
			put(t, filepath.Join(home, tc.path), fixture(t, tc.harness+".jsonl"))
			tr, err := ReadTranscript(home, tc.harness, tc.id)
			if err != nil {
				t.Fatal(err)
			}
			if tr.Path != tc.path {
				t.Errorf("path = %q, want %q", tr.Path, tc.path)
			}
			if fmt.Sprint(tr.Turns) != fmt.Sprint(tc.want) {
				t.Errorf("turns = %q\nwant    %q", tr.Turns, tc.want)
			}
		})
	}
}

func TestReadTranscriptRefuses(t *testing.T) {
	outside := t.TempDir()
	put(t, filepath.Join(outside, "secret.jsonl"), []byte(`{"type":"user","message":{"role":"user","content":"SECRET"}}`+"\n"))
	tests := []struct {
		name  string
		setup func(home string)
		id    string
	}{
		{"glob in id", func(string) {}, "*"},
		{"path in id", func(string) {}, "../x"},
		{"missing", func(string) {}, "nosuch"},
		{"symlink out of the home", func(home string) {
			if err := os.MkdirAll(filepath.Join(home, ".claude/projects/-w"), 0o755); err != nil {
				t.Fatal(err)
			}
			if err := os.Symlink(filepath.Join(outside, "secret.jsonl"), filepath.Join(home, ".claude/projects/-w/s1.jsonl")); err != nil {
				t.Fatal(err)
			}
		}, "s1"},
		{"projects dir links out", func(home string) {
			if err := os.MkdirAll(filepath.Join(home, ".claude"), 0o755); err != nil {
				t.Fatal(err)
			}
			put(t, filepath.Join(outside, "-w/s2.jsonl"), []byte(`{"type":"user","message":{"role":"user","content":"SECRET"}}`+"\n"))
			if err := os.Symlink(outside, filepath.Join(home, ".claude/projects")); err != nil {
				t.Fatal(err)
			}
		}, "s2"},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			home := t.TempDir()
			tc.setup(home)
			tr, err := ReadTranscript(home, "claude", tc.id)
			if err == nil {
				t.Fatalf("read %v", tr)
			}
			if strings.Contains(fmt.Sprint(tr.Turns), "SECRET") {
				t.Fatal("read a file outside the home")
			}
		})
	}
}

func TestReadTranscriptTail(t *testing.T) {
	home := t.TempDir()
	var b strings.Builder
	b.WriteString(`{"type":"message","message":{"role":"user","content":"the very first"}}` + "\n")
	pad := strings.Repeat("x", 1<<20)
	for b.Len() < maxTranscriptRead+(2<<20) {
		fmt.Fprintf(&b, `{"type":"message","message":{"role":"toolResult","content":"%s"}}`+"\n", pad)
	}
	b.WriteString(`{"type":"message","message":{"role":"assistant","content":"the end"}}` + "\n")
	put(t, filepath.Join(home, ".pi/sessions/t_p9.jsonl"), []byte(b.String()))
	tr, err := ReadTranscript(home, "pi", "p9")
	if err != nil {
		t.Fatal(err)
	}
	if len(tr.Turns) != 1 || tr.Turns[0].Text != "the end" {
		t.Fatalf("turns = %v", tr.Turns)
	}
}

func TestMarkdownBudget(t *testing.T) {
	var turns []Turn
	turns = append(turns, Turn{"user", "FIRST ask"})
	for i := range 20 {
		turns = append(turns, Turn{"user", fmt.Sprintf("U%02d %s", i, strings.Repeat("u", 1000))}, Turn{"assistant", fmt.Sprintf("A%02d", i)})
	}
	turns = append(turns, Turn{"user", "LATEST ask"}, Turn{"assistant", "FINAL " + strings.Repeat("z", 9000) + " END"})
	f := Facts{
		Sleeve: "alice", Harness: "claude", To: "codex", SessionID: "s1", Workspace: "/w",
		Branch: "main", Head: "abc1234", Status: strings.Repeat("?? file\n", 1000), Log: "abc1234 2026-10-20 first",
		Transcript: Transcript{Path: ".claude/projects/-w/s1.jsonl", Turns: turns},
		HubNotes:   []string{"b1 fix it (by owner)"}, Mail: []string{"m1 request from bob: please"},
	}
	tests := []struct {
		budget  int
		has     []string
		hasNot  []string
		maxSize int
	}{
		{DefaultBudget, []string{"FIRST ask", "LATEST ask", "FINAL", " END", "U19", "U16", "bytes clipped", "b1 fix it", "m1 request", "branch main at abc1234"}, []string{"U15", "A19"}, DefaultBudget},
		{6000, []string{"FINAL", " END"}, []string{"U19"}, 6000},
		{8000, []string{"FINAL", " END", "LATEST ask", "FIRST ask"}, []string{"U18"}, 8000},
		{2500, []string{"git status"}, []string{"FINAL"}, 5000},
	}
	for _, tc := range tests {
		t.Run(fmt.Sprint(tc.budget), func(t *testing.T) {
			md := f.Markdown(tc.budget)
			for _, s := range tc.has {
				if !strings.Contains(md, s) {
					t.Errorf("missing %q", s)
				}
			}
			for _, s := range tc.hasNot {
				if strings.Contains(md, s) {
					t.Errorf("unexpected %q", s)
				}
			}
			if len(md) > tc.maxSize {
				t.Errorf("size %d > %d", len(md), tc.maxSize)
			}
			if i, j := strings.Index(md, "FIRST ask"), strings.Index(md, "LATEST ask"); i >= 0 && j >= 0 && i > j {
				t.Error("conversation is not in transcript order")
			}
		})
	}
}

func TestDocumentSummaryNotes(t *testing.T) {
	f := Facts{Sleeve: "alice", Harness: "claude", To: "codex", Workspace: "/w", At: time.Date(2026, 10, 20, 1, 2, 3, 0, time.UTC)}
	tests := []struct {
		name string
		s    Summary
		want []string
	}{
		{"frontier", Summary{Text: "# Goal\n- ship\n## Next\n- test", Harness: "codex", Model: "gpt"}, []string{"Summarized by codex gpt from", "### Goal", "### Next", "2026-10-20T01:02:03Z"}},
		{"local", Summary{Text: "## Goal\n- ship", Harness: "pi", Provider: "hotschmoe-local", Model: "dd", Local: true}, []string{"local model (pi hotschmoe-local dd), not a frontier model", "### Goal"}},
		{"none", Summary{Err: "no harness logged in"}, []string{"No summary: no harness logged in. Work from the facts."}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			doc := Document(f, tc.s, DefaultBudget)
			for _, w := range tc.want {
				if !strings.Contains(doc, w) {
					t.Errorf("missing %q in\n%s", w, doc)
				}
			}
			if !strings.Contains(doc, "## Facts") {
				t.Error("no facts")
			}
		})
	}
}

func TestWrite(t *testing.T) {
	outside := t.TempDir()
	tests := []struct {
		name  string
		setup func(ws string)
		ok    bool
	}{
		{"fresh", func(string) {}, true},
		{"overwrites", func(ws string) { put(t, filepath.Join(ws, File), []byte("stale")) }, true},
		{"dir is a symlink out", func(ws string) {
			if err := os.Symlink(outside, filepath.Join(ws, Dir)); err != nil {
				t.Fatal(err)
			}
		}, false},
		{"file is a symlink out", func(ws string) {
			if err := os.MkdirAll(filepath.Join(ws, Dir), 0o755); err != nil {
				t.Fatal(err)
			}
			if err := os.Symlink(filepath.Join(outside, "victim"), filepath.Join(ws, File)); err != nil {
				t.Fatal(err)
			}
		}, true},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			ws := t.TempDir()
			tc.setup(ws)
			p, err := Write(ws, []byte("new handoff"))
			if (err == nil) != tc.ok {
				t.Fatalf("write: %v", err)
			}
			if _, err := os.Stat(filepath.Join(outside, "victim")); err == nil {
				t.Fatal("wrote through a symlink")
			}
			if ents, _ := os.ReadDir(outside); len(ents) > 0 {
				t.Fatalf("wrote outside the workspace: %v", ents)
			}
			if !tc.ok {
				return
			}
			fi, err := os.Lstat(p)
			if err != nil || !fi.Mode().IsRegular() {
				t.Fatalf("handoff %s: %v %v", p, fi, err)
			}
			if got, _ := os.ReadFile(p); string(got) != "new handoff" {
				t.Fatalf("content %q", got)
			}
		})
	}
}

func gitIn(t *testing.T, dir string, args ...string) string {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
	cmd.Env = append(os.Environ(), "GIT_CONFIG_GLOBAL=/dev/null", "GIT_CONFIG_NOSYSTEM=1",
		"GIT_AUTHOR_NAME=t", "GIT_AUTHOR_EMAIL=t@x", "GIT_COMMITTER_NAME=t", "GIT_COMMITTER_EMAIL=t@x")
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("git %v: %v: %s", args, err, out)
	}
	return string(out)
}

func TestExclude(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git not installed")
	}
	tests := []struct {
		name  string
		setup func(t *testing.T) (ws string)
	}{
		{"repo root", func(t *testing.T) string {
			ws := t.TempDir()
			gitIn(t, ws, "init", "-q")
			return ws
		}},
		{"subdirectory", func(t *testing.T) string {
			repo := t.TempDir()
			gitIn(t, repo, "init", "-q")
			ws := filepath.Join(repo, "app")
			put(t, filepath.Join(ws, "x"), []byte("x"))
			return ws
		}},
		{"linked worktree", func(t *testing.T) string {
			repo := t.TempDir()
			gitIn(t, repo, "init", "-q")
			put(t, filepath.Join(repo, "f"), []byte("f"))
			gitIn(t, repo, "add", "f")
			gitIn(t, repo, "commit", "-qm", "c")
			ws := filepath.Join(t.TempDir(), "wt")
			gitIn(t, repo, "worktree", "add", "-q", ws)
			return ws
		}},
		{"exclude without newline", func(t *testing.T) string {
			ws := t.TempDir()
			gitIn(t, ws, "init", "-q")
			put(t, filepath.Join(ws, ".git/info/exclude"), []byte("*.log"))
			return ws
		}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			ws := tc.setup(t)
			if added, err := Exclude(ws); err != nil || !added {
				t.Fatalf("first exclude: %v %v", added, err)
			}
			if added, err := Exclude(ws); err != nil || added {
				t.Fatalf("second exclude: %v %v", added, err)
			}
			if _, err := Write(ws, []byte("h")); err != nil {
				t.Fatal(err)
			}
			if st := gitIn(t, ws, "status", "--porcelain", "--untracked-files=all"); strings.Contains(st, Dir) {
				t.Fatalf("git sees the handoff: %s", st)
			}
		})
	}
	if added, err := Exclude(t.TempDir()); err != nil || added {
		t.Fatalf("no repository: %v %v", added, err)
	}
	ws := t.TempDir()
	put(t, filepath.Join(ws, ".git"), []byte("gitdir: "+t.TempDir()))
	if _, err := Exclude(ws); err == nil {
		t.Fatal("followed a .git file to something that is not a git directory")
	}
}

func TestReadGitFacts(t *testing.T) {
	calls := map[string]string{
		"status":       "## main\n M a.go",
		"symbolic-ref": "main\n",
		"rev-parse":    "abc1234\n",
		"diff":         " a.go | 2 +-",
		"log":          "abc1234 2026-10-20 first",
	}
	f := Facts{Workspace: "/w"}
	f.ReadGit(context.Background(), func(_ context.Context, dir string, args ...string) (string, error) {
		if dir != "/w" {
			t.Errorf("dir %q", dir)
		}
		return calls[args[0]], nil
	})
	if f.Branch != "main" || f.Head != "abc1234" || f.Status != "## main\n M a.go" || f.DiffStat == "" || f.Log == "" || f.GitErr != "" {
		t.Fatalf("facts %+v", f)
	}
	g := Facts{Workspace: "/w"}
	g.ReadGit(context.Background(), func(context.Context, string, ...string) (string, error) {
		return "", fmt.Errorf("not a git repository")
	})
	if g.GitErr == "" || g.Status != "" {
		t.Fatalf("facts %+v", g)
	}
}
