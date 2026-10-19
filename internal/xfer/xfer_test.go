package xfer

import (
	"archive/tar"
	"bytes"
	"crypto/rand"
	"errors"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func send(t *testing.T, src string, compress bool, sink *Sink) (wire int, compressed int, tr Trailer) {
	t.Helper()
	s, err := OpenSource(src, compress)
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	for {
		c, err := s.Next()
		if err == io.EOF {
			break
		}
		if err != nil {
			t.Fatal(err)
		}
		wire += len(c.Data)
		if c.Compressed {
			compressed++
		}
		if err := sink.Write(c); err != nil {
			t.Fatal(err)
		}
	}
	tr, err = s.Trailer()
	if err != nil {
		t.Fatal(err)
	}
	return wire, compressed, tr
}

func TestFileRoundTrip(t *testing.T) {
	random := make([]byte, 3<<20)
	_, _ = rand.Read(random)
	tests := []struct {
		name         string
		file         string
		data         []byte
		compress     bool
		wantCompress bool
	}{
		{"text compresses", "a.txt", bytes.Repeat([]byte("hello world\n"), 400000), true, true},
		{"compression off", "a.txt", bytes.Repeat([]byte("hello world\n"), 400000), false, false},
		{"random does not shrink", "a.bin", random, true, false},
		{"known compressed extension", "a.zip", bytes.Repeat([]byte("x"), 1<<20), true, false},
		{"empty", "e.txt", nil, true, false},
		{"spans chunks", "big.txt", bytes.Repeat([]byte("0123456789"), ChunkSize/5), true, true},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			dir := t.TempDir()
			src := filepath.Join(dir, tc.file)
			if err := os.WriteFile(src, tc.data, 0o640); err != nil {
				t.Fatal(err)
			}
			dest := filepath.Join(dir, "sub", "out")
			sink, err := NewSink(dest, KindFile, 0o640, false)
			if err != nil {
				t.Fatal(err)
			}
			wire, nc, tr := send(t, src, tc.compress, sink)
			if (nc > 0) != tc.wantCompress {
				t.Fatalf("compressed chunks = %d, want compression %v", nc, tc.wantCompress)
			}
			if tc.wantCompress && wire >= len(tc.data) {
				t.Fatalf("wire %d not smaller than %d", wire, len(tc.data))
			}
			if err := sink.Commit(tr); err != nil {
				t.Fatal(err)
			}
			got, err := os.ReadFile(dest)
			if err != nil || !bytes.Equal(got, tc.data) {
				t.Fatalf("content mismatch: %v", err)
			}
			if runtime.GOOS != "windows" {
				if fi, _ := os.Stat(dest); fi.Mode().Perm() != 0o640 {
					t.Fatalf("mode = %v", fi.Mode())
				}
			}
			left, _ := filepath.Glob(filepath.Join(dir, "sub", tempPrefix+"*"))
			if len(left) != 0 {
				t.Fatalf("temp files left: %v", left)
			}
		})
	}
}

func TestCommitVerification(t *testing.T) {
	tests := []struct {
		name   string
		mutate func(*Trailer)
		want   string
	}{
		{"bad hash", func(tr *Trailer) { tr.SHA256 = strings.Repeat("0", 64) }, "sha256 mismatch"},
		{"bad size", func(tr *Trailer) { tr.Size++ }, "size mismatch"},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			dir := t.TempDir()
			src := filepath.Join(dir, "in")
			_ = os.WriteFile(src, []byte("payload"), 0o600)
			dest := filepath.Join(dir, "out")
			sink, err := NewSink(dest, KindFile, 0o600, false)
			if err != nil {
				t.Fatal(err)
			}
			_, _, tr := send(t, src, true, sink)
			tc.mutate(&tr)
			if err := sink.Commit(tr); err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("err = %v, want %q", err, tc.want)
			}
			if _, err := os.Stat(dest); err == nil {
				t.Fatal("destination written despite failed verification")
			}
			if left, _ := filepath.Glob(filepath.Join(dir, tempPrefix+"*")); len(left) != 0 {
				t.Fatalf("temp files left: %v", left)
			}
		})
	}
}

func TestOverwrite(t *testing.T) {
	dir := t.TempDir()
	dest := filepath.Join(dir, "out")
	_ = os.WriteFile(dest, []byte("old"), 0o600)
	if _, err := NewSink(dest, KindFile, 0o600, false); !errors.Is(err, ErrExists) {
		t.Fatalf("err = %v, want ErrExists", err)
	}
	if _, err := NewSink(dest, KindDir, 0, true); err == nil {
		t.Fatal("directory over file accepted")
	}
	src := filepath.Join(dir, "in")
	_ = os.WriteFile(src, []byte("new"), 0o600)
	sink, err := NewSink(dest, KindFile, 0o600, true)
	if err != nil {
		t.Fatal(err)
	}
	_, _, tr := send(t, src, true, sink)
	if err := sink.Commit(tr); err != nil {
		t.Fatal(err)
	}
	if b, _ := os.ReadFile(dest); string(b) != "new" {
		t.Fatalf("dest = %q", b)
	}
}

