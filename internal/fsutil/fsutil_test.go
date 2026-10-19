package fsutil

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

func TestWriteFile(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "a.txt")
	tests := []struct {
		name string
		data string
		perm os.FileMode
	}{
		{"create", "one", 0o600},
		{"replace", "two-longer", 0o644},
		{"shrink", "x", 0o600},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if err := WriteFile(path, []byte(tt.data), tt.perm); err != nil {
				t.Fatal(err)
			}
			got, err := os.ReadFile(path)
			if err != nil || string(got) != tt.data {
				t.Fatalf("got %q, %v", got, err)
			}
			if runtime.GOOS != "windows" {
				fi, _ := os.Stat(path)
				if fi.Mode().Perm() != tt.perm {
					t.Fatalf("mode %v, want %v", fi.Mode().Perm(), tt.perm)
				}
			}
			ents, _ := os.ReadDir(dir)
			if len(ents) != 1 {
				t.Fatalf("temp files left: %v", ents)
			}
		})
	}
}

func TestWriteFileMissingDir(t *testing.T) {
	if err := WriteFile(filepath.Join(t.TempDir(), "no", "f"), nil, 0o600); err == nil {
		t.Fatal("expected error")
	}
}
