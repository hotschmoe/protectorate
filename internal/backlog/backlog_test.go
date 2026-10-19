package backlog

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"
)

func testStore(t *testing.T) (*Store, *time.Time) {
	t.Helper()
	s := NewStore(t.TempDir())
	clock := time.Date(2026, 10, 20, 9, 0, 0, 0, time.UTC)
	s.now = func() time.Time { clock = clock.Add(time.Minute); return clock }
	return s, &clock
}

func ptr[T any](v T) *T { return &v }

func TestCreateValidation(t *testing.T) {
	tests := []struct {
		name    string
		in      NewNote
		wantErr bool
		check   func(Note) string
	}{
		{"title only", NewNote{Title: "  call dentist  "}, false, func(n Note) string {
			if n.Title != "call dentist" {
				return "title not trimmed"
			}
			return ""
		}},
		{"body only", NewNote{Body: "just text"}, false, nil},
		{"crlf body", NewNote{Title: "t", Body: "a\r\nb"}, false, func(n Note) string {
			if n.Body != "a\nb" {
				return "crlf kept"
			}
			return ""
		}},
		{"empty", NewNote{}, true, nil},
		{"blank body", NewNote{Body: " \n "}, true, nil},
		{"multi-line title", NewNote{Title: "a\nb"}, true, nil},
		{"long title", NewNote{Title: strings.Repeat("x", maxTitle+1)}, true, nil},
		{"multi-line project", NewNote{Title: "t", Project: "a\nb"}, true, nil},
		{"huge body", NewNote{Title: "t", Body: strings.Repeat("x", maxBody+1)}, true, nil},
		{"nul in body", NewNote{Title: "t", Body: "a\x00b"}, true, nil},
		{"bad utf8", NewNote{Title: "t", Body: "\xff\xfe"}, true, nil},
		{"tagged", NewNote{Title: "t", Project: "me/repo", Source: "alice", Pinned: true}, false, func(n Note) string {
			if n.Project != "me/repo" || n.Source != "alice" || !n.Pinned {
				return "fields lost"
			}
			return ""
		}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			s, _ := testStore(t)
			n, err := s.Create(tt.in)
			if tt.wantErr {
				if !errors.Is(err, ErrInvalid) {
					t.Fatalf("err = %v, want ErrInvalid", err)
				}
				if all, _ := s.List(Filter{}); len(all) != 0 {
					t.Fatalf("a rejected note was stored: %v", all)
				}
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			if n.ID == "" || n.Created.IsZero() || !n.Created.Equal(n.Updated) {
				t.Fatalf("note = %+v", n)
			}
			if tt.check != nil {
				if msg := tt.check(n); msg != "" {
					t.Fatal(msg)
				}
			}
		})
	}
}

func TestUpdate(t *testing.T) {
	tests := []struct {
		name        string
		patch       Patch
		wantErr     error
		wantChanged bool
		check       func(Note) bool
	}{
		{"title", Patch{Title: ptr("new")}, nil, true, func(n Note) bool { return n.Title == "new" && n.Body == "body" && n.Project == "p" }},
		{"same title", Patch{Title: ptr("old")}, nil, false, nil},
		{"body", Patch{Body: ptr("other")}, nil, true, func(n Note) bool { return n.Body == "other" && n.Title == "old" }},
		{"clear project", Patch{Project: ptr("")}, nil, true, func(n Note) bool { return n.Project == "" }},
		{"pin", Patch{Pinned: ptr(true)}, nil, true, func(n Note) bool { return n.Pinned }},
		{"empty patch", Patch{}, nil, false, nil},
		{"bad title", Patch{Title: ptr("a\nb")}, ErrInvalid, false, nil},
		{"would empty the note", Patch{Title: ptr(""), Body: ptr("")}, ErrInvalid, false, nil},
		{"clear title keeps body", Patch{Title: ptr("")}, nil, true, func(n Note) bool { return n.Title == "" && n.Body == "body" }},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			s, _ := testStore(t)
			orig, err := s.Create(NewNote{Title: "old", Body: "body", Project: "p"})
			if err != nil {
				t.Fatal(err)
			}
			got, changed, err := s.Update(orig.ID, tt.patch)
			if !errors.Is(err, tt.wantErr) {
				t.Fatalf("err = %v, want %v", err, tt.wantErr)
			}
			if err != nil {
				return
			}
			if changed != tt.wantChanged {
				t.Fatalf("changed = %v", changed)
			}
			if changed && !got.Updated.After(orig.Updated) {
				t.Errorf("updated time did not advance: %v", got.Updated)
			}
			if !changed && !got.Updated.Equal(orig.Updated) {
				t.Errorf("a no-op moved the updated time")
			}
			if tt.check != nil && !tt.check(got) {
				t.Fatalf("note = %+v", got)
			}
			if stored, _ := s.Get(orig.ID); stored != got {
				t.Fatalf("stored %+v != returned %+v", stored, got)
			}
		})
	}
	s, _ := testStore(t)
	if _, _, err := s.Update("ffff", Patch{Title: ptr("x")}); !errors.Is(err, ErrNotFound) {
		t.Fatalf("unknown id: %v", err)
	}
}

