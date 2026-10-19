// Package xfer moves files and directories as a stream of chunks with
// optional zstd compression, verified by size and SHA-256 and written
// atomically. Both ends of a transfer (the CLI and the node agent) use it.
package xfer

import (
	"archive/tar"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"hash"
	"io"
	"io/fs"
	"os"
	"path"
	"path/filepath"
	"strings"
	"sync"

	"github.com/klauspost/compress/zstd"
)

// ChunkSize is the largest uncompressed chunk, which bounds memory on both
// ends to a few chunks.
const ChunkSize = 4 << 20

const (
	tempPrefix = ".protectorate-put-"
	// give up trying to compress after this many chunks in a row that did
	// not shrink.
	maxMisses = 3
)

type Kind int

const (
	KindFile Kind = iota + 1
	KindDir
)

// Chunk is a slice of the uncompressed stream, compressed only when that
// helped.
type Chunk struct {
	Data       []byte
	Compressed bool
	RawSize    uint32
}

// Trailer ends a transfer: Size and SHA256 (hex) cover the uncompressed
// stream. Skipped counts directory entries that cannot travel.
type Trailer struct {
	Size    uint64
	SHA256  string
	Skipped uint32
}

// Header describes what a Source will send.
type Header struct {
	Kind Kind
	Mode uint32
	// Size is the file size; unknown (0) for directories.
	Size uint64
}

var incompressible = map[string]bool{
	".zip": true, ".gz": true, ".tgz": true, ".bz2": true, ".xz": true, ".zst": true, ".7z": true, ".rar": true,
	".jpg": true, ".jpeg": true, ".png": true, ".gif": true, ".webp": true, ".heic": true, ".avif": true,
	".mp3": true, ".aac": true, ".ogg": true, ".flac": true, ".mp4": true, ".mkv": true, ".mov": true, ".webm": true, ".avi": true,
	".pdf": true, ".docx": true, ".xlsx": true, ".pptx": true, ".jar": true, ".apk": true, ".whl": true,
}

var (
	encoder = sync.OnceValue(func() *zstd.Encoder {
		e, _ := zstd.NewWriter(nil, zstd.WithEncoderConcurrency(1), zstd.WithEncoderLevel(zstd.SpeedDefault))
		return e
	})
	decoder = sync.OnceValue(func() *zstd.Decoder {
		d, _ := zstd.NewReader(nil, zstd.WithDecoderConcurrency(1), zstd.WithDecoderMaxMemory(ChunkSize))
		return d
	})
)

func shrinks(raw, packed int) bool { return packed < raw-raw/16 }

// Source reads a file or directory as chunks.
type Source struct {
	hdr      Header
	r        io.Reader
	closers  []io.Closer
	sum      hash.Hash
	size     uint64
	try      bool
	misses   int
	skipped  uint32
	skipMu   sync.Mutex
	buf      []byte
	finished bool
	tarErr   chan error
}

// OpenSource opens p for sending. compress allows zstd; files with an
// already-compressed extension are never compressed.
func OpenSource(p string, compress bool) (*Source, error) {
	fi, err := os.Stat(p)
	if err != nil {
		return nil, err
	}
	s := &Source{sum: sha256.New(), try: compress, buf: make([]byte, ChunkSize)}
	switch {
	case fi.IsDir():
		s.hdr = Header{Kind: KindDir}
		pr, pw := io.Pipe()
		s.r = pr
		s.closers = append(s.closers, pr)
		s.tarErr = make(chan error, 1)
		go func() {
			err := s.writeTar(pw, p)
			pw.CloseWithError(err)
			s.tarErr <- err
		}()
	case fi.Mode().IsRegular():
		f, err := os.Open(p)
		if err != nil {
			return nil, err
		}
		s.hdr = Header{Kind: KindFile, Mode: uint32(fi.Mode().Perm()), Size: uint64(fi.Size())}
		s.r = f
		s.closers = append(s.closers, f)
		if incompressible[strings.ToLower(filepath.Ext(p))] {
			s.try = false
		}
	default:
		return nil, fmt.Errorf("%s is neither a regular file nor a directory", p)
	}
	return s, nil
}

func (s *Source) Header() Header { return s.hdr }

