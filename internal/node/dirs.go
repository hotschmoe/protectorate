package node

import (
	"context"
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"time"

	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

func measureDirs(ctx context.Context, paths []string, budget time.Duration) []*pb.DirSize {
	out := make([]*pb.DirSize, 0, len(paths))
	for _, p := range paths {
		if ctx.Err() != nil {
			break
		}
		out = append(out, dirSize(ctx, expandPath(p), budget))
	}
	return out
}

func expandPath(p string) string {
	p = os.ExpandEnv(p)
	if p == "~" || strings.HasPrefix(p, "~/") || strings.HasPrefix(p, `~\`) {
		if home, err := os.UserHomeDir(); err == nil {
			p = filepath.Join(home, p[1:])
		}
	}
	return p
}

// dirSize sums regular-file sizes below path without following symlinks. When
// the budget ends the walk the result is a lower bound and not complete.
func dirSize(ctx context.Context, path string, budget time.Duration) *pb.DirSize {
	start := time.Now()
	ctx, cancel := context.WithTimeout(ctx, budget)
	defer cancel()
	ds := &pb.DirSize{Path: path, Complete: true}
	var size int64
	var firstErr error
	n := 0
	err := filepath.WalkDir(path, func(p string, d fs.DirEntry, err error) error {
		if err != nil {
			if p == path {
				return err
			}
			if firstErr == nil {
				firstErr = err
			}
			if d != nil && d.IsDir() {
				return fs.SkipDir
			}
			return nil
		}
		if n++; n%256 == 0 && ctx.Err() != nil {
			return ctx.Err()
		}
		if d.Type().IsRegular() {
			if info, err := d.Info(); err == nil {
				size += info.Size()
			}
		}
		return nil
	})
	ds.SizeBytes = size
	ds.MeasuredAt = timestamppb.Now()
	ds.Took = durationpb.New(time.Since(start))
	switch {
	case errors.Is(err, context.DeadlineExceeded), errors.Is(err, context.Canceled):
		ds.Complete = false
	case err != nil:
		ds.Complete, ds.Error = false, err.Error()
	case firstErr != nil:
		ds.Complete, ds.Error = false, "unreadable entries skipped: "+firstErr.Error()
	}
	return ds
}
