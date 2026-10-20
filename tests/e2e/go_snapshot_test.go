//go:build linux

package e2e

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"
)

func snapshotNumber(row map[string]any, key string) uint64 {
	n, _ := strconv.ParseUint(fmt.Sprint(row[key]), 10, 64)
	return n
}

func snapshotReady(c *cluster, previous string) map[string]any {
	var row map[string]any
	c.eventually(func() bool {
		row = c.call("HubService", "GetSleeveRuntime", map[string]any{})
		lower, _ := row["goBuildLower"].(string)
		return lower != "" && lower != previous && row["snapshotPreparing"] != true
	})
	return row
}

func writeSnapshotIndex(t *testing.T, cache string, id, output int, size uint64, used time.Time) string {
	t.Helper()
	action, out := fmt.Sprintf("%064x", id), fmt.Sprintf("%064x", output)
	path := filepath.Join(cache, action[:2], action+"-a")
	if err := os.MkdirAll(filepath.Dir(path), 0700); err != nil {
		t.Fatal(err)
	}
	data := fmt.Sprintf("v1 %s %s %20d %20d\n", action, out, size, used.UnixNano())
	if len(data) != 175 {
		t.Fatal("invalid fixture index")
	}
	if err := os.WriteFile(path, []byte(data), 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.Chtimes(path, used, used); err != nil {
		t.Fatal(err)
	}
	return path
}

func writeSnapshotOutput(t *testing.T, cache string, id int, size int64) string {
	t.Helper()
	out := fmt.Sprintf("%064x", id)
	path := filepath.Join(cache, out[:2], out+"-d")
	if err := os.MkdirAll(filepath.Dir(path), 0700); err != nil {
		t.Fatal(err)
	}
	f, err := os.OpenFile(path, os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0600)
	if err != nil {
		t.Fatal(err)
	}
	if err := f.Truncate(size); err != nil {
		f.Close()
		t.Fatal(err)
	}
	if _, err := f.Write([]byte("fixture")); err != nil {
		f.Close()
		t.Fatal(err)
	}
	if err := f.Close(); err != nil {
		t.Fatal(err)
	}
	return path
}

func TestBinaryGoSnapshotLifecycle(t *testing.T) {
	c := newCluster(t)
	if c.call("HubService", "GetSleeveRuntime", map[string]any{})["overlays"] != true {
		t.Skip("Go sharing requires overlay-capable bwrap")
	}
	c.stop(c.hub)
	cache := filepath.Join(c.root, ".cache/go-build")
	now := time.Now()
	const outputBytes = 128 << 20
	for i := 1; i <= 1024; i++ {
		writeSnapshotOutput(t, cache, i, outputBytes)
		writeSnapshotIndex(t, cache, i, i, outputBytes, now.Add(-time.Duration(2048-i)*time.Second))
	}
	writeSnapshotIndex(t, cache, 2001, 1024, outputBytes, now.Add(-time.Second))
	const duplicateIndexes = 100000
	for i := 0; i < duplicateIndexes; i++ {
		writeSnapshotIndex(t, cache, 10000+i, 1024, outputBytes, now.Add(-time.Hour))
	}
	writeSnapshotOutput(t, cache, 3001, outputBytes)
	writeSnapshotIndex(t, cache, 3001, 3001, outputBytes, now.Add(-6*24*time.Hour))
	writeSnapshotOutput(t, cache, 3002, 2<<30)
	writeSnapshotIndex(t, cache, 3002, 3002, 2<<30, now)
	writeSnapshotOutput(t, cache, 3003, 8)
	writeSnapshotIndex(t, cache, 3003, 3003, outputBytes, now)
	executableID := fmt.Sprintf("%064x", 3004)
	executableDir := filepath.Join(cache, executableID[:2], executableID+"-d")
	if err := os.MkdirAll(executableDir, 0700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(executableDir, "probe"), []byte("snapshot"), 0700); err != nil {
		t.Fatal(err)
	}
	writeSnapshotIndex(t, cache, 3004, 3004, 8, now.Add(-2*time.Second))
	start := time.Now()
	c.boot("--sleeve-go-cache-max", "1G")
	if elapsed := time.Since(start); elapsed > 2*time.Second {
		t.Fatalf("snapshot blocked hub startup: %s", elapsed)
	}
	ready := snapshotReady(c, "")
	lower := ready["goBuildLower"].(string)
	wantBytes := uint64(7*outputBytes + 8 + (9+duplicateIndexes)*175)
	if snapshotNumber(ready, "snapshotBytes") != wantBytes || snapshotNumber(ready, "snapshotEntries") != 9+duplicateIndexes || snapshotNumber(ready, "snapshotLimitBytes") != 1<<30 {
		t.Fatalf("newest-first capped/deduplicated import: %v, want %d bytes and %d indexes", ready, wantBytes, 9+duplicateIndexes)
	}
	var inventory struct {
		Entries []map[string]any `json:"entries"`
	}
	if err := json.Unmarshal([]byte(c.cli("sleeve", "prune", "--caches", "--json")), &inventory); err != nil {
		t.Fatal(err)
	}
	found := false
	for _, entry := range inventory.Entries {
		if entry["path"] == filepath.Dir(lower) && entry["kind"] == "shared-go-cache" && snapshotNumber(entry, "bytes") == wantBytes && entry["eligible"] == true && entry["removed"] != true {
			found = true
		}
	}
	if !found {
		t.Fatalf("cache dry run omitted logical snapshot accounting: %+v", inventory)
	}
	for _, id := range []int{1, 3001, 3002, 3003} {
		name := fmt.Sprintf("%064x", id)
		if _, err := os.Stat(filepath.Join(lower, name[:2], name+"-a")); !os.IsNotExist(err) {
			t.Fatalf("imported old, oversized or mismatched index %d: %v", id, err)
		}
	}
	out := fmt.Sprintf("%064x", 1024)
	source, target := filepath.Join(cache, out[:2], out+"-d"), filepath.Join(lower, out[:2], out+"-d")
	original, err := os.Stat(source)
	if err != nil {
		t.Fatal(err)
	}
	linked, err := os.Stat(target)
	if err != nil || !os.SameFile(original, linked) {
		t.Fatalf("output data copied instead of hardlinked: %v", err)
	}
	sourceIndex := filepath.Join(cache, out[:2], out+"-a")
	targetIndex := filepath.Join(lower, out[:2], out+"-a")
	a, err := os.Stat(sourceIndex)
	if err != nil {
		t.Fatal(err)
	}
	b, err := os.Stat(targetIndex)
	if err != nil || os.SameFile(a, b) {
		t.Fatalf("action index was hardlinked: %v", err)
	}
	indexBytes, err := os.ReadFile(targetIndex)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(sourceIndex, []byte(strings.Repeat("x", 175)), 0600); err != nil {
		t.Fatal(err)
	}
	if got, err := os.ReadFile(targetIndex); err != nil || string(got) != string(indexBytes) {
		t.Fatalf("owner index rewrite changed snapshot: %v", err)
	}
	if err := os.Remove(source); err != nil {
		t.Fatal(err)
	}
	if got, err := os.Stat(target); err != nil || got.Size() != outputBytes {
		t.Fatalf("owner trim removed linked snapshot output: %v", err)
	}
	if err := os.Link(target, source); err != nil {
		t.Fatal(err)
	}
	if logs, err := os.ReadFile(c.hub.log.Name()); err == nil {
		for _, line := range strings.Split(string(logs), "\n") {
			if strings.Contains(line, "Go snapshot ") && strings.Contains(line, " ready in ") {
				t.Logf("128 GiB sparse source, %d indexes: %s", 1029+duplicateIndexes, line)
			}
		}
	}
	c.stop(c.hub)
	for _, file := range []string{filepath.Join(filepath.Dir(lower), "snapshot.json"), filepath.Join(c.home, "cache/go-build/current.json")} {
		data, err := os.ReadFile(file)
		if err != nil {
			t.Fatal(err)
		}
		var meta map[string]any
		if err := json.Unmarshal(data, &meta); err != nil {
			t.Fatal(err)
		}
		meta["prepared"] = now.Add(-48 * time.Hour).UTC().Format(time.RFC3339Nano)
		data, err = json.Marshal(meta)
		if err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(file, data, 0600); err != nil {
			t.Fatal(err)
		}
	}
	c.boot("--sleeve-go-cache-max", "1G")
	refreshed := snapshotReady(c, lower)
	c.eventually(func() bool { _, err := os.Stat(filepath.Dir(lower)); return os.IsNotExist(err) })
	lower = refreshed["goBuildLower"].(string)
	t.Run("live mounts survive refresh and prune", func(t *testing.T) {
		previous := c.t
		c.t = t
		defer func() { c.t = previous }()
		c.requireRuntime(t)
		workspace := filepath.Join(c.root, "snapshot-reader")
		if err := os.Mkdir(workspace, 0700); err != nil {
			t.Fatal(err)
		}
		c.call("SleeveService", "SpawnSleeve", map[string]any{
			"name": "snapshot-reader", "workspace": workspace, "harness": "HARNESS_SHELL",
			"args": []string{"bash", "-c", `set -eu; test -f "$GOCACHE/$CACHE_ENTRY"; touch mounted; exec sleep 60`},
			"env":  map[string]string{"CACHE_ENTRY": filepath.Join(out[:2], out+"-d")},
		})
		t.Cleanup(func() { c.rpc("SleeveService", "KillSleeve", map[string]any{"name": "snapshot-reader"}, nil) })
		c.eventually(func() bool { _, err := os.Stat(filepath.Join(workspace, "mounted")); return err == nil })
		c.stop(c.hub)
		c.boot("--sleeve-go-cache-max", "1G")
		snapshotReady(c, lower)
		c.cli("sleeve", "prune", "--caches", "--running", "--yes", "--json")
		if _, err := os.Stat(lower); err != nil {
			t.Fatalf("prune removed mounted old generation: %v", err)
		}
		pin := filepath.Join(c.home, "sleeves/snapshot-reader/go-cache.json")
		pinBytes, err := os.ReadFile(pin)
		if err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(pin, []byte("{"), 0600); err != nil {
			t.Fatal(err)
		}
		c.cli("sleeve", "prune", "--caches", "--running", "--yes", "--json")
		if _, err := os.Stat(lower); err != nil {
			t.Fatalf("unknown mount reference was not retained conservatively: %v", err)
		}
		if err := os.WriteFile(pin, pinBytes, 0600); err != nil {
			t.Fatal(err)
		}
		c.call("SleeveService", "KillSleeve", map[string]any{"name": "snapshot-reader"})
	})
	c.cli("sleeve", "prune", "--caches", "--yes", "--json")
	if _, err := os.Stat(filepath.Dir(lower)); !os.IsNotExist(err) {
		t.Fatalf("prune kept unreferenced snapshot: %v", err)
	}
	if _, err := os.Stat(source); err != nil {
		t.Fatalf("prune removed owner's linked output: %v", err)
	}
	c.stop(c.hub)
	c.boot("--sleeve-go-cache-max", "0")
	row := c.call("HubService", "GetSleeveRuntime", map[string]any{})
	if row["goBuildLower"] != nil || snapshotNumber(row, "snapshotLimitBytes") != 0 {
		t.Fatalf("disabled sharing still published a lower: %v", row)
	}
}

func TestBinaryGoSnapshotHardlinkFailure(t *testing.T) {
	c := newCluster(t)
	if c.call("HubService", "GetSleeveRuntime", map[string]any{})["overlays"] != true {
		t.Skip("Go sharing requires overlay-capable bwrap")
	}
	var source string
	for _, parent := range []string{"/dev/shm", "."} {
		dir, err := os.MkdirTemp(parent, "go-snapshot-source-")
		if err != nil {
			continue
		}
		dir, err = filepath.Abs(dir)
		if err != nil {
			t.Fatal(err)
		}
		t.Cleanup(func() { os.RemoveAll(dir) })
		output := writeSnapshotOutput(t, dir, 1, 64)
		probe := filepath.Join(c.root, "link-probe")
		if err := os.Link(output, probe); err == nil {
			os.Remove(probe)
			continue
		}
		source = dir
		writeSnapshotIndex(t, source, 1, 1, 64, time.Now())
		break
	}
	if source == "" {
		t.Skip("no writable second filesystem for hardlink failure scenario")
	}
	c.stop(c.hub)
	c.env = append(c.env, "GOCACHE="+source)
	c.boot()
	var row map[string]any
	c.eventually(func() bool {
		row = c.call("HubService", "GetSleeveRuntime", map[string]any{})
		return row["snapshotError"] != nil && row["snapshotPreparing"] != true
	})
	if row["goBuildLower"] != nil || !strings.Contains(fmt.Sprint(row["snapshotError"]), "hardlink") {
		t.Fatalf("failed links did not disable sharing: %v", row)
	}
	entries, err := os.ReadDir(filepath.Join(c.home, "cache/go-build"))
	if err != nil {
		t.Fatal(err)
	}
	for _, entry := range entries {
		if entry.IsDir() {
			t.Fatalf("failed hardlink import copied or retained data: %s", entry.Name())
		}
	}
	if logs, err := os.ReadFile(c.hub.log.Name()); err != nil || !strings.Contains(string(logs), "no copy fallback") {
		t.Fatalf("no explicit fallback log: %s: %v", logs, err)
	}
}
