package e2e

import (
	"archive/tar"
	"bytes"
	"io"
	"os"
	"path/filepath"

	"github.com/klauspost/compress/zstd"
)

func (c *cluster) rejectTamperedBackups(archive []byte) {
	c.t.Helper()
	zr, err := zstd.NewReader(bytes.NewReader(archive))
	if err != nil {
		c.t.Fatal(err)
	}
	defer zr.Close()
	type entry struct {
		header tar.Header
		data   []byte
	}
	var entries []entry
	tr := tar.NewReader(zr)
	for {
		h, err := tr.Next()
		if err == io.EOF {
			break
		}
		if err != nil {
			c.t.Fatal(err)
		}
		b, err := io.ReadAll(tr)
		if err != nil {
			c.t.Fatal(err)
		}
		entries = append(entries, entry{*h, b})
	}
	if len(entries) < 2 {
		c.t.Fatal("backup lacks state and manifest")
	}
	for _, tc := range []struct {
		name, want string
		change     func([]entry) []entry
	}{
		{"traversal", "not a plain file", func(es []entry) []entry { es[0].header.Name = "../escaped"; return es }},
		{"duplicate", "twice", func(es []entry) []entry { return append([]entry{es[0]}, es...) }},
		{"manifest", "does not match the manifest", func(es []entry) []entry { es[0].data = append([]byte("tampered"), es[0].data...); return es }},
		{"symlink", "not a plain file", func(es []entry) []entry {
			es[0].header.Typeflag = tar.TypeSymlink
			es[0].header.Linkname = "../escaped"
			es[0].data = nil
			return es
		}},
	} {
		var buf bytes.Buffer
		zw, err := zstd.NewWriter(&buf)
		if err != nil {
			c.t.Fatal(err)
		}
		tw := tar.NewWriter(zw)
		for _, e := range tc.change(append([]entry(nil), entries...)) {
			e.header.Size = int64(len(e.data))
			if err := tw.WriteHeader(&e.header); err != nil {
				c.t.Fatal(err)
			}
			if _, err := tw.Write(e.data); err != nil {
				c.t.Fatal(err)
			}
		}
		if err := tw.Close(); err != nil {
			c.t.Fatal(err)
		}
		if err := zw.Close(); err != nil {
			c.t.Fatal(err)
		}
		path := filepath.Join(c.root, tc.name+".tar.zst")
		if err := os.WriteFile(path, buf.Bytes(), 0600); err != nil {
			c.t.Fatal(err)
		}
		dest := filepath.Join(c.root, tc.name+"-restore")
		c.rejectedCLI(tc.want, "hub", "restore", path, "--home", dest)
		if _, err := os.Stat(dest); !os.IsNotExist(err) {
			c.t.Fatalf("invalid backup created destination: %v", err)
		}
	}
	truncated := filepath.Join(c.root, "truncated.tar.zst")
	if err := os.WriteFile(truncated, archive[:len(archive)/2], 0600); err != nil {
		c.t.Fatal(err)
	}
	c.rejectedCLI("read archive", "hub", "restore", truncated, "--home", filepath.Join(c.root, "truncated-restore"))
}
