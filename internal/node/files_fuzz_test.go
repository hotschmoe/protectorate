package node

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func FuzzConfinedFilePaths(f *testing.F) {
	root := f.TempDir()
	outside := f.TempDir()
	if err := os.Symlink(outside, filepath.Join(root, "escape")); err != nil {
		f.Skipf("file confinement fuzzing requires symlinks: %v", err)
	}
	files, err := NewFiles(root, false)
	if err != nil {
		f.Fatal(err)
	}
	for _, path := range []string{"plain", "nested/new", "../outside", "escape/secret", "escape/not-created/child", ".", "\x00", root, outside} {
		f.Add(path)
	}
	f.Fuzz(func(t *testing.T, path string) {
		resolved, err := files.resolve(path)
		if err != nil {
			return
		}
		rel, err := filepath.Rel(root, resolved)
		if err != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
			t.Fatalf("accepted traversal %q -> %q", path, resolved)
		}
		ancestor := resolved
		for {
			real, err := filepath.EvalSymlinks(ancestor)
			if err == nil {
				rel, err := filepath.Rel(root, real)
				if err != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
					t.Fatalf("accepted symlink escape %q -> %q", path, real)
				}
				break
			}
			parent := filepath.Dir(ancestor)
			if parent == ancestor {
				t.Fatalf("accepted path without a resolvable ancestor: %q", resolved)
			}
			ancestor = parent
		}
	})
}