// Next returns the next chunk, or io.EOF after the last one; call Trailer
// then.
func (s *Source) Next() (Chunk, error) {
	n, err := io.ReadFull(s.r, s.buf)
	if n == 0 {
		if err == io.EOF || err == io.ErrUnexpectedEOF {
			s.finished = true
			if s.tarErr != nil {
				if terr := <-s.tarErr; terr != nil {
					return Chunk{}, fmt.Errorf("archive directory: %w", terr)
				}
			}
			return Chunk{}, io.EOF
		}
		return Chunk{}, err
	}
	if err != nil && err != io.ErrUnexpectedEOF {
		return Chunk{}, err
	}
	raw := s.buf[:n]
	s.sum.Write(raw)
	s.size += uint64(n)
	c := Chunk{Data: raw, RawSize: uint32(n)}
	if s.try {
		packed := encoder().EncodeAll(raw, nil)
		if shrinks(n, len(packed)) {
			c.Data, c.Compressed = packed, true
			s.misses = 0
		} else if s.misses++; s.misses >= maxMisses {
			s.try = false
		}
	}
	return c, nil
}

// Trailer is valid after Next returned io.EOF.
func (s *Source) Trailer() (Trailer, error) {
	if !s.finished {
		return Trailer{}, errors.New("source not fully read")
	}
	if s.hdr.Kind == KindFile && s.size != s.hdr.Size {
		return Trailer{}, fmt.Errorf("file changed while sending: expected %d bytes, read %d", s.hdr.Size, s.size)
	}
	s.skipMu.Lock()
	defer s.skipMu.Unlock()
	return Trailer{Size: s.size, SHA256: hex.EncodeToString(s.sum.Sum(nil)), Skipped: s.skipped}, nil
}

func (s *Source) Close() error {
	var first error
	for _, c := range s.closers {
		if err := c.Close(); err != nil && first == nil {
			first = err
		}
	}
	return first
}

func (s *Source) writeTar(w io.Writer, root string) error {
	tw := tar.NewWriter(w)
	err := filepath.WalkDir(root, func(p string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(root, p)
		if err != nil {
			return err
		}
		if rel == "." {
			return nil
		}
		fi, err := d.Info()
		if err != nil {
			return err
		}
		name := filepath.ToSlash(rel)
		switch {
		case fi.IsDir():
			return tw.WriteHeader(&tar.Header{Typeflag: tar.TypeDir, Name: name + "/", Mode: int64(fi.Mode().Perm()), ModTime: fi.ModTime()})
		case fi.Mode().IsRegular():
			if err := tw.WriteHeader(&tar.Header{Typeflag: tar.TypeReg, Name: name, Mode: int64(fi.Mode().Perm()), Size: fi.Size(), ModTime: fi.ModTime()}); err != nil {
				return err
			}
			f, err := os.Open(p)
			if err != nil {
				return err
			}
			defer f.Close()
			n, err := io.Copy(tw, io.LimitReader(f, fi.Size()))
			if err != nil {
				return err
			}
			if n != fi.Size() {
				return fmt.Errorf("%s shrank while archiving", p)
			}
			return nil
		default:
			s.skipMu.Lock()
			s.skipped++
			s.skipMu.Unlock()
			return nil
		}
	})
	if err != nil {
		return err
	}
	return tw.Close()
}

// Sink receives chunks into a temporary file next to the destination and
// replaces the destination only when Commit has verified size and SHA-256.
type Sink struct {
	dest      string
	kind      Kind
	mode      uint32
	overwrite bool
	f         *os.File
	sum       hash.Hash
	received  uint64
	done      bool
}

// NewSink prepares to receive into dest. It fails when dest exists and
// overwrite is not set, or when the kinds disagree.
func NewSink(dest string, kind Kind, mode uint32, overwrite bool) (*Sink, error) {
	if kind != KindFile && kind != KindDir {
		return nil, errors.New("unknown transfer kind")
	}
	if err := checkDest(dest, kind, overwrite); err != nil {
		return nil, err
	}
	dir := filepath.Dir(dest)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return nil, fmt.Errorf("create parent directory: %w", err)
	}
	f, err := os.CreateTemp(dir, tempPrefix+"*")
	if err != nil {
		return nil, fmt.Errorf("create temporary file: %w", err)
	}
	return &Sink{dest: dest, kind: kind, mode: mode, overwrite: overwrite, f: f, sum: sha256.New()}, nil
}

// ErrExists reports an existing destination without overwrite.
var ErrExists = errors.New("destination exists")

func checkDest(dest string, kind Kind, overwrite bool) error {
	fi, err := os.Lstat(dest)
	switch {
	case errors.Is(err, fs.ErrNotExist):
		return nil
	case err != nil:
		return err
	case !overwrite:
		return fmt.Errorf("%w: %s", ErrExists, dest)
	case fi.IsDir() != (kind == KindDir):
		return fmt.Errorf("%s exists as a different kind (file or directory)", dest)
	}
	return nil
}

