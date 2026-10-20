package backup

import (
	"archive/tar"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sort"
	"strings"
	"testing"
	"time"

	"github.com/klauspost/compress/zstd"
)

func write(t *testing.T, home, rel, content string, mode os.FileMode) {
	t.Helper()
	p := filepath.Join(home, filepath.FromSlash(rel))
	if err := os.MkdirAll(filepath.Dir(p), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(p, []byte(content), mode); err != nil {
		t.Fatal(err)
	}
}

// fixtureHome builds a state directory with files that belong in a backup and
// files that must stay out, and a stopped WAL-mode SQLite database.
func fixtureHome(t *testing.T) (home string, dbFile string) {
	t.Helper()
	home = t.TempDir()
	for rel, content := range map[string]string{
		"token":                                           "secret-token\n",
		"tokens.json":                                     "{\"tokens\": []}\n",
		"nodes.json":                                      "{\"nodes\": []}\n",
		"sleeves.json":                                    "{\"sleeves\": []}\n",
		"leases.json":                                     "{\"leases\": []}\n",
		"desk-policies.json":                              "{}\n",
		"desk-use.json":                                   "{}\n",
		"mailbox.jsonl":                                   "{\"m\":1}\n{\"m\":2}\n",
		"audit/2026-10-20.jsonl":                          "{}\n",
		"tailnet/config.json":                             "{}\n",
		"tailnet/derp_private.key":                        "derpkey\n",
		"tailnet/derp_cert.pem":                           "cert\n",
		"tailnet/noise_private.key":                       "noise\n",
		"tailnet/tsnet-hub/tailscaled.state":              "state\n",
		"tailnet/headscale.log":                           "log noise\n",
		"tailnet/headscale.yaml":                          "regenerated\n",
		"tailnet/apikey":                                  "apikey\n",
		"tailnet/tsnet-hub/tailscaled.log1.txt":           "log\n",
		"jobs/20261020-010101-aaaaaaaa/job.json":          "{}\n",
		"jobs/20261020-010101-aaaaaaaa/job.log":           "out\n",
		"jobs/20261020-010101-aaaaaaaa/artifacts/big.bin": "artifact\n",
		"jobs/request-ids.jsonl":                          "{}\n",
		"sleeves/api/chat.jsonl":                          "{}\n",
		"sleeves/api/chat.jsonl.request-ids":              "{}\n",
		"sleeves/api/home/.protectorate-token":            "sleeve secret\n",
		"sleeves/api/workspaces/repo/main.go":             "package main\n",
		"bin/headscale-0.29.5":                            "binary\n",
		"auth/claude/.credentials.json":                   "login\n",
		"node-files/x":                                    "x\n",
	} {
		mode := os.FileMode(0o644)
		if rel == "token" || strings.HasSuffix(rel, ".key") {
			mode = 0o600
		}
		write(t, home, rel, content, mode)
	}
	dbFile = filepath.Join(home, "tailnet", "db.sqlite")
	pythonSQLite(t, dbFile, `db.execute("PRAGMA journal_mode=WAL"); db.execute("CREATE TABLE nodes (id INTEGER PRIMARY KEY, name TEXT)"); db.executemany("INSERT INTO nodes (name) VALUES (?)", [("alpha",),("beta",)]); db.commit()`)
	return home, dbFile
}

func pythonSQLite(t *testing.T, dbFile, script string) string {
	t.Helper()
	python, err := exec.LookPath("python3")
	if err != nil {
		t.Skip("SQLite fixture requires python3")
	}
	out, err := exec.Command(python, "-c", "import sqlite3,sys; db=sqlite3.connect(sys.argv[1]); "+script, dbFile).CombinedOutput()
	if err != nil {
		t.Fatalf("SQLite fixture: %v: %s", err, out)
	}
	return strings.TrimSpace(string(out))
}

func nodeNames(t *testing.T, dbFile string) []string {
	t.Helper()
	return strings.Split(pythonSQLite(t, dbFile, `assert db.execute("PRAGMA integrity_check").fetchone()[0] == "ok"; print("\n".join(r[0] for r in db.execute("SELECT name FROM nodes ORDER BY id")))`), "\n")
}

func listFiles(t *testing.T, root string) []string {
	t.Helper()
	var out []string
	err := filepath.WalkDir(root, func(p string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.Type().IsRegular() {
			rel, _ := filepath.Rel(root, p)
			out = append(out, filepath.ToSlash(rel))
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	sort.Strings(out)
	return out
}

func TestRoundTripStoppedDatabase(t *testing.T) {
	home, _ := fixtureHome(t)
	dest := t.TempDir()
	file, err := Create(context.Background(), Options{Home: home, Dir: dest, Version: "v-test", Now: func() time.Time { return time.Date(2026, 10, 20, 9, 30, 0, 0, time.UTC) }})
	if err != nil {
		t.Fatal(err)
	}
	if filepath.Base(file) != "protectorate-hub-20261020T093000Z.tar.zst" {
		t.Fatalf("archive name = %s", file)
	}
	for _, p := range []string{file, file + ".sha256"} {
		if fi, err := os.Stat(p); err != nil || (runtime.GOOS != "windows" && fi.Mode().Perm() != 0o600) {
			t.Fatalf("%s: %v %v", p, fi, err)
		}
	}
	if left := listFiles(t, dest); len(left) != 2 {
		t.Fatalf("temporary files left in the backup dir: %v", left)
	}

	restored := filepath.Join(t.TempDir(), "restored")
	m, err := Restore(context.Background(), file, restored)
	if err != nil {
		t.Fatal(err)
	}
	if m.Version != "v-test" || m.Home != home {
		t.Fatalf("manifest = %+v", m)
	}
	want := []string{
		"audit/2026-10-20.jsonl",
		"desk-policies.json", "desk-use.json",
		"jobs/20261020-010101-aaaaaaaa/job.json", "jobs/20261020-010101-aaaaaaaa/job.log", "jobs/request-ids.jsonl",
		"leases.json", "mailbox.jsonl", "nodes.json",
		"sleeves.json", "sleeves/api/chat.jsonl", "sleeves/api/chat.jsonl.request-ids",
		"tailnet/config.json", "tailnet/db.sqlite", "tailnet/derp_cert.pem", "tailnet/derp_private.key", "tailnet/noise_private.key",
		"tailnet/tsnet-hub/tailscaled.state",
		"token", "tokens.json",
	}
	got := listFiles(t, restored)
	got = removeAll(got, "tailnet/db.sqlite-shm", "tailnet/db.sqlite-wal")
	if strings.Join(got, "\n") != strings.Join(want, "\n") {
		t.Fatalf("restored files:\n%s\nwant:\n%s", strings.Join(got, "\n"), strings.Join(want, "\n"))
	}
	if b, _ := os.ReadFile(filepath.Join(restored, "mailbox.jsonl")); string(b) != "{\"m\":1}\n{\"m\":2}\n" {
		t.Fatalf("mailbox = %q", b)
	}
	if runtime.GOOS != "windows" {
		if fi, _ := os.Stat(filepath.Join(restored, "token")); fi.Mode().Perm() != 0o600 {
			t.Fatalf("token mode = %v", fi.Mode().Perm())
		}
	}
	names := nodeNames(t, filepath.Join(restored, "tailnet", "db.sqlite"))
	if len(names) < 2 || names[0] != "alpha" || names[1] != "beta" {
		t.Fatalf("restored database lost its first rows: %v", names)
	}
	for _, n := range names[2:] {
		if n != "busy" {
			t.Fatalf("restored database = %v", names)
		}
	}
}

func removeAll(list []string, drop ...string) []string {
	var out []string
	for _, s := range list {
		skip := false
		for _, d := range drop {
			skip = skip || s == d
		}
		if !skip {
			out = append(out, s)
		}
	}
	return out
}

func TestRestoreRefusesNonEmptyHome(t *testing.T) {
	home, _ := fixtureHome(t)
	file, err := Create(context.Background(), Options{Home: home, Dir: t.TempDir()})
	if err != nil {
		t.Fatal(err)
	}
	tests := []struct {
		name  string
		setup func(dir string)
		ok    bool
	}{
		{"missing directory", func(string) {}, true},
		{"empty directory", func(dir string) { os.MkdirAll(dir, 0o700) }, true},
		{"directory with a file", func(dir string) { write(t, dir, "token", "other", 0o600) }, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			dir := filepath.Join(t.TempDir(), "home")
			tt.setup(dir)
			_, err := Restore(context.Background(), file, dir)
			if (err == nil) != tt.ok {
				t.Fatalf("err = %v", err)
			}
			if !tt.ok {
				if b, _ := os.ReadFile(filepath.Join(dir, "token")); string(b) != "other" {
					t.Fatalf("refused restore touched the home: %q", b)
				}
			}
		})
	}
}

// rewrite builds an archive from the one at src by letting edit change the
// entries; it keeps the original manifest unless edit changes it.
func rewrite(t *testing.T, src string, edit func(name string, body []byte) (string, []byte, bool)) string {
	t.Helper()
	f, err := os.Open(src)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	zr, err := zstd.NewReader(f)
	if err != nil {
		t.Fatal(err)
	}
	defer zr.Close()
	tr := tar.NewReader(zr)
	var out bytes.Buffer
	zw, _ := zstd.NewWriter(&out)
	tw := tar.NewWriter(zw)
	for {
		h, err := tr.Next()
		if errors.Is(err, io.EOF) {
			break
		}
		if err != nil {
			t.Fatal(err)
		}
		body, _ := io.ReadAll(tr)
		name, nb, keep := edit(h.Name, body)
		if !keep {
			continue
		}
		if err := tw.WriteHeader(&tar.Header{Name: name, Mode: h.Mode, Size: int64(len(nb)), Typeflag: tar.TypeReg}); err != nil {
			t.Fatal(err)
		}
		tw.Write(nb)
	}
	tw.Close()
	zw.Close()
	dst := filepath.Join(t.TempDir(), "tampered.tar.zst")
	if err := os.WriteFile(dst, out.Bytes(), 0o600); err != nil {
		t.Fatal(err)
	}
	return dst
}

func TestRestoreRejectsBadArchives(t *testing.T) {
	home, _ := fixtureHome(t)
	good, err := Create(context.Background(), Options{Home: home, Dir: t.TempDir()})
	if err != nil {
		t.Fatal(err)
	}
	same := func(n string, b []byte) (string, []byte, bool) { return n, b, true }
	tests := []struct {
		name string
		file func(t *testing.T) string
		want string
	}{
		{"a changed byte", func(t *testing.T) string {
			return rewrite(t, good, func(n string, b []byte) (string, []byte, bool) {
				if n == "tokens.json" {
					return n, append(b[:len(b)-2:len(b)-2], 'X', '\n'), true
				}
				return same(n, b)
			})
		}, "does not match the manifest"},
		{"a missing file", func(t *testing.T) string {
			return rewrite(t, good, func(n string, b []byte) (string, []byte, bool) { return n, b, n != "tokens.json" })
		}, "manifest lists"},
		{"an extra file", func(t *testing.T) string {
			return rewrite(t, good, func(n string, b []byte) (string, []byte, bool) {
				if n == "tokens.json" {
					return "extra", b, true
				}
				return same(n, b)
			})
		}, "manifest lists"},
		{"no manifest", func(t *testing.T) string {
			return rewrite(t, good, func(n string, b []byte) (string, []byte, bool) { return n, b, n != ManifestName })
		}, "no manifest"},
		{"a path that escapes the home", func(t *testing.T) string {
			return rewrite(t, good, func(n string, b []byte) (string, []byte, bool) {
				if n == "tokens.json" {
					return "../evil", b, true
				}
				return same(n, b)
			})
		}, "not a plain file"},
		{"a wrong sidecar digest", func(t *testing.T) string {
			p := filepath.Join(t.TempDir(), "a.tar.zst")
			raw, _ := os.ReadFile(good)
			os.WriteFile(p, raw, 0o600)
			os.WriteFile(p+".sha256", []byte(strings.Repeat("0", 64)+"  a.tar.zst\n"), 0o600)
			return p
		}, "does not match"},
		{"not an archive", func(t *testing.T) string {
			p := filepath.Join(t.TempDir(), "junk.tar.zst")
			os.WriteFile(p, []byte("junk"), 0o600)
			return p
		}, ""},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			dir := filepath.Join(t.TempDir(), "home")
			_, err := Restore(context.Background(), tt.file(t), dir)
			if err == nil || !strings.Contains(err.Error(), tt.want) {
				t.Fatalf("err = %v, want %q", err, tt.want)
			}
			if _, statErr := os.Stat(dir); statErr == nil {
				t.Fatalf("a rejected archive wrote into the home: %v", listFiles(t, dir))
			}
		})
	}
}

func TestManifestListsEverythingOnce(t *testing.T) {
	home, _ := fixtureHome(t)
	file, err := Create(context.Background(), Options{Home: home, Dir: t.TempDir()})
	if err != nil {
		t.Fatal(err)
	}
	m, err := scan(context.Background(), file, nil)
	if err != nil {
		t.Fatal(err)
	}
	raw, _ := json.Marshal(m)
	if !bytes.Contains(raw, []byte(`"sha256"`)) || len(m.Files) != 20 {
		t.Fatalf("manifest has %d files: %s", len(m.Files), raw)
	}
}

func TestLatest(t *testing.T) {
	dir := t.TempDir()
	if p, _, err := Latest(filepath.Join(dir, "none")); err != nil || p != "" {
		t.Fatalf("missing dir: %q %v", p, err)
	}
	for _, n := range []string{"protectorate-hub-20261020T010000Z.tar.zst", "protectorate-hub-20261021T010000Z.tar.zst", "protectorate-hub-20261021T010000Z.tar.zst.sha256", "notes.txt"} {
		write(t, dir, n, "x", 0o600)
	}
	p, at, err := Latest(dir)
	if err != nil || filepath.Base(p) != "protectorate-hub-20261021T010000Z.tar.zst" || !at.Equal(time.Date(2026, 10, 21, 1, 0, 0, 0, time.UTC)) {
		t.Fatalf("latest = %q %v %v", p, at, err)
	}
}

func TestRunSchedulesBackups(t *testing.T) {
	home, _ := fixtureHome(t)
	var tick int
	opts := func(dir string) Options {
		return Options{Home: home, Dir: dir, Now: func() time.Time {
			tick++
			return time.Now().Add(time.Duration(tick) * time.Second)
		}}
	}
	count := func(dir string) int {
		n := 0
		for _, f := range listFiles(t, dir) {
			if strings.HasSuffix(f, ".tar.zst") {
				n++
			}
		}
		return n
	}
	run := func(dir string, every, d time.Duration) {
		ctx, cancel := context.WithTimeout(context.Background(), d)
		defer cancel()
		Run(ctx, opts(dir), every, t.Logf)
	}

	fresh := t.TempDir()
	run(fresh, time.Hour, 5*time.Second)
	if n := count(fresh); n != 1 {
		t.Fatalf("an empty backup dir got %d backups at start, want 1", n)
	}
	run(fresh, time.Hour, 500*time.Millisecond)
	if n := count(fresh); n != 1 {
		t.Fatalf("a recent backup did not hold off the next: %d", n)
	}
	every := t.TempDir()
	run(every, 300*time.Millisecond, 3*time.Second)
	if n := count(every); n < 3 {
		t.Fatalf("%d backups in 3s at a 300ms interval", n)
	}
	for _, f := range listFiles(t, every) {
		if strings.HasSuffix(f, ".part") {
			t.Fatalf("left a partial file: %s", f)
		}
	}
}
