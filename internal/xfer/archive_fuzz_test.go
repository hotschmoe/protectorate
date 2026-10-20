package xfer

import (
	"archive/tar"
	"bytes"
	"crypto/sha256"
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

func FuzzTransferArchive(f *testing.F) {
	for _, header := range []tar.Header{
		{Name: "nested/file", Typeflag: tar.TypeReg, Mode: 0700, Size: 3},
		{Name: "../sentinel", Typeflag: tar.TypeReg, Mode: 0600, Size: 3},
		{Name: "/absolute", Typeflag: tar.TypeReg, Mode: 0600, Size: 3},
		{Name: `nested\escape`, Typeflag: tar.TypeReg, Mode: 0600, Size: 3},
		{Name: "link", Typeflag: tar.TypeSymlink, Linkname: "../sentinel"},
		{Name: "hardlink", Typeflag: tar.TypeLink, Linkname: "../sentinel"},
	} {
		var archive bytes.Buffer
		tw := tar.NewWriter(&archive)
		if err := tw.WriteHeader(&header); err != nil {
			f.Fatal(err)
		}
		if header.Size > 0 {
			if _, err := tw.Write([]byte("new")); err != nil {
				f.Fatal(err)
			}
		}
		if err := tw.Close(); err != nil {
			f.Fatal(err)
		}
		f.Add(archive.Bytes())
	}
	f.Add([]byte("not a tar archive"))
	f.Fuzz(func(t *testing.T, archive []byte) {
		if len(archive) > 64<<10 {
			return
		}
		root := t.TempDir()
		sentinel := filepath.Join(root, "sentinel")
		if err := os.WriteFile(sentinel, []byte("original"), 0600); err != nil {
			t.Fatal(err)
		}
		dest := filepath.Join(root, "received")
		sink, err := NewSink(dest, KindDir, 0, false)
		if err != nil {
			t.Fatal(err)
		}
		defer sink.Abort()
		if err := sink.Write(Chunk{Data: archive, RawSize: uint32(len(archive))}); err != nil {
			t.Fatal(err)
		}
		err = sink.Commit(Trailer{Size: uint64(len(archive)), SHA256: fmt.Sprintf("%x", sha256.Sum256(archive))})
		if b, readErr := os.ReadFile(sentinel); readErr != nil || string(b) != "original" {
			t.Fatalf("archive overwrote a sibling: %q %v", b, readErr)
		}
		if err != nil {
			if _, statErr := os.Lstat(dest); !os.IsNotExist(statErr) {
				t.Fatalf("failed archive published a destination: %v", statErr)
			}
		} else {
			if err := filepath.WalkDir(dest, func(path string, entry os.DirEntry, err error) error {
				if err != nil {
					return err
				}
				if entry.Type()&os.ModeSymlink != 0 {
					t.Fatalf("archive published a symlink: %s", path)
				}
				return nil
			}); err != nil {
				t.Fatal(err)
			}
		}
		if leftovers, err := filepath.Glob(filepath.Join(root, tempPrefix+"*")); err != nil || len(leftovers) != 0 {
			t.Fatalf("archive leaked staging files: %v %v", leftovers, err)
		}
	})
}
