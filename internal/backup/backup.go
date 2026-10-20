// Package backup snapshots the hub's state directory into one tar.zst and
// restores it into an empty directory.
//
// The hub keeps running while a snapshot is taken. Every file it writes is
// either replaced atomically or append-only with torn-tail recovery, so each
// copied file is consistent on its own. The supervisor stops headscale while
// its database and WAL are copied, then restarts it before archiving begins.
//
// The archive holds the files below and, last, MANIFEST.json with the size and
// SHA-256 of every file. A sidecar <archive>.sha256 holds the digest of the
// archive itself. Nothing is ever deleted: old backups stay until you remove
// them.
//
//	token, tokens.json, nodes.json, sleeves.json, leases.json, desk-policies.json,
//	desk-use.json, directory.json, mailbox.jsonl, backlog.md, audit/, ca/,
//	tailnet/ (db.sqlite as a snapshot, config, DERP certificate and keys, noise
//	key, tsnet state; not logs, sockets, headscale.yaml, policy.json, derpmap.json
//	or the API key, which the hub regenerates),
//	jobs/<id>/job.json and job.log (not artifacts) and jobs/request-ids.jsonl,
//	sleeves/<name>/chat.jsonl* and its request ids.
//
// Not included: bin/ (re-fetch with `hub tailnet install`), auth/ and login/
// (harness logins; run `protectorate login` again), node-files/, local-jobs/
// and every sleeve's workspace and home.
package backup

import (
	"archive/tar"
	"context"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/hotschmoe/protectorate/internal/headscaledb"
	"github.com/klauspost/compress/zstd"
)

const (
	// ManifestName is the last entry of every archive.
	ManifestName = "MANIFEST.json"
	// FormatVersion is bumped when the archive layout changes.
	FormatVersion = 2

	dbPath      = "tailnet/db.sqlite"
	archivePre  = "protectorate-hub-"
	archiveExt  = ".tar.zst"
	stampLayout = "20060102T150405Z"
)

// File is one archived file.
type File struct {
	Path   string `json:"path"`
	Size   int64  `json:"size"`
	SHA256 string `json:"sha256"`
	Mode   uint32 `json:"mode"`
}

// Manifest describes an archive.
type Manifest struct {
	Format  int       `json:"format"`
	Created time.Time `json:"created"`
	Version string    `json:"hub_version,omitempty"`
	Home    string    `json:"home"`
	Files   []File    `json:"files"`
}

// Options configure Create.
type Options struct {
	// Home is the hub state directory.
	Home string
	// Dir receives the archive; it is created if missing.
	Dir string
	// Version is recorded in the manifest.
	Version string
	// Now overrides the clock, for tests.
	Now func() time.Time
}

type source struct {
	rel  string
	path string
}

type contextReader struct {
	ctx context.Context
	r   io.Reader
}

func (r *contextReader) Read(p []byte) (int, error) {
	if err := r.ctx.Err(); err != nil {
		return 0, err
	}
	return r.r.Read(p)
}

