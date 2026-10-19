package backlog

import (
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"sync"
	"time"

	"github.com/hotschmoe/protectorate/internal/fsutil"
)

const (
	notesFile  = "notes.json"
	legacyFile = "backlog.md"
	// legacyDone is where the pre-notes markdown backlog is moved after its
	// items have been converted.
	legacyDone = "backlog.md.migrated"
)

// Store keeps the notes in $home/notes.json. Every change reads the fresh
// file under an OS file lock and replaces it by temp file and rename, so a
// reader never sees a partial file. The first load of a hub that still has
// the old markdown backlog converts its items into notes.
type Store struct {
	dir string
	mu  sync.Mutex
	now func() time.Time
}

func NewStore(home string) *Store {
	return &Store{dir: home, now: time.Now}
}

func (s *Store) path() string { return filepath.Join(s.dir, notesFile) }

type fileDoc struct {
	Notes []Note `json:"notes"`
}

func (s *Store) List(f Filter) ([]Note, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	all, err := s.snapshot()
	if err != nil {
		return nil, err
	}
	out := make([]Note, 0, len(all))
	for _, n := range all {
		if f.match(n) {
			out = append(out, n)
		}
	}
	sortNotes(out)
	return out, nil
}

func (s *Store) Get(id string) (Note, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	all, err := s.snapshot()
	if err != nil {
		return Note{}, err
	}
	for _, n := range all {
		if n.ID == id {
			return n, nil
		}
	}
	return Note{}, fmt.Errorf("%w: %q", ErrNotFound, id)
}

func (s *Store) Create(n NewNote) (Note, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	var out Note
	err := s.modify(func(notes []Note) ([]Note, error) {
		used := map[string]bool{}
		for _, e := range notes {
			used[e.ID] = true
		}
		id, err := newID(used)
		if err != nil {
			return nil, err
		}
		if out, err = n.build(id, s.now().UTC()); err != nil {
			return nil, err
		}
		return append(notes, out), nil
	})
	return out, err
}

// Update applies p to note id and reports whether anything changed.
func (s *Store) Update(id string, p Patch) (Note, bool, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	var (
		out     Note
		changed bool
	)
	err := s.modify(func(notes []Note) ([]Note, error) {
		for i, n := range notes {
			if n.ID != id {
				continue
			}
			var err error
			if out, changed, err = p.apply(n, s.now().UTC()); err != nil || !changed {
				return nil, err
			}
			notes[i] = out
			return notes, nil
		}
		return nil, fmt.Errorf("%w: %q", ErrNotFound, id)
	})
	return out, changed, err
}

// Delete removes note id and returns it.
func (s *Store) Delete(id string) (Note, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	var out Note
	err := s.modify(func(notes []Note) ([]Note, error) {
		for i, n := range notes {
			if n.ID == id {
				out = n
				return append(notes[:i:i], notes[i+1:]...), nil
			}
		}
		return nil, fmt.Errorf("%w: %q", ErrNotFound, id)
	})
	return out, err
}

func readFile(path string) ([]byte, error) {
	var b []byte
	err := fsutil.Retry(func() (err error) {
		b, err = os.ReadFile(path)
		return err
	})
	if errors.Is(err, os.ErrNotExist) {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("read notes: %w", err)
	}
	return b, nil
}

func decode(raw []byte) ([]Note, error) {
	var d fileDoc
	if err := json.Unmarshal(raw, &d); err != nil {
		return nil, fmt.Errorf("parse %s: %w", notesFile, err)
	}
	return d.Notes, nil
}

// snapshot reads the notes without the lock; the file is replaced atomically.
func (s *Store) snapshot() ([]Note, error) {
	raw, err := readFile(s.path())
	if err != nil {
		return nil, err
	}
	if raw != nil {
		return decode(raw)
	}
	unlock, err := lock(s.path())
	if err != nil {
		return nil, err
	}
	defer unlock()
	return s.loadLocked()
}

