package backlog

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"testing"
	"time"
)

const sample = `# My backlog

Some free text the user wrote.

## hotschmoe/protectorate
- [ ] Add GPU metrics to node agent  #p2 @any  id:b7f3
  Optional indented notes.
  second note line
- [ ] Fix flaky test  https://github.com/hotschmoe/protectorate/issues/12  @codex  id:9c1a
- [x] Done item  id:0001
- [~] Taken item @grok id:0002 claim:alice/grok@2026-10-19T10:00:00Z
* not an item, just a bullet
-[ ] also not an item

## donate: owner/public-repo
- [ ] https://github.com/owner/public-repo/issues/345  id:0003
- [ ] no id yet @pi
trailing text
`

func TestParseItems(t *testing.T) {
	items := Parse(sample).Items()
	want := []Item{
		{ID: "b7f3", Project: "hotschmoe/protectorate", Title: "Add GPU metrics to node agent", Priority: 2, Harness: "any", Notes: "Optional indented notes.\nsecond note line"},
		{ID: "9c1a", Project: "hotschmoe/protectorate", Title: "Fix flaky test", IssueURL: "https://github.com/hotschmoe/protectorate/issues/12", Harness: "codex"},
		{ID: "0001", Project: "hotschmoe/protectorate", Title: "Done item", State: StateDone},
		{ID: "0002", Project: "hotschmoe/protectorate", Title: "Taken item", Harness: "grok", State: StateClaimed,
			Claim: &Claim{By: "alice/grok", At: time.Date(2026, 10, 19, 10, 0, 0, 0, time.UTC)}},
		{ID: "0003", Project: "donate: owner/public-repo", IssueURL: "https://github.com/owner/public-repo/issues/345", Donate: true},
		{Project: "donate: owner/public-repo", Title: "no id yet", Harness: "pi", Donate: true},
	}
	if len(items) != len(want) {
		t.Fatalf("got %d items, want %d: %+v", len(items), len(want), items)
	}
	for i, w := range want {
		g := items[i]
		g.Line = 0
		if fmt.Sprintf("%+v", g.Claim) != fmt.Sprintf("%+v", w.Claim) {
			t.Errorf("item %d claim = %+v, want %+v", i, g.Claim, w.Claim)
		}
		g.Claim, w.Claim = nil, nil
		if g != w {
			t.Errorf("item %d = %+v, want %+v", i, g, w)
		}
	}
}

func TestTitleEdgeCases(t *testing.T) {
	tests := []struct {
		line  string
		title string
		kinds string
	}{
		{"- [ ] plain title", "plain title", ""},
		{"- [ ] mention @bob in the middle id:1", "mention @bob in the middle", "id"},
		{"- [ ] A #p1 #p2 id:x", "A #p1", "id prio"},
		{"- [ ] A https://a https://b id:x", "A https://a", "id url"},
		{"- [ ]   spaced   words   @codex", "spaced   words", "harness"},
		{"- [ ] id:only", "", "id"},
		{"- [X] upper id:u", "", "id"},
	}
	for _, tt := range tests {
		p, ok := parseItemLine(tt.line)
		if !ok {
			t.Fatalf("%q did not parse", tt.line)
		}
		if tt.line == "- [X] upper id:u" {
			tt.title = "upper"
		}
		if p.title != tt.title {
			t.Errorf("%q: title %q, want %q", tt.line, p.title, tt.title)
		}
		for _, k := range strings.Fields(tt.kinds) {
			if _, ok := p.spans[k]; !ok {
				t.Errorf("%q: missing %s", tt.line, k)
			}
		}
	}
}

func TestRoundTripSample(t *testing.T) {
	for _, in := range []string{"", "\n", "\n\n", sample, strings.TrimSuffix(sample, "\n"), strings.ReplaceAll(sample, "\n", "\r\n"), "- [ ] x\r\n\tnote\r\n"} {
		if got := Parse(in).String(); got != in {
			t.Errorf("round trip changed %q into %q", in, got)
		}
	}
}