// Create writes a new archive into opts.Dir and returns its path. The archive
// is complete when it appears: it is written under a temporary name and
// renamed.
func Create(ctx context.Context, opts Options) (string, error) {
	now := time.Now
	if opts.Now != nil {
		now = opts.Now
	}
	home, err := filepath.Abs(opts.Home)
	if err != nil {
		return "", err
	}
	if fi, err := os.Stat(home); err != nil || !fi.IsDir() {
		return "", fmt.Errorf("hub home %s is not a directory", home)
	}
	if err := os.MkdirAll(opts.Dir, 0o700); err != nil {
		return "", fmt.Errorf("create backup dir: %w", err)
	}
	dbDir, err := snapshotDB(ctx, home, opts.Dir)
	if err != nil {
		return "", fmt.Errorf("snapshot headscale: %w", err)
	}
	defer os.RemoveAll(dbDir)
	srcs, err := plan(home)
	if err != nil {
		return "", fmt.Errorf("list state files: %w", err)
	}
	for _, name := range []string{"db.sqlite", "db.sqlite-wal"} {
		p := filepath.Join(dbDir, name)
		if _, err := os.Stat(p); err == nil {
			srcs = append(srcs, source{rel: "tailnet/" + name, path: p})
		}
	}
	sort.Slice(srcs, func(i, j int) bool { return srcs[i].rel < srcs[j].rel })
	created := now().UTC()
	final := filepath.Join(opts.Dir, archivePre+created.Format(stampLayout)+archiveExt)
	if _, err := os.Stat(final); err == nil {
		return "", fmt.Errorf("%s already exists", final)
	}
	part, err := os.CreateTemp(opts.Dir, ".backup-*.part")
	if err != nil {
		return "", fmt.Errorf("create backup file: %w", err)
	}
	done := false
	defer func() {
		part.Close()
		if !done {
			os.Remove(part.Name())
		}
	}()
	if err := part.Chmod(0o600); err != nil {
		return "", err
	}
	archiveHash := sha256.New()
	zw, err := zstd.NewWriter(io.MultiWriter(part, archiveHash))
	if err != nil {
		return "", err
	}
	tw := tar.NewWriter(zw)
	m := Manifest{Format: FormatVersion, Created: created, Version: opts.Version, Home: home}
	for _, s := range srcs {
		if err := ctx.Err(); err != nil {
			return "", err
		}
		f, err := addFile(ctx, tw, s)
		if errors.Is(err, fs.ErrNotExist) {
			continue
		}
		if err != nil {
			return "", fmt.Errorf("back up %s: %w", s.rel, err)
		}
		m.Files = append(m.Files, f)
	}
	raw, err := json.MarshalIndent(m, "", "  ")
	if err != nil {
		return "", err
	}
	if err := writeEntry(tw, ManifestName, 0o600, raw); err != nil {
		return "", err
	}
	if err := tw.Close(); err != nil {
		return "", err
	}
	if err := zw.Close(); err != nil {
		return "", err
	}
	if err := part.Sync(); err != nil {
		return "", err
	}
	if err := part.Close(); err != nil {
		return "", err
	}
	if err := os.Rename(part.Name(), final); err != nil {
		return "", err
	}
	done = true
	sum := hex.EncodeToString(archiveHash.Sum(nil))
	if err := os.WriteFile(final+".sha256", []byte(sum+"  "+filepath.Base(final)+"\n"), 0o600); err != nil {
		return "", fmt.Errorf("write checksum: %w", err)
	}
	return final, nil
}

// plan lists what to archive, sorted by archive path.
func plan(home string) ([]source, error) {
	var out []source
	add := func(rel string) {
		out = append(out, source{rel: rel, path: filepath.Join(home, filepath.FromSlash(rel))})
	}
	for _, f := range []string{"token", "tokens.json", "nodes.json", "sleeves.json", "leases.json", "desk-policies.json", "desk-use.json", "directory.json", "mailbox.jsonl", "backlog.md", "jobs/request-ids.jsonl"} {
		if fi, err := os.Lstat(filepath.Join(home, filepath.FromSlash(f))); err == nil && fi.Mode().IsRegular() {
			add(f)
		}
	}
	walk := func(dir string, keep func(rel string) bool) error {
		root := filepath.Join(home, dir)
		err := filepath.WalkDir(root, func(p string, d fs.DirEntry, err error) error {
			if err != nil {
				if errors.Is(err, fs.ErrNotExist) {
					return nil
				}
				return err
			}
			if !d.Type().IsRegular() {
				return nil
			}
			rel, err := filepath.Rel(home, p)
			if err != nil {
				return err
			}
			rel = filepath.ToSlash(rel)
			if keep(rel) {
				add(rel)
			}
			return nil
		})
		return err
	}
	all := func(string) bool { return true }
	if err := walk("audit", all); err != nil {
		return nil, err
	}
	if err := walk("ca", all); err != nil {
		return nil, err
	}
	if err := walk("tailnet", keepTailnet); err != nil {
		return nil, err
	}
	if err := walk("jobs", keepJob); err != nil {
		return nil, err
	}
	if err := walk("sleeves", keepChat); err != nil {
		return nil, err
	}
	sort.Slice(out, func(i, j int) bool { return out[i].rel < out[j].rel })
	return out, nil
}

func keepTailnet(rel string) bool {
	name := path.Base(rel)
	switch {
	case strings.HasPrefix(rel, "tailnet/db.sqlite"), strings.Contains(name, ".log"):
		return false
	}
	switch name {
	case "headscale.sock", "headscale.yaml", "policy.json", "derpmap.json", "apikey", headscaledb.Request, headscaledb.Database:
		return false
	}
	return true
}