func (s *Sink) Received() uint64 { return s.received }

// Write appends one chunk.
func (s *Sink) Write(c Chunk) error {
	data := c.Data
	if c.Compressed {
		var err error
		if data, err = decoder().DecodeAll(c.Data, nil); err != nil {
			return fmt.Errorf("decompress chunk: %w", err)
		}
	}
	if len(data) > ChunkSize || uint32(len(data)) != c.RawSize {
		return fmt.Errorf("chunk is %d bytes, header says %d", len(data), c.RawSize)
	}
	if _, err := s.f.Write(data); err != nil {
		return fmt.Errorf("write: %w", err)
	}
	s.sum.Write(data)
	s.received += uint64(len(data))
	return nil
}

// Commit verifies the stream and moves it into place.
func (s *Sink) Commit(t Trailer) error {
	if s.done {
		return errors.New("transfer already finished")
	}
	s.done = true
	tmp := s.f.Name()
	err := s.commit(t)
	if err != nil {
		s.f.Close()
		_ = os.Remove(tmp)
	}
	return err
}

func (s *Sink) commit(t Trailer) error {
	if s.received != t.Size {
		return fmt.Errorf("size mismatch: received %d bytes, sender sent %d", s.received, t.Size)
	}
	if got := hex.EncodeToString(s.sum.Sum(nil)); got != strings.ToLower(t.SHA256) {
		return fmt.Errorf("sha256 mismatch: received %s, sender computed %s", got, t.SHA256)
	}
	tmp := s.f.Name()
	if err := s.f.Sync(); err != nil {
		return err
	}
	if s.kind == KindFile {
		if err := s.f.Close(); err != nil {
			return err
		}
		if s.mode != 0 {
			if err := os.Chmod(tmp, fs.FileMode(s.mode).Perm()); err != nil {
				return err
			}
		}
		if err := checkDest(s.dest, KindFile, s.overwrite); err != nil {
			return err
		}
		return os.Rename(tmp, s.dest)
	}
	defer os.Remove(tmp)
	defer s.f.Close()
	tree, err := os.MkdirTemp(filepath.Dir(s.dest), tempPrefix+"dir-*")
	if err != nil {
		return err
	}
	if err := unpack(s.f, tree); err != nil {
		_ = os.RemoveAll(tree)
		return fmt.Errorf("extract directory: %w", err)
	}
	if err := checkDest(s.dest, KindDir, s.overwrite); err != nil {
		_ = os.RemoveAll(tree)
		return err
	}
	old := ""
	if _, err := os.Lstat(s.dest); err == nil {
		old = tree + ".old"
		if err := os.Rename(s.dest, old); err != nil {
			_ = os.RemoveAll(tree)
			return fmt.Errorf("move existing directory aside: %w", err)
		}
	}
	if err := os.Rename(tree, s.dest); err != nil {
		if old != "" {
			_ = os.Rename(old, s.dest)
		}
		_ = os.RemoveAll(tree)
		return err
	}
	if old != "" {
		_ = os.RemoveAll(old)
	}
	return nil
}

// Abort drops a transfer that will not finish.
func (s *Sink) Abort() {
	if s.done {
		return
	}
	s.done = true
	name := s.f.Name()
	s.f.Close()
	_ = os.Remove(name)
}

func unpack(f *os.File, root string) error {
	if _, err := f.Seek(0, io.SeekStart); err != nil {
		return err
	}
	tr := tar.NewReader(f)
	for {
		h, err := tr.Next()
		if err == io.EOF {
			return nil
		}
		if err != nil {
			return err
		}
		name := path.Clean(h.Name)
		local := filepath.FromSlash(name)
		if !filepath.IsLocal(local) || strings.Contains(h.Name, `\`) {
			return fmt.Errorf("unsafe entry name %q", h.Name)
		}
		target := filepath.Join(root, local)
		perm := fs.FileMode(h.Mode).Perm()
		switch h.Typeflag {
		case tar.TypeDir:
			if err := os.MkdirAll(target, perm|0o700); err != nil {
				return err
			}
		case tar.TypeReg:
			if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
				return err
			}
			out, err := os.OpenFile(target, os.O_WRONLY|os.O_CREATE|os.O_EXCL, perm|0o600)
			if err != nil {
				return err
			}
			_, err = io.Copy(out, tr)
			if cerr := out.Close(); err == nil {
				err = cerr
			}
			if err != nil {
				return err
			}
			_ = os.Chtimes(target, h.ModTime, h.ModTime)
		default:
			return fmt.Errorf("unsupported entry type %q for %q", h.Typeflag, h.Name)
		}
	}
}
