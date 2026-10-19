package main

import (
	"bytes"
	"context"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/node"
)

func TestParseDeskArgs(t *testing.T) {
	tests := []struct {
		name    string
		verb    string
		args    []string
		flags   deskFlags
		check   func(*pb.DeskActionRequest) bool
		wantErr string
	}{
		{"screenshot", "screenshot", nil, deskFlags{}, func(a *pb.DeskActionRequest) bool { return a.GetScreenshot() != nil }, ""},
		{"windows", "windows", nil, deskFlags{}, func(a *pb.DeskActionRequest) bool { return a.GetListWindows() != nil }, ""},
		{"click", "click", []string{"10", "-20"}, deskFlags{button: "right", double: true}, func(a *pb.DeskActionRequest) bool {
			c := a.GetClick()
			return c.X == 10 && c.Y == -20 && c.Button == "right" && c.Double
		}, ""},
		{"move", "move", []string{"1", "2"}, deskFlags{}, func(a *pb.DeskActionRequest) bool { return a.GetMove().X == 1 && a.GetMove().Y == 2 }, ""},
		{"type joins words", "type", []string{"hello", "world"}, deskFlags{}, func(a *pb.DeskActionRequest) bool { return a.GetType().Text == "hello world" }, ""},
		{"key", "key", []string{"ctrl+s"}, deskFlags{}, func(a *pb.DeskActionRequest) bool { return a.GetKey().Chord == "ctrl+s" }, ""},
		{"run with args", "run", []string{"notepad.exe", "-x", "a.txt"}, deskFlags{}, func(a *pb.DeskActionRequest) bool {
			r := a.GetRun()
			return r.Program == "notepad.exe" && strings.Join(r.Args, " ") == "-x a.txt"
		}, ""},
		{"click missing y", "click", []string{"1"}, deskFlags{}, nil, "usage"},
		{"click not a number", "click", []string{"a", "2"}, deskFlags{}, nil, "not an integer"},
		{"click bad button", "click", []string{"1", "2"}, deskFlags{button: "back"}, nil, "button"},
		{"key bad chord", "key", []string{"ctrl+nonsense"}, deskFlags{}, nil, "unknown key"},
		{"key none", "key", nil, deskFlags{}, nil, "usage"},
		{"run none", "run", nil, deskFlags{}, nil, "usage"},
		{"type none", "type", nil, deskFlags{}, nil, "usage"},
		{"screenshot extra", "screenshot", []string{"x"}, deskFlags{}, nil, "no arguments"},
		{"unknown verb", "dance", nil, deskFlags{}, nil, "unknown desk command"},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got, err := parseDeskArgs(tc.verb, tc.args, tc.flags)
			if tc.wantErr != "" {
				if err == nil || !strings.Contains(err.Error(), tc.wantErr) {
					t.Fatalf("err = %v, want %q", err, tc.wantErr)
				}
				return
			}
			if err != nil || !tc.check(got) {
				t.Fatalf("got %v, %v", got, err)
			}
		})
	}
}

type fileHub struct {
	protectoratev1connect.UnimplementedNodeServiceHandler
	files *node.Files
}

func (f fileHub) PutFile(_ context.Context, r *connect.Request[pb.PutFileRequest]) (*connect.Response[pb.PutFileResponse], error) {
	res, err := f.files.Put(r.Msg)
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(res), nil
}

func (f fileHub) GetFile(ctx context.Context, r *connect.Request[pb.GetFileRequest], s *connect.ServerStream[pb.GetFileResponse]) error {
	return f.files.Get(ctx, r.Msg, s.Send)
}

func TestPutGetRoundTrip(t *testing.T) {
	files, err := node.NewFiles(filepath.Join(t.TempDir(), "root"), false)
	if err != nil {
		t.Fatal(err)
	}
	mux := http.NewServeMux()
	mux.Handle(protectoratev1connect.NewNodeServiceHandler(fileHub{files: files}))
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	cl := protectoratev1connect.NewNodeServiceClient(srv.Client(), srv.URL)
	ctx := context.Background()

	work := t.TempDir()
	payload := bytes.Repeat([]byte("line of text\n"), 700000)
	_ = os.WriteFile(filepath.Join(work, "in.txt"), payload, 0o600)
	_ = os.MkdirAll(filepath.Join(work, "dir", "sub"), 0o755)
	_ = os.WriteFile(filepath.Join(work, "dir", "sub", "n.txt"), []byte("nested"), 0o644)

	res, wire, total, err := putFile(ctx, cl, "n1", filepath.Join(work, "in.txt"), "up/in.txt", transferFlags{})
	if err != nil || !res.Done || total != uint64(len(payload)) {
		t.Fatalf("put: %v %v %d", res, err, total)
	}
	if wire >= int64(len(payload))/4 {
		t.Fatalf("text sent as %d of %d bytes: not compressed", wire, len(payload))
	}
	if _, _, _, err := putFile(ctx, cl, "n1", filepath.Join(work, "in.txt"), "up/in.txt", transferFlags{}); connect.CodeOf(err) != connect.CodeAlreadyExists {
		t.Fatalf("second put: %v", err)
	}
	if _, _, _, err := putFile(ctx, cl, "n1", filepath.Join(work, "dir"), "up/dir", transferFlags{noCompress: true}); err != nil {
		t.Fatal(err)
	}

	out := filepath.Join(work, "out.txt")
	tr, gwire, err := getFile(ctx, cl, "n1", "up/in.txt", out, transferFlags{})
	if err != nil || tr.Size != uint64(len(payload)) || gwire >= int64(len(payload))/4 {
		t.Fatalf("get: %v %v %d", tr, err, gwire)
	}
	if b, _ := os.ReadFile(out); !bytes.Equal(b, payload) {
		t.Fatal("downloaded content differs")
	}
	if _, _, err := getFile(ctx, cl, "n1", "up/in.txt", out, transferFlags{}); err == nil || !strings.Contains(err.Error(), "--force") {
		t.Fatalf("get over existing file: %v", err)
	}
	outDir := filepath.Join(work, "outdir")
	if _, _, err := getFile(ctx, cl, "n1", "up/dir", outDir, transferFlags{}); err != nil {
		t.Fatal(err)
	}
	if b, _ := os.ReadFile(filepath.Join(outDir, "sub", "n.txt")); string(b) != "nested" {
		t.Fatalf("dir download = %q", b)
	}
	if _, _, err := getFile(ctx, cl, "n1", "../etc/passwd", filepath.Join(work, "pw"), transferFlags{}); connect.CodeOf(err) != connect.CodePermissionDenied {
		t.Fatalf("escape: %v", err)
	}
	if _, err := os.Stat(filepath.Join(work, "pw")); err == nil {
		t.Fatal("a refused get created the destination")
	}
	if left, _ := filepath.Glob(filepath.Join(work, ".protectorate-put-*")); len(left) != 0 {
		t.Fatalf("temp files left: %v", left)
	}
}