// keepJob keeps job.json and job.log of each job, not its artifacts.
func keepJob(rel string) bool {
	parts := strings.Split(rel, "/")
	return len(parts) == 3 && (parts[2] == "job.json" || parts[2] == "job.log")
}

// keepChat keeps sleeves/<name>/chat.jsonl* and the request ids beside it.
func keepChat(rel string) bool {
	parts := strings.Split(rel, "/")
	return len(parts) == 3 && strings.HasPrefix(parts[2], "chat.jsonl")
}

func writeEntry(tw *tar.Writer, name string, mode int64, data []byte) error {
	if err := tw.WriteHeader(&tar.Header{Name: name, Mode: mode, Size: int64(len(data)), Typeflag: tar.TypeReg}); err != nil {
		return err
	}
	_, err := tw.Write(data)
	return err
}

// addFile writes one source into the archive and returns its manifest entry.
// A file that grows while it is copied is archived as the prefix of the size
// it had when the copy started.
func addFile(ctx context.Context, tw *tar.Writer, s source) (File, error) {
	p := s.path
	f, err := os.Open(p)
	if err != nil {
		return File{}, err
	}
	defer f.Close()
	fi, err := f.Stat()
	if err != nil {
		return File{}, err
	}
	mode := fi.Mode().Perm()
	size := fi.Size()
	if err := tw.WriteHeader(&tar.Header{Name: s.rel, Mode: int64(mode), Size: size, ModTime: fi.ModTime(), Typeflag: tar.TypeReg}); err != nil {
		return File{}, err
	}
	h := sha256.New()
	if _, err := io.Copy(io.MultiWriter(tw, h), io.LimitReader(&contextReader{ctx: ctx, r: f}, size)); err != nil {
		return File{}, err
	}
	return File{Path: s.rel, Size: size, SHA256: hex.EncodeToString(h.Sum(nil)), Mode: uint32(mode)}, nil
}

// snapshotDB holds the request until the supervisor has reaped headscale and
// released the database lock. WAL recovery rebuilds SHM when headscale starts.
func snapshotDB(ctx context.Context, home, dir string) (string, error) {
	request, err := headscaledb.Lock(ctx, home, headscaledb.Request)
	if err != nil {
		return "", err
	}
	defer request.Close()
	gate, err := headscaledb.Lock(ctx, home, headscaledb.Database)
	if err != nil {
		return "", err
	}
	defer gate.Close()
	tmp, err := os.MkdirTemp(dir, ".backup-db-")
	if err != nil {
		return "", fmt.Errorf("create database snapshot directory: %w", err)
	}
	ok := false
	defer func() {
		if !ok {
			os.RemoveAll(tmp)
		}
	}()
	for _, name := range []string{"db.sqlite", "db.sqlite-wal"} {
		src, err := os.Open(filepath.Join(home, "tailnet", name))
		if errors.Is(err, fs.ErrNotExist) {
			continue
		}
		if err != nil {
			return "", fmt.Errorf("open %s: %w", name, err)
		}
		dst, err := os.OpenFile(filepath.Join(tmp, name), os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0o600)
		if err != nil {
			src.Close()
			return "", fmt.Errorf("create %s snapshot: %w", name, err)
		}
		_, err = io.Copy(dst, &contextReader{ctx: ctx, r: src})
		src.Close()
		closeErr := dst.Close()
		if err != nil {
			return "", fmt.Errorf("copy %s: %w", name, err)
		}
		if closeErr != nil {
			return "", fmt.Errorf("close %s snapshot: %w", name, closeErr)
		}
	}
	db := filepath.Join(tmp, "db.sqlite")
	if _, err := os.Stat(db); err == nil {
		if err := checkDB(ctx, db); err != nil {
			return "", fmt.Errorf("validate database snapshot: %w", err)
		}
	} else if !errors.Is(err, fs.ErrNotExist) {
		return "", fmt.Errorf("stat database snapshot: %w", err)
	} else if _, err := os.Stat(filepath.Join(tmp, "db.sqlite-wal")); err == nil {
		return "", fmt.Errorf("headscale WAL exists without a database")
	}
	ok = true
	return tmp, nil
}