func TestSetStateKeepsOtherLines(t *testing.T) {
	now := time.Date(2026, 10, 20, 8, 30, 0, 0, time.UTC)
	tests := []struct {
		name    string
		id      string
		st      State
		by      string
		want    string
		wantErr error
	}{
		{"claim", "b7f3", StateClaimed, "bob/codex", "- [~] Add GPU metrics to node agent  #p2 @any  id:b7f3 claim:bob/codex@2026-10-20T08:30:00Z", nil},
		{"claim needs by", "b7f3", StateClaimed, "", "", ErrInvalid},
		{"claim bad by", "b7f3", StateClaimed, "a b", "", ErrInvalid},
		{"claim taken", "0002", StateClaimed, "bob", "", ErrUnavailable},
		{"claim same holder idempotent", "0002", StateClaimed, "alice/grok", "- [~] Taken item @grok id:0002 claim:alice/grok@2026-10-19T10:00:00Z", nil},
		{"claim done", "0001", StateClaimed, "bob", "", ErrUnavailable},
		{"release", "0002", StateTodo, "", "- [ ] Taken item @grok id:0002", nil},
		{"reopen", "0001", StateTodo, "", "- [ ] Done item  id:0001", nil},
		{"done keeps claim", "0002", StateDone, "", "- [x] Taken item @grok id:0002 claim:alice/grok@2026-10-19T10:00:00Z", nil},
		{"done by holder", "0002", StateDone, "alice/grok", "- [x] Taken item @grok id:0002 claim:alice/grok@2026-10-19T10:00:00Z", nil},
		{"done by thief", "0002", StateDone, "bob", "", ErrUnavailable},
		{"done records doer", "9c1a", StateDone, "carol", "- [x] Fix flaky test  https://github.com/hotschmoe/protectorate/issues/12  @codex  id:9c1a claim:carol@2026-10-20T08:30:00Z", nil},
		{"missing", "ffff", StateDone, "", "", ErrNotFound},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			d := Parse(sample)
			before := append([]string(nil), d.lines...)
			it, _, err := d.SetState(tt.id, tt.st, tt.by, now)
			if !errors.Is(err, tt.wantErr) {
				t.Fatalf("err = %v, want %v", err, tt.wantErr)
			}
			if err != nil {
				if d.String() != sample {
					t.Fatal("failed update modified the document")
				}
				return
			}
			if it.State != tt.st {
				t.Errorf("state = %v", it.State)
			}
			for i := range before {
				switch {
				case i == it.Line && d.lines[i] != tt.want:
					t.Errorf("line = %q\nwant   %q", d.lines[i], tt.want)
				case i != it.Line && d.lines[i] != before[i]:
					t.Errorf("line %d changed: %q", i, d.lines[i])
				}
			}
		})
	}
}

func TestAdd(t *testing.T) {
	tests := []struct {
		name    string
		in      string
		n       NewItem
		wantHas string
		wantErr error
	}{
		{"existing project after notes", sample, NewItem{Project: "hotschmoe/protectorate", Title: "New one", Priority: 1, Harness: "claude"}, "claim:alice/grok@2026-10-19T10:00:00Z\n- [ ] New one  #p1 @claude id:", nil},
		{"new project", sample, NewItem{Project: "other/repo", Title: "T"}, "text\n\n## other/repo\n- [ ] T  id:", nil},
		{"empty file", "", NewItem{Project: "p", Title: "T"}, "## p\n- [ ] T  id:", nil},
		{"no trailing newline", "## p\n- [ ] a id:1", NewItem{Project: "p", Title: "b"}, "- [ ] a id:1\n- [ ] b  id:", nil},
		{"donate with url only", "", NewItem{Project: "donate: o/r", IssueURL: "https://github.com/o/r/issues/1"}, "- [ ] https://github.com/o/r/issues/1 id:", nil},
		{"donate needs url", "", NewItem{Project: "donate: o/r", Title: "x"}, "", ErrInvalid},
		{"no title or url", "", NewItem{Project: "p"}, "", ErrInvalid},
		{"bad prio", "", NewItem{Project: "p", Title: "x", Priority: 10}, "", ErrInvalid},
		{"bad harness", "", NewItem{Project: "p", Title: "x", Harness: "a b"}, "", ErrInvalid},
		{"multiline", "", NewItem{Project: "p", Title: "x\ny"}, "", ErrInvalid},
		{"title ends in meta", "", NewItem{Project: "p", Title: "ping @bob"}, "", ErrInvalid},
		{"bad url", "", NewItem{Project: "p", Title: "x", IssueURL: "ftp://x"}, "", ErrInvalid},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			d := Parse(tt.in)
			it, err := d.Add(tt.n)
			if !errors.Is(err, tt.wantErr) {
				t.Fatalf("err = %v, want %v", err, tt.wantErr)
			}
			if err != nil {
				if d.String() != tt.in {
					t.Fatal("failed add modified the document")
				}
				return
			}
			if !strings.Contains(d.String(), tt.wantHas) {
				t.Errorf("document lacks %q:\n%s", tt.wantHas, d)
			}
			if it.ID == "" || it.Project != tt.n.Project || it.Title != tt.n.Title && !strings.HasPrefix(tt.n.Title, "https://") && tt.n.Title != "" {
				t.Errorf("item = %+v", it)
			}
			if it.IssueURL != tt.n.IssueURL || it.Priority != tt.n.Priority || it.Harness != tt.n.Harness {
				t.Errorf("fields not preserved: %+v", it)
			}
			var rest []string
			for _, l := range strings.Split(d.String(), "\n") {
				if !strings.Contains(l, "id:"+it.ID) {
					rest = append(rest, l)
				}
			}
			if got := strings.Join(rest, "\n"); !strings.HasPrefix(got, strings.TrimRight(tt.in, "\n")) {
				t.Errorf("existing text was altered:\n%s", d)
			}
		})
	}
}

