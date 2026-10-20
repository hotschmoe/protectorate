package e2e

import (
	"archive/tar"
	"bytes"
	"encoding/json"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/hotschmoe/protectorate/internal/backup"
	"github.com/klauspost/compress/zstd"
)

func TestBinaryBackupManifestCoverage(t *testing.T) {
	c := newCluster(t)
	c.stop(c.hub)
	archive := strings.Split(strings.TrimSpace(c.cli("hub", "backup", "--to", filepath.Join(c.root, "backups"))), "\n")[0]
	f, err := os.Open(archive)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	zr, err := zstd.NewReader(f)
	if err != nil {
		t.Fatal(err)
	}
	defer zr.Close()
	type entry struct {
		header tar.Header
		body   []byte
	}
	var entries []entry
	tr := tar.NewReader(zr)
	for {
		h, err := tr.Next()
		if err == io.EOF {
			break
		}
		if err != nil {
			t.Fatal(err)
		}
		body, err := io.ReadAll(tr)
		if err != nil {
			t.Fatal(err)
		}
		entries = append(entries, entry{*h, body})
	}
	if len(entries) < 3 || entries[len(entries)-1].header.Name != backup.ManifestName {
		t.Fatal("backup lacks multiple state files and a trailing manifest")
	}
	var manifest backup.Manifest
	if err := json.Unmarshal(entries[len(entries)-1].body, &manifest); err != nil {
		t.Fatal(err)
	}
	for _, tc := range []struct {
		name, want string
		duplicate  bool
	}{
		{"duplicate-omits-file", "manifest lists", true},
		{"changed-permissions", "does not match the manifest", false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			previous := c.t
			c.t = t
			defer func() { c.t = previous }()
			changed := append([]entry(nil), entries...)
			m := manifest
			m.Files = append([]backup.File(nil), manifest.Files...)
			if tc.duplicate {
				m.Files[1] = m.Files[0]
				changed[1].body = []byte("unchecked replacement state")
			} else {
				changed[0].header.Mode ^= 0o044
			}
			body, err := json.Marshal(m)
			if err != nil {
				t.Fatal(err)
			}
			changed[len(changed)-1].body = body
			var buf bytes.Buffer
			zw, err := zstd.NewWriter(&buf)
			if err != nil {
				t.Fatal(err)
			}
			tw := tar.NewWriter(zw)
			for _, e := range changed {
				e.header.Size = int64(len(e.body))
				if err := tw.WriteHeader(&e.header); err != nil {
					t.Fatal(err)
				}
				if _, err := tw.Write(e.body); err != nil {
					t.Fatal(err)
				}
			}
			if err := tw.Close(); err != nil {
				t.Fatal(err)
			}
			if err := zw.Close(); err != nil {
				t.Fatal(err)
			}
			path := filepath.Join(c.root, tc.name+".tar.zst")
			if err := os.WriteFile(path, buf.Bytes(), 0600); err != nil {
				t.Fatal(err)
			}
			dest := filepath.Join(c.root, tc.name+"-restore")
			c.rejectedCLI(tc.want, "hub", "restore", path, "--home", dest)
			if _, err := os.Stat(dest); !os.IsNotExist(err) {
				t.Fatalf("invalid manifest created a restore destination: %v", err)
			}
		})
	}
}