// Restore unpacks archive into home, which must be empty or not exist. The
// archive is verified end to end (the sidecar digest when present, then the
// manifest: every file's size and SHA-256, and no entry the manifest does not
// list) before anything is written, and the restored SQLite database is
// checked for a valid SQLite header and page alignment afterwards. Headscale
// performs WAL recovery when it opens the restored database.
func Restore(ctx context.Context, archive, home string) (*Manifest, error) {
	if entries, err := os.ReadDir(home); err == nil && len(entries) > 0 {
		return nil, fmt.Errorf("%s is not empty; restore only into an empty home", home)
	} else if err != nil && !errors.Is(err, fs.ErrNotExist) {
		return nil, err
	}
	if err := verifySidecar(archive); err != nil {
		return nil, err
	}
	m, err := scan(ctx, archive, nil)
	if err != nil {
		return nil, err
	}
	if err := os.MkdirAll(home, 0o700); err != nil {
		return nil, fmt.Errorf("create home: %w", err)
	}
	if _, err := scan(ctx, archive, func(h *tar.Header, r io.Reader) error { return extract(home, h, r) }); err != nil {
		return nil, fmt.Errorf("extract into %s (the directory is incomplete): %w", home, err)
	}
	if _, err := os.Stat(filepath.Join(home, filepath.FromSlash(dbPath))); err == nil {
		if err := checkDB(ctx, filepath.Join(home, filepath.FromSlash(dbPath))); err != nil {
			return nil, fmt.Errorf("restored headscale database: %w", err)
		}
	}
	return m, nil
}

func verifySidecar(archive string) error {
	raw, err := os.ReadFile(archive + ".sha256")
	if errors.Is(err, fs.ErrNotExist) {
		return nil
	}
	if err != nil {
		return err
	}
	want, _, _ := strings.Cut(strings.TrimSpace(string(raw)), " ")
	f, err := os.Open(archive)
	if err != nil {
		return err
	}
	defer f.Close()
	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return err
	}
	if got := hex.EncodeToString(h.Sum(nil)); got != want {
		return fmt.Errorf("%s does not match %s.sha256: archive %s, sidecar %s", archive, archive, got, want)
	}
	return nil
}

// scan reads the whole archive, checks every file against the manifest and
// calls visit with each file's header and content as it is read.
func scan(ctx context.Context, archive string, visit func(*tar.Header, io.Reader) error) (*Manifest, error) {
	f, err := os.Open(archive)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	zr, err := zstd.NewReader(f)
	if err != nil {
		return nil, err
	}
	defer zr.Close()
	tr := tar.NewReader(zr)
	seen := map[string]File{}
	var m *Manifest
	for {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		h, err := tr.Next()
		if errors.Is(err, io.EOF) {
			break
		}
		if err != nil {
			return nil, fmt.Errorf("read archive: %w", err)
		}
		if m != nil {
			return nil, fmt.Errorf("archive has %q after the manifest", h.Name)
		}
		if h.Name == ManifestName {
			raw, err := io.ReadAll(io.LimitReader(tr, 64<<20))
			if err != nil {
				return nil, err
			}
			m = &Manifest{}
			if err := json.Unmarshal(raw, m); err != nil {
				return nil, fmt.Errorf("parse manifest: %w", err)
			}
			continue
		}
		if h.Typeflag != tar.TypeReg || !safeRel(h.Name) {
			return nil, fmt.Errorf("archive entry %q is not a plain file below the home", h.Name)
		}
		if _, dup := seen[h.Name]; dup {
			return nil, fmt.Errorf("archive lists %q twice", h.Name)
		}
		hash := sha256.New()
		body := &countReader{r: io.TeeReader(tr, hash)}
		if visit != nil {
			if err := visit(h, body); err != nil {
				return nil, err
			}
		}
		if _, err := io.Copy(io.Discard, body); err != nil {
			return nil, fmt.Errorf("read %s: %w", h.Name, err)
		}
		seen[h.Name] = File{Path: h.Name, Size: body.n, SHA256: hex.EncodeToString(hash.Sum(nil)), Mode: uint32(h.Mode)}
	}
	if m == nil {
		return nil, errors.New("archive has no manifest")
	}
	if m.Format != FormatVersion {
		return nil, fmt.Errorf("archive format %d, this binary reads %d", m.Format, FormatVersion)
	}
	if len(m.Files) != len(seen) {
		return nil, fmt.Errorf("manifest lists %d files, archive holds %d", len(m.Files), len(seen))
	}
	for _, want := range m.Files {
		got, ok := seen[want.Path]
		switch {
		case !ok:
			return nil, fmt.Errorf("manifest lists %q, which is not in the archive", want.Path)
		case got.Size != want.Size || got.SHA256 != want.SHA256:
			return nil, fmt.Errorf("%s does not match the manifest (size %d, sha256 %s; manifest says %d, %s)", want.Path, got.Size, got.SHA256, want.Size, want.SHA256)
		}
	}
	return m, nil
}