func TestDirRoundTrip(t *testing.T) {
	dir := t.TempDir()
	src := filepath.Join(dir, "src")
	_ = os.MkdirAll(filepath.Join(src, "a", "b"), 0o755)
	_ = os.MkdirAll(filepath.Join(src, "empty"), 0o755)
	_ = os.WriteFile(filepath.Join(src, "top.txt"), []byte("top"), 0o644)
	_ = os.WriteFile(filepath.Join(src, "a", "b", "deep.txt"), bytes.Repeat([]byte("deep"), 5000), 0o644)
	skipped := uint32(0)
	if err := os.Symlink("top.txt", filepath.Join(src, "link")); err == nil {
		skipped = 1
	}
	dest := filepath.Join(dir, "dest")
	_ = os.MkdirAll(dest, 0o755)
	_ = os.WriteFile(filepath.Join(dest, "stale.txt"), []byte("stale"), 0o644)

	if _, err := NewSink(dest, KindDir, 0, false); !errors.Is(err, ErrExists) {
		t.Fatalf("err = %v, want ErrExists", err)
	}
	sink, err := NewSink(dest, KindDir, 0, true)
	if err != nil {
		t.Fatal(err)
	}
	_, _, tr := send(t, src, true, sink)
	if tr.Skipped != skipped {
		t.Fatalf("skipped = %d, want %d", tr.Skipped, skipped)
	}
	if err := sink.Commit(tr); err != nil {
		t.Fatal(err)
	}
	for _, want := range []struct{ p, c string }{{"top.txt", "top"}, {"a/b/deep.txt", strings.Repeat("deep", 5000)}} {
		b, err := os.ReadFile(filepath.Join(dest, filepath.FromSlash(want.p)))
		if err != nil || string(b) != want.c {
			t.Fatalf("%s: %q %v", want.p, b, err)
		}
	}
	if fi, err := os.Stat(filepath.Join(dest, "empty")); err != nil || !fi.IsDir() {
		t.Fatalf("empty dir missing: %v", err)
	}
	if _, err := os.Stat(filepath.Join(dest, "stale.txt")); err == nil {
		t.Fatal("stale file survived overwrite")
	}
	if _, err := os.Lstat(filepath.Join(dest, "link")); err == nil {
		t.Fatal("symlink was transferred")
	}
	if left, _ := filepath.Glob(filepath.Join(dir, tempPrefix+"*")); len(left) != 0 {
		t.Fatalf("temp entries left: %v", left)
	}
}

func TestUnpackRejectsUnsafeEntries(t *testing.T) {
	tests := []struct {
		name string
		hdr  tar.Header
	}{
		{"parent escape", tar.Header{Typeflag: tar.TypeReg, Name: "../evil", Mode: 0o644}},
		{"absolute", tar.Header{Typeflag: tar.TypeReg, Name: "/etc/evil", Mode: 0o644}},
		{"symlink", tar.Header{Typeflag: tar.TypeSymlink, Name: "l", Linkname: "/etc"}},
		{"backslash", tar.Header{Typeflag: tar.TypeReg, Name: `a\..\..\evil`, Mode: 0o644}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			var buf bytes.Buffer
			tw := tar.NewWriter(&buf)
			if err := tw.WriteHeader(&tc.hdr); err != nil {
				t.Fatal(err)
			}
			tw.Close()
			f, _ := os.CreateTemp(t.TempDir(), "tarball")
			defer f.Close()
			_, _ = f.Write(buf.Bytes())
			if err := unpack(f, t.TempDir()); err == nil {
				t.Fatal("unsafe entry accepted")
			}
		})
	}
}

func TestWriteRejectsBadChunks(t *testing.T) {
	tests := []struct {
		name  string
		chunk Chunk
	}{
		{"wrong raw size", Chunk{Data: []byte("abc"), RawSize: 5}},
		{"garbage zstd", Chunk{Data: []byte("not zstd"), Compressed: true, RawSize: 8}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			sink, err := NewSink(filepath.Join(t.TempDir(), "o"), KindFile, 0o600, false)
			if err != nil {
				t.Fatal(err)
			}
			defer sink.Abort()
			if err := sink.Write(tc.chunk); err == nil {
				t.Fatal("bad chunk accepted")
			}
		})
	}
}