// loadLocked returns the stored notes, converting the legacy markdown backlog
// first when no notes file exists yet.
func (s *Store) loadLocked() ([]Note, error) {
	raw, err := readFile(s.path())
	if err != nil {
		return nil, err
	}
	if raw != nil {
		return decode(raw)
	}
	legacy := filepath.Join(s.dir, legacyFile)
	text, err := readFile(legacy)
	if err != nil || text == nil {
		return nil, err
	}
	modTime := s.now().UTC()
	if fi, err := os.Stat(legacy); err == nil {
		modTime = fi.ModTime().UTC()
	}
	notes := convertLegacy(string(text), modTime)
	if err := s.write(notes); err != nil {
		return nil, err
	}
	if err := os.Rename(legacy, filepath.Join(s.dir, legacyDone)); err != nil {
		log.Printf("notes: converted %s but could not rename it to %s: %v", legacy, legacyDone, err)
	}
	return notes, nil
}

func (s *Store) modify(fn func([]Note) ([]Note, error)) error {
	target := s.path()
	if real, err := filepath.EvalSymlinks(target); err == nil {
		target = real
	}
	unlock, err := lock(target)
	if err != nil {
		return err
	}
	defer unlock()
	notes, err := s.loadLocked()
	if err != nil {
		return err
	}
	next, err := fn(notes)
	if err != nil || next == nil {
		return err
	}
	return s.write(next)
}

const lockWait = 10 * time.Second

// lock serializes writers across processes with an OS file lock on a sidecar
// file next to the notes. The kernel drops the lock when its holder exits, so
// a crashed writer never leaves a stale lock. The sidecar is left in place:
// removing it would let a waiter lock an unlinked inode while a newcomer locks
// a fresh file.
func lock(path string) (func(), error) {
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return nil, fmt.Errorf("create %s: %w", dir, err)
	}
	name := filepath.Join(dir, "."+filepath.Base(path)+".lock")
	f, err := os.OpenFile(name, os.O_RDWR|os.O_CREATE, 0o600)
	if err != nil {
		return nil, fmt.Errorf("lock notes: %w", err)
	}
	deadline := time.Now().Add(lockWait)
	for {
		ok, err := tryLockFile(f)
		if err != nil {
			_ = f.Close()
			return nil, fmt.Errorf("lock notes: %w", err)
		}
		if ok {
			return func() { unlockFile(f); _ = f.Close() }, nil
		}
		if time.Now().After(deadline) {
			_ = f.Close()
			return nil, fmt.Errorf("lock notes: timed out waiting for %s", name)
		}
		time.Sleep(2 * time.Millisecond)
	}
}

// write replaces the notes file with notes. The caller holds the lock.
func (s *Store) write(notes []Note) error {
	if notes == nil {
		notes = []Note{}
	}
	data, err := json.MarshalIndent(fileDoc{Notes: notes}, "", "  ")
	if err != nil {
		return fmt.Errorf("encode notes: %w", err)
	}
	data = append(data, '\n')
	path := s.path()
	if real, err := filepath.EvalSymlinks(path); err == nil {
		path = real
	}
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return fmt.Errorf("create %s: %w", dir, err)
	}
	mode := os.FileMode(0o600)
	if fi, err := os.Stat(path); err == nil {
		mode = fi.Mode().Perm()
	}
	tmp, err := os.CreateTemp(dir, ".notes-*.tmp")
	if err != nil {
		return fmt.Errorf("create temp file: %w", err)
	}
	name := tmp.Name()
	cleanup := func() { _ = tmp.Close(); _ = os.Remove(name) }
	if _, err := tmp.Write(data); err != nil {
		cleanup()
		return fmt.Errorf("write notes: %w", err)
	}
	if err := tmp.Sync(); err != nil {
		cleanup()
		return fmt.Errorf("sync notes: %w", err)
	}
	if err := tmp.Chmod(mode); err != nil && !errors.Is(err, errors.ErrUnsupported) {
		cleanup()
		return fmt.Errorf("chmod notes: %w", err)
	}
	if err := tmp.Close(); err != nil {
		_ = os.Remove(name)
		return fmt.Errorf("close notes: %w", err)
	}
	if err := fsutil.Retry(func() error { return os.Rename(name, path) }); err != nil {
		_ = os.Remove(name)
		return fmt.Errorf("replace notes: %w", err)
	}
	return nil
}
