package node

import (
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"io"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/xfer"
)

func newTransferID() (string, error) {
	var b [12]byte
	if _, err := rand.Read(b[:]); err != nil {
		return "", err
	}
	return "xfer-" + hex.EncodeToString(b[:]), nil
}

// Push sends the file or directory at local as a chunked transfer through
// send. base carries the destination (Name, AllowCare, Path, Overwrite);
// Push fills in the transfer id, kind, mode, offsets and chunks. progress, if
// set, is called with the uncompressed bytes sent so far after each chunk.
// wire counts the bytes actually sent, after compression.
func Push(local string, compress bool, base *pb.PutFileRequest, send func(*pb.PutFileRequest) (*pb.PutFileResponse, error), progress func(sent uint64)) (res *pb.PutFileResponse, wire int64, total uint64, err error) {
	src, err := xfer.OpenSource(local, compress)
	if err != nil {
		return nil, 0, 0, fmt.Errorf("open %s: %w", local, err)
	}
	defer src.Close()
	id, err := newTransferID()
	if err != nil {
		return nil, 0, 0, err
	}
	h := src.Header()
	kind := pb.FileKind_FILE_KIND_FILE
	if h.Kind == xfer.KindDir {
		kind = pb.FileKind_FILE_KIND_DIRECTORY
	}
	call := func(req *pb.PutFileRequest) (*pb.PutFileResponse, error) {
		req.Name, req.AllowCare, req.TransferId, req.Path, req.Kind, req.Mode, req.Overwrite = base.Name, base.AllowCare, id, base.Path, kind, h.Mode, base.Overwrite
		return send(req)
	}
	for {
		c, err := src.Next()
		if errors.Is(err, io.EOF) {
			break
		}
		if err != nil {
			return nil, wire, total, fmt.Errorf("read %s: %w", local, err)
		}
		wire += int64(len(c.Data))
		if _, err := call(&pb.PutFileRequest{Offset: total, Chunk: &pb.FileChunk{Data: c.Data, Compressed: c.Compressed, RawSize: c.RawSize}}); err != nil {
			return nil, wire, total, fmt.Errorf("send to %s: %w", base.Name, err)
		}
		total += uint64(c.RawSize)
		if progress != nil {
			progress(total)
		}
	}
	t, err := src.Trailer()
	if err != nil {
		return nil, wire, total, err
	}
	res, err = call(&pb.PutFileRequest{Offset: total, Last: true, Trailer: &pb.FileTrailer{Size: t.Size, Sha256: t.SHA256, Skipped: t.Skipped}})
	if err != nil {
		return nil, wire, total, fmt.Errorf("finish transfer to %s: %w", base.Name, err)
	}
	return res, wire, total, nil
}
