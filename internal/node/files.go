package node

import (
	"context"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/xfer"
)

const (
	maxTransfers = 16
	transferIdle = time.Hour
)

// Files serves file transfers for the hub. Paths are confined to Root unless
// Unconfined is set, in which case the agent's own OS permissions are the only
// limit (the same trust as a hub job, which runs any command as that user).
type Files struct {
	root       string
	unconfined bool

	mu   sync.Mutex
	puts map[string]*put
}

type put struct {
	mu   sync.Mutex
	sink *xfer.Sink
	last time.Time
	dest string
}

// NewFiles creates root if needed. With unconfined set, root only anchors
// relative paths.
func NewFiles(root string, unconfined bool) (*Files, error) {
	root, err := filepath.Abs(root)
	if err != nil {
		return nil, err
	}
	if err := os.MkdirAll(root, 0o755); err != nil {
		return nil, fmt.Errorf("create file root: %w", err)
	}
	if root, err = filepath.EvalSymlinks(root); err != nil {
		return nil, fmt.Errorf("resolve file root: %w", err)
	}
	return &Files{root: root, unconfined: unconfined, puts: map[string]*put{}}, nil
}

// Root is the directory relative remote paths resolve under.
func (f *Files) Root() string { return f.root }

func caseFold(s string) string {
	if runtime.GOOS == "windows" {
		return strings.ToLower(s)
	}
	return s
}

// within reports whether p is root or below it.
func within(root, p string) bool {
	rel, err := filepath.Rel(caseFold(root), caseFold(p))
	return err == nil && filepath.IsLocal(rel)
}

// resolve maps a remote path to a local one. Relative paths are under the
// root. When confined, the result must stay under the root even through
// symlinks of its existing ancestors.
func (f *Files) resolve(p string) (string, error) {
	if p == "" {
		return "", badArg("path is empty")
	}
	if strings.ContainsRune(p, 0) {
		return "", badArg("path contains a NUL byte")
	}
	if !filepath.IsAbs(p) {
		p = filepath.Join(f.root, p)
	}
	p = filepath.Clean(p)
	if f.unconfined {
		return p, nil
	}
	if !within(f.root, p) {
		return "", connect.NewError(connect.CodePermissionDenied, fmt.Errorf("path %s is outside the node file root %s", p, f.root))
	}
	probe := p
	for {
		real, err := filepath.EvalSymlinks(probe)
		if err == nil {
			if !within(f.root, real) {
				return "", connect.NewError(connect.CodePermissionDenied, fmt.Errorf("path %s resolves outside the node file root %s", p, f.root))
			}
			return p, nil
		}
		if !errors.Is(err, fs.ErrNotExist) {
			return "", badArg("resolve %s: %v", p, err)
		}
		next := filepath.Dir(probe)
		if next == probe {
			return p, nil
		}
		probe = next
	}
}

func chunkOf(c *pb.FileChunk) xfer.Chunk {
	return xfer.Chunk{Data: c.GetData(), Compressed: c.GetCompressed(), RawSize: c.GetRawSize()}
}

func kindOf(k pb.FileKind) (xfer.Kind, error) {
	switch k {
	case pb.FileKind_FILE_KIND_FILE:
		return xfer.KindFile, nil
	case pb.FileKind_FILE_KIND_DIRECTORY:
		return xfer.KindDir, nil
	}
	return 0, badArg("kind must be file or directory")
}

func kindProto(k xfer.Kind) pb.FileKind {
	if k == xfer.KindDir {
		return pb.FileKind_FILE_KIND_DIRECTORY
	}
	return pb.FileKind_FILE_KIND_FILE
}

func (f *Files) sweepLocked(now time.Time) {
	for id, p := range f.puts {
		if p.mu.TryLock() {
			if now.Sub(p.last) > transferIdle {
				p.sink.Abort()
				delete(f.puts, id)
			}
			p.mu.Unlock()
		}
	}
}