func TestEnsureIDs(t *testing.T) {
	d := Parse(sample)
	changed, err := d.EnsureIDs()
	if err != nil || !changed {
		t.Fatalf("changed=%v err=%v", changed, err)
	}
	items := d.Items()
	seen := map[string]bool{}
	for _, it := range items {
		if it.ID == "" || seen[it.ID] {
			t.Fatalf("bad ids: %+v", items)
		}
		seen[it.ID] = true
	}
	if !strings.Contains(d.String(), "- [ ] no id yet @pi id:") {
		t.Errorf("id not appended in place:\n%s", d)
	}
	if again, _ := d.EnsureIDs(); again {
		t.Error("second EnsureIDs changed the document")
	}
}

func TestQuery(t *testing.T) {
	todo, done := StateTodo, StateDone
	tests := []struct {
		name string
		f    Filter
		want int
	}{
		{"all", Filter{}, 6},
		{"project", Filter{Project: "donate: owner/public-repo"}, 2},
		{"state todo", Filter{State: &todo}, 4},
		{"state done", Filter{State: &done}, 1},
		{"harness codex gets codex, any, unset", Filter{Harness: "codex"}, 4},
		{"harness pi", Filter{Harness: "pi"}, 4},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := len(Parse(sample).Query(tt.f)); got != tt.want {
				t.Errorf("got %d, want %d", got, tt.want)
			}
		})
	}
}

func newTestStore(t *testing.T, content string) *Store {
	t.Helper()
	p := filepath.Join(t.TempDir(), "sub", "backlog.md")
	if content != "" {
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	return NewStore(p)
}

func TestStoreCreatesFile(t *testing.T) {
	s := newTestStore(t, "")
	if items, err := s.List(Filter{}); err != nil || len(items) != 0 {
		t.Fatalf("items=%v err=%v", items, err)
	}
	if _, err := s.Add(NewItem{Project: "p", Title: "first"}); err != nil {
		t.Fatal(err)
	}
	b, err := os.ReadFile(s.Path())
	if err != nil || !strings.HasPrefix(string(b), "## p\n- [ ] first  id:") {
		t.Fatalf("file = %q err=%v", b, err)
	}
}

func TestStoreListAssignsIDs(t *testing.T) {
	s := newTestStore(t, "## p\n- [ ] a\n- [ ] b\n")
	items, err := s.List(Filter{})
	if err != nil || len(items) != 2 || items[0].ID == "" || items[1].ID == "" {
		t.Fatalf("items=%+v err=%v", items, err)
	}
	b, _ := os.ReadFile(s.Path())
	if !strings.Contains(string(b), "- [ ] a id:"+items[0].ID+"\n") {
		t.Errorf("file = %q", b)
	}
}

func TestStorePreservesFileMode(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("Windows has no unix permission bits")
	}
	s := newTestStore(t, "## p\n- [ ] a id:1\n")
	if _, _, err := s.Update("1", StateDone, ""); err != nil {
		t.Fatal(err)
	}
	fi, err := os.Stat(s.Path())
	if err != nil {
		t.Fatal(err)
	}
	if fi.Mode().Perm() != 0o644 {
		t.Errorf("mode = %v", fi.Mode().Perm())
	}
	ents, _ := os.ReadDir(filepath.Dir(s.Path()))
	if len(ents) != 1 {
		t.Errorf("leftover files: %v", ents)
	}
}

func TestStoreReappliesAfterExternalEdit(t *testing.T) {
	s := newTestStore(t, "## p\n- [ ] a id:aa\n")
	calls := 0
	s.beforeCommit = func() {
		calls++
		if calls == 1 {
			err := os.WriteFile(s.Path(), []byte("# human wrote this\n## p\n- [ ] a id:aa\n- [ ] human item id:hh\n"), 0o644)
			if err != nil {
				t.Error(err)
			}
		}
	}
	if _, _, err := s.Update("aa", StateClaimed, "bob"); err != nil {
		t.Fatal(err)
	}
	if calls != 2 {
		t.Errorf("commit attempts = %d, want 2", calls)
	}
	b, _ := os.ReadFile(s.Path())
	got := string(b)
	if !strings.HasPrefix(got, "# human wrote this\n## p\n- [~] a id:aa claim:bob@") || !strings.Contains(got, "- [ ] human item id:hh\n") {
		t.Errorf("external edit lost or change not applied:\n%s", got)
	}
}

