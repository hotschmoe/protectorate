package auth

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"
)

func TestReadPageAcrossFiles(t *testing.T) {
	dir := t.TempDir()
	a := NewAudit(dir)
	defer a.Close()
	for i := range 30 {
		month := 9 + i/10
		out := OutcomeOK
		if i%7 == 0 {
			out = OutcomeDenied
		}
		a.Write(Entry{Time: time.Date(2026, time.Month(month), 1+i%10, 0, 0, 0, 0, time.UTC), Actor: fmt.Sprintf("token:t%d", i%3), Procedure: "/protectorate.v1.SessionService/KillSession", Subject: fmt.Sprint(i), Outcome: out})
	}
	subjects := func(es []Entry) string {
		var s []string
		for _, e := range es {
			s = append(s, e.Subject)
		}
		return strings.Join(s, ",")
	}
	tests := []struct {
		name string
		q    Query
		want string
		next string
	}{
		{"zero", Query{}, "", ""},
		{"negative", Query{Limit: -1}, "", ""},
		{"newest", Query{Limit: 3}, "29,28,27", "audit-2026-11.jsonl:7"},
		{"to the month start", Query{Limit: 10}, "29,28,27,26,25,24,23,22,21,20", "audit-2026-10.jsonl:*"},
		{"across months", Query{Limit: 12}, "29,28,27,26,25,24,23,22,21,20,19,18", "audit-2026-10.jsonl:8"},
		{"everything", Query{Limit: 100}, "29,28,27,26,25,24,23,22,21,20,19,18,17,16,15,14,13,12,11,10,9,8,7,6,5,4,3,2,1,0", ""},
		{"cursor", Query{Limit: 2, Before: "audit-2026-10.jsonl:8"}, "17,16", "audit-2026-10.jsonl:6"},
		{"whole-file cursor", Query{Limit: 2, Before: "audit-2026-10.jsonl:*"}, "19,18", "audit-2026-10.jsonl:8"},
		{"outcome", Query{Limit: 10, Outcome: OutcomeDenied}, "28,21,14,7,0", ""},
		{"actor and paging", Query{Limit: 2, Actor: "T1"}, "28,25", "audit-2026-11.jsonl:5"},
		{"procedure", Query{Limit: 1, Procedure: "killsession"}, "29", "audit-2026-11.jsonl:9"},
		{"subject", Query{Limit: 5, Subject: "2"}, "29,28,27,26,25", "audit-2026-11.jsonl:5"},
		{"no match", Query{Limit: 5, Actor: "nobody"}, "", ""},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := ReadPage(dir, tt.q)
			if err != nil {
				t.Fatal(err)
			}
			if subjects(got.Entries) != tt.want || got.Next != tt.next {
				t.Fatalf("page = %s next %q, want %s next %q", subjects(got.Entries), got.Next, tt.want, tt.next)
			}
		})
	}
	for _, bad := range []string{"nope", "audit-2026-10.jsonl", "audit-2020-01.jsonl:1", "audit-2026-10.jsonl:-2", "audit-2026-10.jsonl:x", "../audit-2026-10.jsonl:1"} {
		if _, err := ReadPage(dir, Query{Limit: 1, Before: bad}); !errors.Is(err, ErrCursor) {
			t.Errorf("cursor %q: %v", bad, err)
		}
	}
}

func TestReadPageSkipsATornLastLine(t *testing.T) {
	dir := t.TempDir()
	a := NewAudit(dir)
	a.Write(Entry{Time: time.Date(2026, 10, 1, 0, 0, 0, 0, time.UTC), Actor: "owner", Procedure: "p", Subject: "whole", Outcome: OutcomeOK})
	a.Close()
	f, err := os.OpenFile(filepath.Join(dir, "audit-2026-10.jsonl"), os.O_APPEND|os.O_WRONLY, 0o600)
	if err != nil {
		t.Fatal(err)
	}
	f.WriteString(`{"time":"2026-10-02T00:00:00Z","actor":"owner","subj`)
	f.Close()
	got, err := ReadPage(dir, Query{Limit: 5})
	if err != nil || len(got.Entries) != 1 || got.Entries[0].Subject != "whole" {
		t.Fatalf("page = %+v %v", got, err)
	}
	if p, _ := (*Audit)(nil).Page(Query{Limit: 5}); len(p.Entries) != 0 {
		t.Fatalf("nil audit = %+v", p)
	}
}

// oldestFirst returns the last n entries in the order they were written.
func oldestFirst(t *testing.T, dir string, n int) []Entry {
	t.Helper()
	p, err := ReadPage(dir, Query{Limit: n})
	if err != nil {
		t.Fatal(err)
	}
	slices.Reverse(p.Entries)
	return p.Entries
}
