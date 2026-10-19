package backlog

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"

	"github.com/hotschmoe/protectorate/internal/fsutil"
)

const maxAttempts = 16

// Store applies edits to the backlog file. Every edit reads the file, applies
// the change to the fresh content and writes via temp file and rename. If the
// file changed between the read and the rename (a human saving in an editor,
// another process) the change is re-applied to the new content instead of
// overwriting it. A change landing between the final check and the rename
// itself cannot be detected without cooperative locking; that window is
// microseconds wide.
type Store struct {
	path string
	mu   sync.Mutex
	now  func() time.Time

	beforeCommit func()
}

func NewStore(path string) *Store {
	return &Store{path: path, now: time.Now}
}

func (s *Store) Path() string { return s.path }

func (s *Store) List(f Filter) ([]Item, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	raw, err := readFile(s.path)
	if err != nil {
		return nil, err
	}
	d := Parse(string(raw))
	if d.missingIDs() {
		if err := s.modify(func(*Doc) (bool, error) { return false, nil }); err != nil {
			return nil, err
		}
		if raw, err = readFile(s.path); err != nil {
			return nil, err
		}
		d = Parse(string(raw))
	}
	return d.Query(f), nil
}

func (s *Store) Add(n NewItem) (Item, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	var out Item
	err := s.modify(func(d *Doc) (bool, error) {
		it, err := d.Add(n)
		out = it
		return err == nil, err
	})
	return out, err
}

// Update moves item id to st and returns the item and its previous state.
func (s *Store) Update(id string, st State, by string) (Item, State, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	var (
		out  Item
		prev State
	)
	err := s.modify(func(d *Doc) (bool, error) {
		it, p, err := d.SetState(id, st, by, s.now())
		out, prev = it, p
		return err == nil && p != st, err
	})
	return out, prev, err
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
		return nil, fmt.Errorf("read backlog: %w", err)
	}
	return b, nil
}

func (s *Store) modify(fn func(*Doc) (bool, error)) error {
	target := s.path
	if real, err := filepath.EvalSymlinks(target); err == nil {
		target = real
	}
	unlock, err := lock(target)
	if err != nil {
		return err
	}
	defer unlock()
	for attempt := 0; attempt < maxAttempts; attempt++ {
		orig, err := readFile(target)
		if err != nil {
			return err
		}
		d := Parse(string(orig))
		ided, err := d.EnsureIDs()
		if err != nil {
			return err
		}
		changed, err := fn(d)
		if err != nil {
			return err
		}
		if !changed && !ided {
			return nil
		}
		committed, err := commit(target, orig, []byte(d.String()), s.beforeCommit)
		if err != nil {
			return err
		}
		if committed {
			return nil
		}
	}
	return fmt.Errorf("backlog %s kept changing during update", s.path)
}

const lockWait = 10 * time.Second

// lock serializes writers across processes with an OS file lock on a sidecar
// file next to the backlog. The kernel drops the lock when its holder exits,
// so a crashed writer never leaves a stale lock and a live writer is never
// overlapped. The sidecar is left in place: removing it would let a waiter
// lock an unlinked inode while a newcomer locks a fresh file.
func lock(path string) (func(), error) {
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return nil, fmt.Errorf("create %s: %w", dir, err)
	}
	name := filepath.Join(dir, "."+filepath.Base(path)+".lock")
	f, err := os.OpenFile(name, os.O_RDWR|os.O_CREATE, 0o600)
	if err != nil {
		return nil, fmt.Errorf("lock backlog: %w", err)
	}
	deadline := time.Now().Add(lockWait)
	for {
		ok, err := tryLockFile(f)
		if err != nil {
			_ = f.Close()
			return nil, fmt.Errorf("lock backlog: %w", err)
		}
		if ok {
			return func() { unlockFile(f); _ = f.Close() }, nil
		}
		if time.Now().After(deadline) {
			_ = f.Close()
			return nil, fmt.Errorf("lock backlog: timed out waiting for %s", name)
		}
		time.Sleep(2 * time.Millisecond)
	}
}

// commit writes data over path if path still holds orig.
func commit(path string, orig, data []byte, hook func()) (bool, error) {
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return false, fmt.Errorf("create %s: %w", dir, err)
	}
	mode := os.FileMode(0o600)
	if fi, err := os.Stat(path); err == nil {
		mode = fi.Mode().Perm()
	}
	tmp, err := os.CreateTemp(dir, ".backlog-*.tmp")
	if err != nil {
		return false, fmt.Errorf("create temp file: %w", err)
	}
	name := tmp.Name()
	cleanup := func() { _ = tmp.Close(); _ = os.Remove(name) }
	if _, err := tmp.Write(data); err != nil {
		cleanup()
		return false, fmt.Errorf("write backlog: %w", err)
	}
	if err := tmp.Sync(); err != nil {
		cleanup()
		return false, fmt.Errorf("sync backlog: %w", err)
	}
	if err := tmp.Chmod(mode); err != nil && !errors.Is(err, errors.ErrUnsupported) {
		cleanup()
		return false, fmt.Errorf("chmod backlog: %w", err)
	}
	if err := tmp.Close(); err != nil {
		_ = os.Remove(name)
		return false, fmt.Errorf("close backlog: %w", err)
	}
	if hook != nil {
		hook()
	}
	cur, err := readFile(path)
	if err != nil {
		_ = os.Remove(name)
		return false, err
	}
	if !bytes.Equal(cur, orig) {
		_ = os.Remove(name)
		return false, nil
	}
	if err := fsutil.Retry(func() error { return os.Rename(name, path) }); err != nil {
		_ = os.Remove(name)
		return false, fmt.Errorf("replace backlog: %w", err)
	}
	return true, nil
}
