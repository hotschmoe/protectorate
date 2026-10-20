package e2e

import (
	"archive/tar"
	"bytes"
	"crypto/sha256"
	"encoding/base64"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestBinaryTransferFailureAtomicity(t *testing.T) {
	c := newCluster(t)
	original := filepath.Join(c.root, "original")
	if err := os.WriteFile(original, []byte("keep original"), 0700); err != nil {
		t.Fatal(err)
	}
	c.cli("node", "put", "peer", original, "stable")
	c.rejectedCLI("already_exists", "node", "put", "peer", original, "stable")
	remote := filepath.Join(c.root, "node", "node-files", "stable")
	for _, probe := range []struct {
		name   string
		mutate func(map[string]any)
		status int
	}{
		{"digest", func(in map[string]any) {
			in["trailer"] = map[string]any{"size": "11", "sha256": strings.Repeat("0", 64)}
		}, 500},
		{"size", func(in map[string]any) {
			in["trailer"] = map[string]any{"size": "12", "sha256": fmt.Sprintf("%x", sha256.Sum256([]byte("replacement")))}
		}, 500},
		{"raw size", func(in map[string]any) { in["chunk"].(map[string]any)["rawSize"] = 99 }, 400},
		{"compression", func(in map[string]any) { in["chunk"].(map[string]any)["compressed"] = true }, 400},
		{"trailer missing", func(in map[string]any) { delete(in, "trailer") }, 400},
	} {
		t.Run(probe.name, func(t *testing.T) {
			in := map[string]any{"name": "peer", "transferId": "failure-probe-0001", "path": "stable", "kind": "FILE_KIND_FILE", "overwrite": true, "chunk": map[string]any{"data": base64.StdEncoding.EncodeToString([]byte("replacement")), "rawSize": 11}, "last": true, "trailer": map[string]any{"size": "11", "sha256": fmt.Sprintf("%x", sha256.Sum256([]byte("replacement")))}}
			probe.mutate(in)
			if status := c.rpc("NodeService", "PutFile", in, nil); status != probe.status {
				t.Fatalf("corrupt upload status=%d want=%d", status, probe.status)
			}
			if b, err := os.ReadFile(remote); err != nil || string(b) != "keep original" {
				t.Fatalf("failed transfer replaced the destination: %q %v", b, err)
			}
			if status := c.rpc("NodeService", "PutFile", map[string]any{"name": "peer", "transferId": "failure-probe-0001", "offset": "11"}, nil); status != 404 {
				t.Fatalf("failed transfer retained an active session: %d", status)
			}
		})
	}
	c.call("NodeService", "PutFile", map[string]any{"name": "peer", "transferId": "offset-probe-0001", "path": "stable", "kind": "FILE_KIND_FILE", "overwrite": true, "chunk": map[string]any{"data": "YQ==", "rawSize": 1}})
	if status := c.rpc("NodeService", "PutFile", map[string]any{"name": "peer", "transferId": "offset-probe-0001", "offset": "2"}, nil); status != 400 {
		t.Fatalf("out-of-order upload accepted: %d", status)
	}
	if b, err := os.ReadFile(remote); err != nil || string(b) != "keep original" {
		t.Fatalf("out-of-order upload replaced the destination: %q %v", b, err)
	}
	for i, entry := range []tar.Header{
		{Name: "../escaped", Typeflag: tar.TypeReg, Mode: 0600},
		{Name: "link", Typeflag: tar.TypeSymlink, Linkname: "../stable"},
		{Name: "hardlink", Typeflag: tar.TypeLink, Linkname: "../stable"},
	} {
		var archive bytes.Buffer
		tw := tar.NewWriter(&archive)
		if err := tw.WriteHeader(&entry); err != nil {
			t.Fatal(err)
		}
		if err := tw.Close(); err != nil {
			t.Fatal(err)
		}
		data := archive.Bytes()
		if status := c.rpc("NodeService", "PutFile", map[string]any{"name": "peer", "transferId": fmt.Sprintf("archive-probe-%04d", i), "path": "tree", "kind": "FILE_KIND_DIRECTORY", "chunk": map[string]any{"data": base64.StdEncoding.EncodeToString(data), "rawSize": len(data)}, "last": true, "trailer": map[string]any{"size": fmt.Sprint(len(data)), "sha256": fmt.Sprintf("%x", sha256.Sum256(data))}}, nil); status != 500 {
			t.Fatalf("unsafe archive accepted: %v status=%d", entry, status)
		}
		if _, err := os.Stat(filepath.Join(c.root, "node", "node-files", "tree")); !os.IsNotExist(err) {
			t.Fatalf("unsafe archive published a partial tree: %v", err)
		}
	}
	if _, err := os.Stat(filepath.Join(c.root, "node", "node-files", "escaped")); !os.IsNotExist(err) {
		t.Fatalf("archive escaped the destination: %v", err)
	}
	if leftovers, err := filepath.Glob(filepath.Join(c.root, "node", "node-files", ".protectorate-put-*")); err != nil || len(leftovers) != 0 {
		t.Fatalf("failed uploads leaked temporary files: %v %v", leftovers, err)
	}
	tree := filepath.Join(c.root, "source-tree")
	if err := os.MkdirAll(filepath.Join(tree, "nested"), 0700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(tree, "nested", "run"), []byte("#!/bin/sh\nprintf transfer-recovered\n"), 0750); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(original, filepath.Join(tree, "skip-link")); err != nil {
		t.Fatal(err)
	}
	c.cli("node", "put", "peer", tree, "tree")
	download := filepath.Join(c.root, "download-tree")
	c.cli("node", "get", "peer", "tree", download)
	if b, err := os.ReadFile(filepath.Join(download, "nested", "run")); err != nil || !strings.Contains(string(b), "transfer-recovered") {
		t.Fatalf("directory recovery roundtrip: %q %v", b, err)
	}
	if info, err := os.Stat(filepath.Join(download, "nested", "run")); err != nil || info.Mode().Perm() != 0750 {
		t.Fatalf("directory transfer lost executable permissions: %v %v", info, err)
	}
	if _, err := os.Lstat(filepath.Join(download, "skip-link")); !os.IsNotExist(err) {
		t.Fatalf("directory transfer followed a source symlink: %v", err)
	}
	if err := os.WriteFile(original, []byte("verified replacement"), 0700); err != nil {
		t.Fatal(err)
	}
	c.cli("node", "put", "peer", original, "stable", "--force")
	if b, err := os.ReadFile(remote); err != nil || string(b) != "verified replacement" {
		t.Fatalf("successful overwrite did not replace the destination: %q %v", b, err)
	}
}