type countReader struct {
	r io.Reader
	n int64
}

func (c *countReader) Read(p []byte) (int, error) {
	n, err := c.r.Read(p)
	c.n += int64(n)
	return n, err
}

func safeRel(name string) bool {
	return name != "" && !strings.HasPrefix(name, "/") && !strings.Contains(name, "\\") && fs.ValidPath(name) && name != "."
}

func extract(home string, h *tar.Header, r io.Reader) error {
	dst := filepath.Join(home, filepath.FromSlash(h.Name))
	if err := os.MkdirAll(filepath.Dir(dst), 0o700); err != nil {
		return err
	}
	f, err := os.OpenFile(dst, os.O_CREATE|os.O_EXCL|os.O_WRONLY, os.FileMode(h.Mode).Perm()|0o600)
	if err != nil {
		return err
	}
	if _, err := io.Copy(f, r); err != nil {
		f.Close()
		return err
	}
	if err := f.Sync(); err != nil {
		f.Close()
		return err
	}
	if err := f.Close(); err != nil {
		return err
	}
	return os.Chmod(dst, os.FileMode(h.Mode).Perm())
}

func checkDB(ctx context.Context, p string) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	f, err := os.Open(p)
	if err != nil {
		return fmt.Errorf("open database: %w", err)
	}
	defer f.Close()
	var header [100]byte
	if _, err := io.ReadFull(f, header[:]); err != nil {
		return fmt.Errorf("read SQLite header: %w", err)
	}
	if string(header[:16]) != "SQLite format 3\x00" {
		return fmt.Errorf("invalid SQLite header")
	}
	pageSize := int64(binary.BigEndian.Uint16(header[16:18]))
	if pageSize == 1 {
		pageSize = 65536
	}
	fi, err := f.Stat()
	if err != nil {
		return fmt.Errorf("stat database: %w", err)
	}
	if pageSize < 512 || pageSize > 65536 || pageSize&(pageSize-1) != 0 || fi.Size() < pageSize || fi.Size()%pageSize != 0 {
		return fmt.Errorf("invalid SQLite page size %d or file size %d", pageSize, fi.Size())
	}
	if header[18] < 1 || header[18] > 2 || header[19] < 1 || header[19] > 2 || header[21] != 64 || header[22] != 32 || header[23] != 32 {
		return fmt.Errorf("invalid SQLite format fields")
	}
	return nil
}

// Latest returns the newest archive in dir by name, and when it was made.
func Latest(dir string) (string, time.Time, error) {
	entries, err := os.ReadDir(dir)
	if errors.Is(err, fs.ErrNotExist) {
		return "", time.Time{}, nil
	}
	if err != nil {
		return "", time.Time{}, err
	}
	var best string
	var at time.Time
	for _, e := range entries {
		n := e.Name()
		if !strings.HasPrefix(n, archivePre) || !strings.HasSuffix(n, archiveExt) {
			continue
		}
		t, err := time.Parse(stampLayout, strings.TrimSuffix(strings.TrimPrefix(n, archivePre), archiveExt))
		if err == nil && t.After(at) {
			best, at = n, t
		}
	}
	if best == "" {
		return "", time.Time{}, nil
	}
	return filepath.Join(dir, best), at, nil
}

// Run makes a backup now unless the newest one in opts.Dir is younger than
// every, then one each interval until ctx ends. A failed backup is reported
// through logf and tried again next interval.
func Run(ctx context.Context, opts Options, every time.Duration, logf func(format string, args ...any)) {
	wait := time.Duration(0)
	if _, at, err := Latest(opts.Dir); err == nil && !at.IsZero() {
		wait = max(0, every-time.Since(at))
	}
	for {
		select {
		case <-time.After(wait):
		case <-ctx.Done():
			return
		}
		if p, err := Create(ctx, opts); err != nil {
			if ctx.Err() != nil {
				return
			}
			logf("backup: %v", err)
		} else {
			logf("backup: wrote %s", p)
		}
		wait = every
	}
}
