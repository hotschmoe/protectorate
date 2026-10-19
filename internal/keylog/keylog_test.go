package keylog

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

type clock struct{ t time.Time }

func (c *clock) now() time.Time { return c.t }

func TestReloadCompactsAndExpires(t *testing.T) {
	t0 := time.Date(2026, 10, 20, 9, 0, 0, 0, time.UTC)
	tests := []struct {
		name  string
		lines []string
		later time.Duration
		max   int
		want  map[string]string
		// wantLines is the line count of the file after Open.
		wantLines int
		// torn leaves the last line without its newline.
		torn bool
	}{
		{"nothing", nil, 0, 10, map[string]string{}, 0, false},
		{"live entries survive", []string{
			`{"key":"a","value":"1","expires":"2026-10-20T10:00:00Z"}`,
			`{"key":"b","value":"2","expires":"2026-10-20T10:00:00Z"}`,
		}, 30 * time.Minute, 10, map[string]string{"a": "1", "b": "2"}, 2, false},
		{"expired entries are dropped", []string{
			`{"key":"a","value":"1","expires":"2026-10-20T09:30:00Z"}`,
			`{"key":"b","value":"2","expires":"2026-10-20T10:00:00Z"}`,
		}, 45 * time.Minute, 10, map[string]string{"b": "2"}, 1, false},
		{"the last line per key wins", []string{
			`{"key":"a","value":"1","expires":"2026-10-20T10:00:00Z"}`,
			`{"key":"a","value":"2","expires":"2026-10-20T10:00:00Z"}`,
		}, 0, 10, map[string]string{"a": "2"}, 1, false},
		{"garbage and a torn last line are cut", []string{
			`not json`,
			`{"key":"a","value":"1","expires":"2026-10-20T10:00:00Z"}`,
			`{"key":"b","val`,
		}, 0, 10, map[string]string{"a": "1"}, 1, true},
		{"the cap keeps the latest to expire", []string{
			`{"key":"a","value":"1","expires":"2026-10-20T09:40:00Z"}`,
			`{"key":"b","value":"2","expires":"2026-10-20T09:50:00Z"}`,
			`{"key":"c","value":"3","expires":"2026-10-20T09:45:00Z"}`,
		}, 0, 2, map[string]string{"b": "2", "c": "3"}, 2, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			path := filepath.Join(t.TempDir(), "ids.jsonl")
			raw := strings.Join(tt.lines, "\n")
			if len(tt.lines) > 0 && !tt.torn {
				raw += "\n"
			}
			if raw != "" {
				if err := os.WriteFile(path, []byte(raw), 0o600); err != nil {
					t.Fatal(err)
				}
			}
			l, entries, err := Open(path, tt.max, (&clock{t0.Add(tt.later)}).now)
			if err != nil {
				t.Fatal(err)
			}
			defer l.Close()
			got := map[string]string{}
			for _, e := range entries {
				got[e.Key] = e.Value
			}
			if len(got) != len(tt.want) {
				t.Fatalf("entries = %v, want %v", got, tt.want)
			}
			for k, v := range tt.want {
				if got[k] != v {
					t.Fatalf("entries = %v, want %v", got, tt.want)
				}
			}
			after, _ := os.ReadFile(path)
			if n := strings.Count(string(after), "\n"); n != tt.wantLines {
				t.Fatalf("file has %d lines after open, want %d:\n%s", n, tt.wantLines, after)
			}
		})
	}
}

func TestAppendSurvivesReopenAndCompactsWhenBloated(t *testing.T) {
	t0 := time.Date(2026, 10, 20, 9, 0, 0, 0, time.UTC)
	clk := &clock{t0}
	path := filepath.Join(t.TempDir(), "ids.jsonl")
	l, _, err := Open(path, 100, clk.now)
	if err != nil {
		t.Fatal(err)
	}
	if err := l.Append(Entry{Key: "keep", Value: "v", Code: 3, Err: "no", Expires: t0.Add(time.Hour)}); err != nil {
		t.Fatal(err)
	}
	for i := 0; i < slack+10; i++ {
		if err := l.Append(Entry{Key: "short", Value: "x", Expires: t0.Add(time.Minute)}); err != nil {
			t.Fatal(err)
		}
	}
	clk.t = t0.Add(2 * time.Minute)
	for i := 0; i < slack+10; i++ {
		if err := l.Append(Entry{Key: "again", Value: "x", Expires: t0.Add(time.Hour)}); err != nil {
			t.Fatal(err)
		}
	}
	raw, _ := os.ReadFile(path)
	if n := strings.Count(string(raw), "\n"); n > 2+slack+4 {
		t.Fatalf("file never compacted: %d lines", n)
	}
	l.Close()
	_, entries, err := Open(path, 100, clk.now)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 2 || entries[0].Key != "again" || entries[1].Key != "keep" || entries[1].Code != 3 || entries[1].Err != "no" {
		t.Fatalf("reloaded = %+v", entries)
	}
}