func (f *Files) begin(req *pb.PutFileRequest) (*put, error) {
	kind, err := kindOf(req.Kind)
	if err != nil {
		return nil, err
	}
	dest, err := f.resolve(req.Path)
	if err != nil {
		return nil, err
	}
	f.mu.Lock()
	defer f.mu.Unlock()
	now := time.Now()
	f.sweepLocked(now)
	if _, dup := f.puts[req.TransferId]; dup {
		return nil, connect.NewError(connect.CodeAlreadyExists, errors.New("transfer id is in use"))
	}
	if len(f.puts) >= maxTransfers {
		return nil, connect.NewError(connect.CodeResourceExhausted, errors.New("too many transfers in progress"))
	}
	sink, err := xfer.NewSink(dest, kind, req.Mode, req.Overwrite)
	if errors.Is(err, xfer.ErrExists) {
		return nil, connect.NewError(connect.CodeAlreadyExists, err)
	}
	if err != nil {
		return nil, connect.NewError(connect.CodeFailedPrecondition, err)
	}
	p := &put{sink: sink, last: now, dest: dest}
	f.puts[req.TransferId] = p
	return p, nil
}

func (f *Files) drop(id string) {
	f.mu.Lock()
	delete(f.puts, id)
	f.mu.Unlock()
}

// Put applies one chunk of an upload.
func (f *Files) Put(req *pb.PutFileRequest) (*pb.PutFileResponse, error) {
	if !idRe.MatchString(req.TransferId) || len(req.TransferId) < 8 {
		return nil, badArg("transfer_id must be 8 to 80 characters of [A-Za-z0-9._-]")
	}
	var p *put
	if req.Offset == 0 {
		var err error
		if p, err = f.begin(req); err != nil {
			return nil, err
		}
	} else {
		f.mu.Lock()
		p = f.puts[req.TransferId]
		f.mu.Unlock()
		if p == nil {
			return nil, connect.NewError(connect.CodeNotFound, errors.New("unknown or expired transfer"))
		}
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	fail := func(code connect.Code, err error) (*pb.PutFileResponse, error) {
		p.sink.Abort()
		f.drop(req.TransferId)
		return nil, connect.NewError(code, err)
	}
	if got := p.sink.Received(); got != req.Offset {
		return fail(connect.CodeInvalidArgument, fmt.Errorf("offset %d, expected %d", req.Offset, got))
	}
	p.last = time.Now()
	if req.Chunk != nil {
		if err := p.sink.Write(chunkOf(req.Chunk)); err != nil {
			return fail(connect.CodeInvalidArgument, err)
		}
	}
	res := &pb.PutFileResponse{Received: p.sink.Received()}
	if !req.Last {
		return res, nil
	}
	if req.Trailer == nil {
		return fail(connect.CodeInvalidArgument, errors.New("last chunk needs a trailer"))
	}
	if err := p.sink.Commit(xfer.Trailer{Size: req.Trailer.Size, SHA256: req.Trailer.Sha256}); err != nil {
		f.drop(req.TransferId)
		return nil, connect.NewError(connect.CodeDataLoss, err)
	}
	f.drop(req.TransferId)
	res.Path, res.Done = p.dest, true
	return res, nil
}

// Get streams a path out of the file root.
func (f *Files) Get(ctx context.Context, req *pb.GetFileRequest, send func(*pb.GetFileResponse) error) error {
	p, err := f.resolve(req.Path)
	if err != nil {
		return err
	}
	src, err := xfer.OpenSource(p, !req.NoCompress)
	if errors.Is(err, fs.ErrNotExist) {
		return connect.NewError(connect.CodeNotFound, err)
	}
	if err != nil {
		return connect.NewError(connect.CodeFailedPrecondition, err)
	}
	defer src.Close()
	h := src.Header()
	if err := send(&pb.GetFileResponse{Part: &pb.GetFileResponse_Header{Header: &pb.FileHeader{Kind: kindProto(h.Kind), Mode: h.Mode, Size: h.Size, Path: p}}}); err != nil {
		return err
	}
	for {
		if err := ctx.Err(); err != nil {
			return err
		}
		c, err := src.Next()
		if errors.Is(err, io.EOF) {
			break
		}
		if err != nil {
			return connect.NewError(connect.CodeInternal, err)
		}
		if err := send(&pb.GetFileResponse{Part: &pb.GetFileResponse_Chunk{Chunk: &pb.FileChunk{Data: c.Data, Compressed: c.Compressed, RawSize: c.RawSize}}}); err != nil {
			return err
		}
	}
	t, err := src.Trailer()
	if err != nil {
		return connect.NewError(connect.CodeAborted, err)
	}
	return send(&pb.GetFileResponse{Part: &pb.GetFileResponse_Trailer{Trailer: &pb.FileTrailer{Size: t.Size, Sha256: t.SHA256, Skipped: t.Skipped}}})
}
