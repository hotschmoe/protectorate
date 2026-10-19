package node

import (
	"context"
	"errors"
	"image"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/desk"
)

func TestFilesResolve(t *testing.T) {
	f, err := NewFiles(filepath.Join(t.TempDir(), "root"), false)
	if err != nil {
		t.Fatal(err)
	}
	outside := t.TempDir()
	linkErr := os.Symlink(outside, filepath.Join(f.Root(), "link"))
	tests := []struct {
		name     string
		path     string
		wantCode connect.Code
		want     string
		skip     bool
	}{
		{"relative", "a/b.txt", 0, filepath.Join(f.Root(), "a", "b.txt"), false},
		{"absolute inside", filepath.Join(f.Root(), "x"), 0, filepath.Join(f.Root(), "x"), false},
		{"root itself", f.Root(), 0, f.Root(), false},
		{"dot dot out", "../x", connect.CodePermissionDenied, "", false},
		{"dot dot sneaking", "a/../../x", connect.CodePermissionDenied, "", false},
		{"absolute outside", filepath.Join(outside, "x"), connect.CodePermissionDenied, "", false},
		{"sibling with shared prefix", f.Root() + "-evil", connect.CodePermissionDenied, "", false},
		{"through symlink", "link/x", connect.CodePermissionDenied, "", linkErr != nil},
		{"empty", "", connect.CodeInvalidArgument, "", false},
		{"nul", "a\x00b", connect.CodeInvalidArgument, "", false},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			if tc.skip {
				t.Skip("symlinks unavailable")
			}
			got, err := f.resolve(tc.path)
			if tc.wantCode != 0 {
				if connect.CodeOf(err) != tc.wantCode {
					t.Fatalf("err = %v, want %v", err, tc.wantCode)
				}
				return
			}
			if err != nil || got != tc.want {
				t.Fatalf("resolve = %q, %v; want %q", got, err, tc.want)
			}
		})
	}

	u, err := NewFiles(filepath.Join(t.TempDir(), "root"), true)
	if err != nil {
		t.Fatal(err)
	}
	if got, err := u.resolve(filepath.Join(outside, "x")); err != nil || got != filepath.Join(outside, "x") {
		t.Fatalf("unconfined absolute: %q %v", got, err)
	}
}

func TestFilesPutProtocol(t *testing.T) {
	f, err := NewFiles(filepath.Join(t.TempDir(), "root"), false)
	if err != nil {
		t.Fatal(err)
	}
	chunk := &pb.FileChunk{Data: []byte("hello"), RawSize: 5}
	start := func(id string) *pb.PutFileRequest {
		return &pb.PutFileRequest{TransferId: id, Path: "f", Kind: pb.FileKind_FILE_KIND_FILE, Mode: 0o600, Chunk: chunk}
	}
	tests := []struct {
		name string
		do   func() error
		want connect.Code
	}{
		{"short id", func() error { _, err := f.Put(start("abc")); return err }, connect.CodeInvalidArgument},
		{"unknown transfer", func() error {
			_, err := f.Put(&pb.PutFileRequest{TransferId: "xfer-unknown1", Offset: 5, Chunk: chunk})
			return err
		}, connect.CodeNotFound},
		{"bad kind", func() error {
			r := start("xfer-badkind1")
			r.Kind = 0
			_, err := f.Put(r)
			return err
		}, connect.CodeInvalidArgument},
		{"offset mismatch", func() error {
			if _, err := f.Put(start("xfer-offset01")); err != nil {
				return err
			}
			_, err := f.Put(&pb.PutFileRequest{TransferId: "xfer-offset01", Offset: 99, Chunk: chunk})
			return err
		}, connect.CodeInvalidArgument},
		{"last without trailer", func() error {
			r := start("xfer-notrail1")
			r.Last = true
			_, err := f.Put(r)
			return err
		}, connect.CodeInvalidArgument},
		{"wrong hash", func() error {
			r := start("xfer-wronghash")
			r.Last, r.Trailer = true, &pb.FileTrailer{Size: 5, Sha256: strings.Repeat("0", 64)}
			_, err := f.Put(r)
			return err
		}, connect.CodeDataLoss},
		{"duplicate id", func() error {
			if _, err := f.Put(start("xfer-dupe0001")); err != nil {
				return err
			}
			_, err := f.Put(start("xfer-dupe0001"))
			return err
		}, connect.CodeAlreadyExists},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			if err := tc.do(); connect.CodeOf(err) != tc.want {
				t.Fatalf("err = %v, want %v", err, tc.want)
			}
		})
	}
	if _, err := os.Stat(filepath.Join(f.Root(), "f")); err == nil {
		t.Fatal("a failed transfer produced the destination")
	}

	t.Run("stale transfers are swept", func(t *testing.T) {
		r := start("xfer-stale0001")
		r.Path = "stale"
		if _, err := f.Put(r); err != nil {
			t.Fatal(err)
		}
		f.mu.Lock()
		p := f.puts["xfer-stale0001"]
		f.mu.Unlock()
		p.mu.Lock()
		p.last = time.Now().Add(-2 * transferIdle)
		p.mu.Unlock()
		if _, err := f.Put(start("xfer-fresh0001")); err != nil {
			t.Fatal(err)
		}
		f.mu.Lock()
		_, still := f.puts["xfer-stale0001"]
		f.mu.Unlock()
		if still {
			t.Fatal("stale transfer survived")
		}
	})
}

