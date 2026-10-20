package sleeve

import (
	"context"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
)

func TestRenameRecord(t *testing.T) {
	tests := []struct {
		name      string
		records   map[string]State
		dirs      []string
		wantNames []string
		wantErr   bool
		moved     bool
	}{
		{name: "no record", records: map[string]State{"alpha": Running}, wantNames: []string{"alpha"}},
		{name: "stopped record", records: map[string]State{"poe": Exited}, dirs: []string{"poe"}, wantNames: []string{"envoy"}, moved: true},
		{name: "running chat record", records: map[string]State{"poe": Running, "alpha": Running}, dirs: []string{"poe", "alpha"}, wantNames: []string{"alpha", "envoy"}, moved: true},
		{name: "record without state dir", records: map[string]State{"poe": Exited}, wantNames: []string{"envoy"}},
		{name: "both records", records: map[string]State{"poe": Exited, "envoy": Running}, dirs: []string{"poe", "envoy"}, wantNames: []string{"envoy", "poe"}, wantErr: true},
		{name: "target dir in the way", records: map[string]State{"poe": Exited}, dirs: []string{"poe", "envoy"}, wantNames: []string{"poe"}, wantErr: true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			home := t.TempDir()
			cfg := Config{Home: home, CgroupRoot: filepath.Join(home, "cgroup"), UnitPrefix: "prot-test-"}
			m := &Manager{cfg: cfg, path: filepath.Join(home, "sleeves.json"), sleeves: map[string]*entry{}, pending: map[string]chan struct{}{}}
			for n, st := range tt.records {
				m.sleeves[n] = &entry{Info: Info{Name: n, Mode: ModeChat, State: st, Chat: &Chat{SessionID: "native-" + n}}}
			}
			for _, d := range tt.dirs {
				dir := cfg.sleeveDir(d)
				if err := os.MkdirAll(dir, 0o700); err != nil {
					t.Fatal(err)
				}
				if err := os.WriteFile(filepath.Join(dir, "chat.jsonl"), []byte(d), 0o600); err != nil {
					t.Fatal(err)
				}
			}
			err := m.renameRecord(context.Background(), "poe", "envoy")
			if (err != nil) != tt.wantErr {
				t.Fatalf("err = %v, wantErr %v", err, tt.wantErr)
			}
			var got []string
			for _, i := range m.List() {
				got = append(got, i.Name)
			}
			if !slices.Equal(got, tt.wantNames) {
				t.Fatalf("records = %v, want %v", got, tt.wantNames)
			}
			if tt.moved {
				b, err := os.ReadFile(filepath.Join(cfg.sleeveDir("envoy"), "chat.jsonl"))
				if err != nil || string(b) != "poe" {
					t.Fatalf("moved chat log = %q, %v; want the old sleeve's", b, err)
				}
				if _, err := os.Stat(cfg.sleeveDir("poe")); !os.IsNotExist(err) {
					t.Fatalf("old state dir still there: %v", err)
				}
			}
			if slices.Contains(tt.wantNames, "envoy") && !tt.wantErr {
				e := m.sleeves["envoy"]
				if e.Name != "envoy" || e.Chat == nil || e.Chat.SessionID != "native-poe" {
					t.Fatalf("record = %+v, chat %+v", e.Info, e.Chat)
				}
				raw, err := os.ReadFile(m.path)
				if err != nil || !strings.Contains(string(raw), `"name": "envoy"`) || strings.Contains(string(raw), `"name": "poe"`) {
					t.Fatalf("persisted = %q, %v", raw, err)
				}
			}
			if tt.wantErr {
				for _, d := range tt.dirs {
					if _, err := os.Stat(filepath.Join(cfg.sleeveDir(d), "chat.jsonl")); err != nil {
						t.Errorf("state dir %s lost: %v", d, err)
					}
				}
			}
		})
	}
}
