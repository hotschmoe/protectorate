package main

import (
	"context"
	"errors"
	"fmt"
	"io"
	"time"

	"connectrpc.com/connect"
	"github.com/spf13/cobra"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/node"
	"github.com/hotschmoe/protectorate/internal/xfer"
)

type transferFlags struct {
	overwrite, noCompress, allowFamily bool
}

func (t *transferFlags) bind(c *cobra.Command) {
	f := c.Flags()
	f.BoolVarP(&t.overwrite, "force", "f", false, "replace an existing destination")
	f.BoolVar(&t.noCompress, "no-compress", false, "send chunks uncompressed (already-compressed data is detected and sent as is anyway)")
	f.BoolVar(&t.allowFamily, "allow-family", false, "allow nodes of tier family")
}

func nodePutCmd(o *opts) *cobra.Command {
	var tf transferFlags
	c := &cobra.Command{
		Use:   "put <node> <local> <remote>",
		Short: "Copy a file or directory to a node",
		Long: `Copy <local> to <remote> on the node, through the hub. A directory is sent as
a tar stream (symlinks are skipped). Data is zstd-compressed per chunk only
where that makes it smaller, then verified by size and SHA-256 and moved into
place atomically. A relative <remote> is under the node's file root; absolute
paths must stay inside it unless the node agent runs with --file-unconfined.
<remote> is the destination path itself, not a directory to copy into.`,
		Args: cobra.ExactArgs(3),
		RunE: func(cmd *cobra.Command, args []string) error {
			cl, err := o.nodeClient()
			if err != nil {
				return err
			}
			start := time.Now()
			res, wire, total, err := putFile(cmd.Context(), cl, args[0], args[1], args[2], tf)
			if err != nil {
				return err
			}
			return o.out(cmd).print(res, func(w io.Writer) {
				fmt.Fprintf(w, "put\t%s:%s\nbytes\t%d (%s sent, %s)\n", args[0], res.Path, total, fmtBytes(uint64(wire)), time.Since(start).Round(time.Millisecond))
			})
		},
	}
	tf.bind(c)
	return c
}

type putClient interface {
	PutFile(context.Context, *connect.Request[pb.PutFileRequest]) (*connect.Response[pb.PutFileResponse], error)
}

func putFile(ctx context.Context, cl putClient, name, local, remote string, tf transferFlags) (*pb.PutFileResponse, int64, uint64, error) {
	base := &pb.PutFileRequest{Name: name, AllowFamily: tf.allowFamily, Path: remote, Overwrite: tf.overwrite}
	return node.Push(local, !tf.noCompress, base, func(req *pb.PutFileRequest) (*pb.PutFileResponse, error) {
		r, err := cl.PutFile(ctx, connect.NewRequest(req))
		if err != nil {
			return nil, err
		}
		return r.Msg, nil
	}, nil)
}

func nodeGetCmd(o *opts) *cobra.Command {
	var tf transferFlags
	c := &cobra.Command{
		Use:   "get <node> <remote> <local>",
		Short: "Copy a file or directory from a node",
		Long: `Copy <remote> on the node to <local>, through the hub, with the same
compression, verification and path rules as put. <local> is the destination
path itself. It is written to a temporary file next to it and renamed only
after size and SHA-256 match.`,
		Args: cobra.ExactArgs(3),
		RunE: func(cmd *cobra.Command, args []string) error {
			cl, err := o.nodeClient()
			if err != nil {
				return err
			}
			start := time.Now()
			t, wire, err := getFile(cmd.Context(), cl, args[0], args[1], args[2], tf)
			if err != nil {
				return err
			}
			return o.out(cmd).print(t, func(w io.Writer) {
				fmt.Fprintf(w, "get\t%s:%s\nbytes\t%d (%s received, %s)\n", args[0], args[1], t.Size, fmtBytes(uint64(wire)), time.Since(start).Round(time.Millisecond))
				if t.Skipped > 0 {
					fmt.Fprintf(w, "skipped\t%d entries (symlinks or special files)\n", t.Skipped)
				}
			})
		},
	}
	tf.bind(c)
	return c
}

type getClient interface {
	GetFile(context.Context, *connect.Request[pb.GetFileRequest]) (*connect.ServerStreamForClient[pb.GetFileResponse], error)
}

func getFile(ctx context.Context, cl getClient, node, remote, local string, tf transferFlags) (t *pb.FileTrailer, wire int64, err error) {
	stream, err := cl.GetFile(ctx, connect.NewRequest(&pb.GetFileRequest{Name: node, AllowFamily: tf.allowFamily, Path: remote, NoCompress: tf.noCompress}))
	if err != nil {
		return nil, 0, err
	}
	defer stream.Close()
	var sink *xfer.Sink
	var hdr *pb.FileHeader
	defer func() {
		if sink != nil && err != nil {
			sink.Abort()
		}
	}()
	for stream.Receive() {
		switch p := stream.Msg().Part.(type) {
		case *pb.GetFileResponse_Header:
			hdr = p.Header
			kind := xfer.KindFile
			if hdr.Kind == pb.FileKind_FILE_KIND_DIRECTORY {
				kind = xfer.KindDir
			}
			if sink, err = xfer.NewSink(local, kind, hdr.Mode, tf.overwrite); err != nil {
				if errors.Is(err, xfer.ErrExists) {
					err = fmt.Errorf("%w (use --force to replace it)", err)
				}
				return nil, wire, err
			}
		case *pb.GetFileResponse_Chunk:
			if sink == nil {
				return nil, wire, errors.New("hub sent data before the header")
			}
			wire += int64(len(p.Chunk.Data))
			if err = sink.Write(xfer.Chunk{Data: p.Chunk.Data, Compressed: p.Chunk.Compressed, RawSize: p.Chunk.RawSize}); err != nil {
				return nil, wire, err
			}
		case *pb.GetFileResponse_Trailer:
			if sink == nil {
				return nil, wire, errors.New("hub sent a trailer before the header")
			}
			if hdr.Kind == pb.FileKind_FILE_KIND_FILE && hdr.Size != p.Trailer.Size {
				return nil, wire, fmt.Errorf("size mismatch: header says %d bytes, trailer %d", hdr.Size, p.Trailer.Size)
			}
			if err = sink.Commit(xfer.Trailer{Size: p.Trailer.Size, SHA256: p.Trailer.Sha256}); err != nil {
				return nil, wire, err
			}
			t = p.Trailer
		}
	}
	if err = stream.Err(); err != nil {
		return nil, wire, err
	}
	if t == nil {
		err = errors.New("transfer ended without a trailer")
		return nil, wire, err
	}
	return t, wire, nil
}