func TestStoreGivesUpOnEndlessChurn(t *testing.T) {
	s := newTestStore(t, "## p\n- [ ] a id:aa\n")
	n := 0
	s.beforeCommit = func() {
		n++
		_ = os.WriteFile(s.Path(), []byte(fmt.Sprintf("## p\n- [ ] a id:aa\n%d\n", n)), 0o644)
	}
	if _, _, err := s.Update("aa", StateDone, ""); err == nil {
		t.Fatal("expected error")
	}
}

func TestStoreFollowsSymlink(t *testing.T) {
	dir := t.TempDir()
	real := filepath.Join(dir, "real.md")
	link := filepath.Join(dir, "backlog.md")
	if err := os.WriteFile(real, []byte("## p\n- [ ] a id:1\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(real, link); err != nil {
		t.Skip("symlinks unavailable:", err)
	}
	if _, _, err := NewStore(link).Update("1", StateDone, ""); err != nil {
		t.Fatal(err)
	}
	if fi, _ := os.Lstat(link); fi.Mode()&os.ModeSymlink == 0 {
		t.Error("symlink was replaced")
	}
	if b, _ := os.ReadFile(real); !strings.Contains(string(b), "[x]") {
		t.Errorf("target not updated: %q", b)
	}
}

func TestConcurrentClaimsHaveOneWinner(t *testing.T) {
	s := newTestStore(t, "## p\n- [ ] a id:aa\n")
	const n = 20
	var wg sync.WaitGroup
	errs := make([]error, n)
	for i := range n {
		wg.Add(1)
		go func() {
			defer wg.Done()
			_, _, errs[i] = NewStore(s.Path()).Update("aa", StateClaimed, fmt.Sprintf("s%d", i))
		}()
	}
	wg.Wait()
	wins := 0
	for _, err := range errs {
		switch {
		case err == nil:
			wins++
		case !errors.Is(err, ErrUnavailable):
			t.Errorf("unexpected error: %v", err)
		}
	}
	if wins != 1 {
		t.Fatalf("%d claimants won, want 1", wins)
	}
}

func TestConcurrentAddsKeepAll(t *testing.T) {
	s := newTestStore(t, "## p\n")
	var wg sync.WaitGroup
	for i := range 20 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if _, err := NewStore(s.Path()).Add(NewItem{Project: "p", Title: fmt.Sprintf("item %d", i)}); err != nil {
				t.Error(err)
			}
		}()
	}
	wg.Wait()
	items, err := s.List(Filter{})
	if err != nil || len(items) != 20 {
		t.Fatalf("got %d items, err=%v", len(items), err)
	}
}

func FuzzRoundTrip(f *testing.F) {
	f.Add(sample)
	f.Add("")
	f.Add("\n\n")
	f.Add("## \n- [ ]\n- [ ] \r\n  note\r\n\t\n")
	f.Add("- [x] a #p1 @b id:c claim:d@e https://f\n")
	f.Fuzz(func(t *testing.T, in string) {
		d := Parse(in)
		if got := d.String(); got != in {
			t.Fatalf("round trip changed %q into %q", in, got)
		}
		d.Items()
	})
}

// FuzzEditPreservesRest checks that state changes, id assignment and adds
// touch only the intended lines of arbitrary documents.
func FuzzEditPreservesRest(f *testing.F) {
	f.Add(sample, uint8(0), uint8(1))
	f.Add("- [ ] a id:1\n- [ ] b id:1\n", uint8(1), uint8(2))
	f.Add("## p\r\n- [ ] a\r\n", uint8(0), uint8(0))
	f.Fuzz(func(t *testing.T, in string, pick, state uint8) {
		d := Parse(in)
		items := d.Items()
		if _, err := d.EnsureIDs(); err != nil {
			t.Fatal(err)
		}
		items = d.Items()
		if len(items) == 0 {
			return
		}
		it := items[int(pick)%len(items)]
		before := append([]string(nil), d.lines...)
		got, _, err := d.SetState(it.ID, State(state%3), "w", time.Unix(0, 0))
		if err != nil {
			if d.String() != strings.Join(before, "\n")+tail(d) {
				t.Fatal("failed SetState modified the document")
			}
			return
		}
		for i := range before {
			if i != got.Line && d.lines[i] != before[i] {
				t.Fatalf("line %d changed", i)
			}
		}
		if again, _ := Parse(d.String()).find(it.ID); again.State != got.State || again.Title != got.Title {
			t.Fatalf("reparse differs: %+v vs %+v", again, got)
		}
	})
}

func tail(d *Doc) string {
	if d.nl && len(d.lines) > 0 {
		return "\n"
	}
	return ""
}
