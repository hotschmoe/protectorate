// Package fsutil holds the crash-safe file replacement used across the
// repository.
package fsutil

import (
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"runtime"
	"time"
)

// WriteFile replaces path with data so a crash never leaves a torn file: the
// data goes to a temp file in the same directory, is fsynced, chmodded to perm
// and renamed over path, then the directory is fsynced where supported.
func WriteFile(path string, data []byte, perm os.FileMode) (err error) {
	dir := filepath.Dir(path)
	f, err := os.CreateTemp(dir, "."+filepath.Base(path)+".*")
	if err != nil {
		return err
	}
	tmp := f.Name()
	defer func() {
		if err != nil {
			f.Close()
			os.Remove(tmp)
		}
	}()
	if err = chmod(f, perm); err != nil {
		return err
	}
	if _, err = f.Write(data); err != nil {
		return err
	}
	if err = f.Sync(); err != nil {
		return err
	}
	if err = f.Close(); err != nil {
		return err
	}
	if err = Rename(tmp, path); err != nil {
		return err
	}
	syncDir(dir)
	return nil
}

func chmod(f *os.File, perm os.FileMode) error {
	err := f.Chmod(perm)
	if errors.Is(err, fs.ErrPermission) || errors.Is(err, os.ErrInvalid) || errors.Is(err, errors.ErrUnsupported) {
		return nil
	}
	return err
}

func syncDir(dir string) {
	if runtime.GOOS == "windows" {
		return
	}
	d, err := os.Open(dir)
	if err != nil {
		return
	}
	_ = d.Sync()
	_ = d.Close()
}

// Rename is os.Rename that rides out Windows sharing conflicts.
func Rename(oldpath, newpath string) error {
	return Retry(func() error { return os.Rename(oldpath, newpath) })
}

// Retry reruns fn while Windows reports a sharing conflict, which shows
// up as a permission error: a file another handle has open cannot be replaced
// or deleted, and a file being deleted cannot be opened.
func Retry(fn func() error) error {
	err := fn()
	if runtime.GOOS != "windows" {
		return err
	}
	for deadline := time.Now().Add(5 * time.Second); errors.Is(err, fs.ErrPermission) && time.Now().Before(deadline); err = fn() {
		time.Sleep(2 * time.Millisecond)
	}
	return err
}