func TestListOrderAndFilter(t *testing.T) {
	s, _ := testStore(t)
	mk := func(n NewNote) Note {
		t.Helper()
		out, err := s.Create(n)
		if err != nil {
			t.Fatal(err)
		}
		return out
	}
	a := mk(NewNote{Title: "alpha", Project: "x/y"})
	b := mk(NewNote{Title: "beta", Body: "Needle inside"})
	c := mk(NewNote{Title: "gamma", Pinned: true})
	if _, _, err := s.Update(a.ID, Patch{Body: ptr("touched")}); err != nil {
		t.Fatal(err)
	}
	tests := []struct {
		name string
		f    Filter
		want []string
	}{
		{"pinned first then updated", Filter{}, []string{c.ID, a.ID, b.ID}},
		{"project", Filter{Project: "x/y"}, []string{a.ID}},
		{"query in body, any case", Filter{Query: "needle"}, []string{b.ID}},
		{"query in title", Filter{Query: "GAM"}, []string{c.ID}},
		{"no match", Filter{Query: "zzz"}, nil},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := s.List(tt.f)
			if err != nil {
				t.Fatal(err)
			}
			var ids []string
			for _, n := range got {
				ids = append(ids, n.ID)
			}
			if fmt.Sprint(ids) != fmt.Sprint(tt.want) {
				t.Fatalf("ids = %v, want %v", ids, tt.want)
			}
		})
	}
}

func TestDeleteAndPersistence(t *testing.T) {
	dir := t.TempDir()
	s := NewStore(dir)
	keep, _ := s.Create(NewNote{Title: "keep"})
	drop, _ := s.Create(NewNote{Title: "drop"})
	if _, err := s.Delete(drop.ID); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Delete(drop.ID); !errors.Is(err, ErrNotFound) {
		t.Fatalf("second delete: %v", err)
	}
	got, err := NewStore(dir).List(Filter{})
	if err != nil || len(got) != 1 || got[0].ID != keep.ID {
		t.Fatalf("reopened = %v err=%v", got, err)
	}
	if _, err := s.Delete(keep.ID); err != nil {
		t.Fatal(err)
	}
	if got, err := NewStore(dir).List(Filter{}); err != nil || len(got) != 0 {
		t.Fatalf("emptied store = %v err=%v", got, err)
	}
}

func TestConcurrentStores(t *testing.T) {
	dir := t.TempDir()
	var wg sync.WaitGroup
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			s := NewStore(dir)
			for j := 0; j < 5; j++ {
				if _, err := s.Create(NewNote{Title: fmt.Sprintf("n%d-%d", i, j)}); err != nil {
					t.Error(err)
				}
			}
		}()
	}
	wg.Wait()
	got, err := NewStore(dir).List(Filter{})
	if err != nil || len(got) != 40 {
		t.Fatalf("notes = %d err=%v", len(got), err)
	}
	ids := map[string]bool{}
	for _, n := range got {
		ids[n.ID] = true
	}
	if len(ids) != 40 {
		t.Fatalf("duplicate ids: %d unique", len(ids))
	}
}