type stubDesk struct {
	ready error
	calls int
}

func (s *stubDesk) Ready() error { return s.ready }
func (s *stubDesk) Screenshot() (*image.RGBA, error) {
	s.calls++
	return image.NewRGBA(image.Rect(0, 0, 2, 2)), nil
}
func (s *stubDesk) Foreground() (desk.Foreground, error)    { s.calls++; return desk.Foreground{}, nil }
func (s *stubDesk) Windows() ([]desk.Window, error)         { s.calls++; return nil, nil }
func (s *stubDesk) Click(int, int, desk.Button, bool) error { s.calls++; return nil }
func (s *stubDesk) Move(int, int) error                     { s.calls++; return nil }
func (s *stubDesk) Type(string) error                       { s.calls++; return nil }
func (s *stubDesk) Key(desk.Chord) error                    { s.calls++; return nil }
func (s *stubDesk) Run(string, []string) (uint32, error)    { s.calls++; return 1, nil }

func TestDeskErrorsAndValidation(t *testing.T) {
	click := &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Click{Click: &pb.ClickRequest{X: 1, Y: 1}}}
	tests := []struct {
		name  string
		ready error
		req   *pb.DeskActionRequest
		want  connect.Code
	}{
		{"ok", nil, click, 0},
		{"session 0", desk.ErrNotInteractive, click, connect.CodeFailedPrecondition},
		{"unsupported os", desk.ErrUnsupported, click, connect.CodeUnimplemented},
		{"bad button before session check", desk.ErrNotInteractive, &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Click{Click: &pb.ClickRequest{Button: "x"}}}, connect.CodeInvalidArgument},
		{"bad chord", nil, &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Key{Key: &pb.KeyRequest{Chord: "ctrl+"}}}, connect.CodeInvalidArgument},
		{"empty program", nil, &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Run{Run: &pb.RunRequest{}}}, connect.CodeInvalidArgument},
		{"no action", nil, &pb.DeskActionRequest{}, connect.CodeInvalidArgument},
		{"screenshot not interactive", desk.ErrNotInteractive, &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Screenshot{Screenshot: &pb.ScreenshotRequest{}}}, connect.CodeFailedPrecondition},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			s := &stubDesk{ready: tc.ready}
			_, err := (&Desk{D: s}).Do(context.Background(), tc.req)
			if tc.want == 0 {
				if err != nil {
					t.Fatal(err)
				}
				return
			}
			var ce *connect.Error
			if !errors.As(err, &ce) || ce.Code() != tc.want {
				t.Fatalf("err = %v, want %v", err, tc.want)
			}
			if s.calls != 0 {
				t.Fatalf("driver called %d times despite the error", s.calls)
			}
		})
	}
}