const legacy = `Intro text is ignored.

## hotschmoe/protectorate
- [ ] Add GPU metrics to node agent  #p2 @codex  id:b7f3
  Optional indented notes.
  Second line.
- [~] Fix flaky test  https://github.com/hotschmoe/protectorate/issues/12  @codex  id:9c1a claim:alice/codex@2026-10-19T10:00:00Z
- [x] Done item  id:0c2d
- [ ] Item without an id
## donate: owner/public-repo
- [ ] https://github.com/owner/public-repo/issues/345  id:41ee
`

func TestMigratesLegacyBacklog(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "backlog.md"), []byte(legacy), 0o600); err != nil {
		t.Fatal(err)
	}
	s := NewStore(dir)
	got, err := s.List(Filter{})
	if err != nil {
		t.Fatal(err)
	}
	byID := map[string]Note{}
	for _, n := range got {
		byID[n.ID] = n
	}
	if len(got) != 5 {
		t.Fatalf("notes = %d: %+v", len(got), got)
	}
	tests := []struct {
		id, title, project string
		body               []string
	}{
		{"b7f3", "Add GPU metrics to node agent", "hotschmoe/protectorate", []string{"Optional indented notes.\nSecond line.", "Priority: p2", "Harness: codex"}},
		{"9c1a", "Fix flaky test", "hotschmoe/protectorate", []string{"Issue: https://github.com/hotschmoe/protectorate/issues/12", "Status: claimed by alice/codex at 2026-10-19T10:00:00Z"}},
		{"0c2d", "Done item", "hotschmoe/protectorate", []string{"Status: done"}},
		{"41ee", "https://github.com/owner/public-repo/issues/345", "donate: owner/public-repo", []string{"Issue: https://github.com/owner/public-repo/issues/345"}},
	}
	for _, tt := range tests {
		t.Run(tt.id, func(t *testing.T) {
			n, ok := byID[tt.id]
			if !ok {
				t.Fatalf("id %s not preserved", tt.id)
			}
			if n.Title != tt.title || n.Project != tt.project || n.Source != "owner" {
				t.Fatalf("note = %+v", n)
			}
			for _, w := range tt.body {
				if !strings.Contains(n.Body, w) {
					t.Errorf("body %q lacks %q", n.Body, w)
				}
			}
		})
	}
	var noID *Note
	for i := range got {
		if got[i].Title == "Item without an id" {
			noID = &got[i]
		}
	}
	if noID == nil || noID.ID == "" {
		t.Fatalf("item without an id: %+v", noID)
	}

	if _, err := os.Stat(filepath.Join(dir, "backlog.md")); !errors.Is(err, os.ErrNotExist) {
		t.Errorf("legacy file still in place: %v", err)
	}
	if b, err := os.ReadFile(filepath.Join(dir, "backlog.md.migrated")); err != nil || string(b) != legacy {
		t.Errorf("legacy copy = %q err=%v", b, err)
	}

	// a stray backlog.md after migration is not read again
	if err := os.WriteFile(filepath.Join(dir, "backlog.md"), []byte("- [ ] ghost  id:eeee\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	again, err := NewStore(dir).List(Filter{})
	if err != nil || len(again) != 5 {
		t.Fatalf("second load = %d err=%v", len(again), err)
	}
}

func TestEmptyHubHasNoNotesFile(t *testing.T) {
	dir := t.TempDir()
	got, err := NewStore(dir).List(Filter{})
	if err != nil || len(got) != 0 {
		t.Fatalf("list = %v err=%v", got, err)
	}
	if _, err := os.Stat(filepath.Join(dir, "notes.json")); !errors.Is(err, os.ErrNotExist) {
		t.Errorf("a read created notes.json: %v", err)
	}
}
