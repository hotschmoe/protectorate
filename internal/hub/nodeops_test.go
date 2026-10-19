package hub

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"image"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"connectrpc.com/connect"
	"github.com/klauspost/compress/zstd"
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/desk"
	"github.com/hotschmoe/protectorate/internal/xfer"
)

type fakeDesk struct {
	mu    sync.Mutex
	ready error
	calls []string
}

func (f *fakeDesk) rec(s string) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.calls = append(f.calls, s)
	return nil
}

func (f *fakeDesk) got() []string {
	f.mu.Lock()
	defer f.mu.Unlock()
	return append([]string(nil), f.calls...)
}

func (f *fakeDesk) Ready() error { return f.ready }
func (f *fakeDesk) Screenshot() (*image.RGBA, error) {
	return image.NewRGBA(image.Rect(0, 0, 4, 3)), f.rec("screenshot")
}
func (f *fakeDesk) Windows() ([]desk.Window, error) {
	return []desk.Window{{Title: "ArchiCAD", X: 1, Y: 2, Width: 300, Height: 200, PID: 42}}, f.rec("windows")
}
func (f *fakeDesk) Click(x, y int, b desk.Button, double bool) error { return f.rec("click") }
func (f *fakeDesk) Move(x, y int) error                              { return f.rec("move") }
func (f *fakeDesk) Type(s string) error                              { return f.rec("type " + s) }
func (f *fakeDesk) Key(c desk.Chord) error                           { return f.rec("key") }
func (f *fakeDesk) Run(p string, a []string) (uint32, error)         { return 7, f.rec("run " + p) }

// putPath sends src as chunk calls like the CLI does.
func putPath(t *testing.T, c protectoratev1connect.NodeServiceClient, node, src, remote string, overwrite bool) (*pb.PutFileResponse, error) {
	t.Helper()
	s, err := xfer.OpenSource(src, true)
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	kind := pb.FileKind_FILE_KIND_FILE
	if s.Header().Kind == xfer.KindDir {
		kind = pb.FileKind_FILE_KIND_DIRECTORY
	}
	id := "xfer-" + strings.ReplaceAll(strings.ToLower(t.Name()), "/", "-")
	call := func(r *pb.PutFileRequest) (*pb.PutFileResponse, error) {
		r.Name, r.TransferId, r.Path, r.Kind, r.Mode, r.Overwrite = node, id, remote, kind, s.Header().Mode, overwrite
		res, err := c.PutFile(context.Background(), connect.NewRequest(r))
		if err != nil {
			return nil, err
		}
		return res.Msg, nil
	}
	var off uint64
	for {
		ch, err := s.Next()
		if err == io.EOF {
			break
		}
		if err != nil {
			t.Fatal(err)
		}
		if _, err := call(&pb.PutFileRequest{Offset: off, Chunk: &pb.FileChunk{Data: ch.Data, Compressed: ch.Compressed, RawSize: ch.RawSize}}); err != nil {
			return nil, err
		}
		off += uint64(ch.RawSize)
	}
	tr, err := s.Trailer()
	if err != nil {
		t.Fatal(err)
	}
	return call(&pb.PutFileRequest{Offset: off, Last: true, Trailer: &pb.FileTrailer{Size: tr.Size, Sha256: tr.SHA256}})
}

func getPath(c protectoratev1connect.NodeServiceClient, node, remote string) (hdr *pb.FileHeader, data []byte, tr *pb.FileTrailer, err error) {
	st, err := c.GetFile(context.Background(), connect.NewRequest(&pb.GetFileRequest{Name: node, Path: remote}))
	if err != nil {
		return nil, nil, nil, err
	}
	defer st.Close()
	for st.Receive() {
		switch p := st.Msg().Part.(type) {
		case *pb.GetFileResponse_Header:
			hdr = p.Header
		case *pb.GetFileResponse_Chunk:
			b := p.Chunk.Data
			if p.Chunk.Compressed {
				b, err = zstdDecode(b)
				if err != nil {
					return nil, nil, nil, err
				}
			}
			data = append(data, b...)
		case *pb.GetFileResponse_Trailer:
			tr = p.Trailer
		}
	}
	return hdr, data, tr, st.Err()
}

func TestFileTransferThroughHub(t *testing.T) {
	e := newJobEnv(t, 1)
	hc := &http.Client{Transport: bearer{testToken}}
	nc := protectoratev1connect.NewNodeServiceClient(hc, e.srv.URL)
	work := t.TempDir()
	big := bytes.Repeat([]byte("compress me please\n"), 600000)
	if err := os.WriteFile(filepath.Join(work, "big.txt"), big, 0o640); err != nil {
		t.Fatal(err)
	}
	tree := filepath.Join(work, "tree")
	_ = os.MkdirAll(filepath.Join(tree, "sub"), 0o755)
	_ = os.WriteFile(filepath.Join(tree, "sub", "a.txt"), []byte("alpha"), 0o644)

	for _, tc := range []struct{ node, root string }{{"peer", e.peerRoot}, {"hubhost", e.hubRoot}} {
		t.Run(tc.node, func(t *testing.T) {
			res, err := putPath(t, nc, tc.node, filepath.Join(work, "big.txt"), "in/big.txt", false)
			if err != nil {
				t.Fatal(err)
			}
			if !res.Done || res.Path != filepath.Join(tc.root, "in", "big.txt") {
				t.Fatalf("response = %v", res)
			}
			if got, _ := os.ReadFile(res.Path); !bytes.Equal(got, big) {
				t.Fatal("remote content differs")
			}
			if _, err := putPath(t, nc, tc.node, filepath.Join(work, "big.txt"), "in/big.txt", false); connect.CodeOf(err) != connect.CodeAlreadyExists {
				t.Fatalf("second put err = %v, want AlreadyExists", err)
			}
			if _, err := putPath(t, nc, tc.node, filepath.Join(work, "big.txt"), "in/big.txt", true); err != nil {
				t.Fatalf("overwrite: %v", err)
			}
			hdr, data, tr, err := getPath(nc, tc.node, "in/big.txt")
			if err != nil {
				t.Fatal(err)
			}
			sum := sha256.Sum256(big)
			if hdr.Kind != pb.FileKind_FILE_KIND_FILE || !bytes.Equal(data, big) || tr.Sha256 != hex.EncodeToString(sum[:]) {
				t.Fatalf("download mismatch: %v %v", hdr, tr)
			}
			if _, err := putPath(t, nc, tc.node, tree, "trees/t1", false); err != nil {
				t.Fatalf("dir put: %v", err)
			}
			if got, _ := os.ReadFile(filepath.Join(tc.root, "trees", "t1", "sub", "a.txt")); string(got) != "alpha" {
				t.Fatalf("dir content = %q", got)
			}
			if hdr, _, _, err := getPath(nc, tc.node, "trees/t1"); err != nil || hdr.Kind != pb.FileKind_FILE_KIND_DIRECTORY {
				t.Fatalf("dir get: %v %v", hdr, err)
			}
		})
	}

	outside := filepath.Join(t.TempDir(), "x")
	_ = os.WriteFile(outside, []byte("secret"), 0o600)
	link := filepath.Join(e.peerRoot, "escape")
	linkErr := os.Symlink(filepath.Dir(outside), link)
	tests := []struct {
		name string
		path string
		skip bool
	}{
		{"parent traversal", "../outside", false},
		{"absolute outside", outside, false},
		{"symlink out of root", "escape/x", linkErr != nil},
	}
	for _, tc := range tests {
		t.Run("confined "+tc.name, func(t *testing.T) {
			if tc.skip {
				t.Skip("symlinks unavailable")
			}
			if _, err := putPath(t, nc, "peer", filepath.Join(work, "big.txt"), tc.path, true); connect.CodeOf(err) != connect.CodePermissionDenied {
				t.Fatalf("put err = %v, want PermissionDenied", err)
			}
			if _, _, _, err := getPath(nc, "peer", tc.path); connect.CodeOf(err) != connect.CodePermissionDenied {
				t.Fatalf("get err = %v, want PermissionDenied", err)
			}
		})
	}
	if _, _, _, err := getPath(nc, "peer", "missing"); connect.CodeOf(err) != connect.CodeNotFound {
		t.Fatalf("missing err = %v", err)
	}
	if _, _, _, err := getPath(nc, "nonode", "x"); connect.CodeOf(err) != connect.CodeNotFound {
		t.Fatalf("unknown node err = %v", err)
	}
}

func TestFileScopesAndAudit(t *testing.T) {
	e := newJobEnv(t, 1)
	store, err := auth.OpenStore(filepath.Join(e.home, "tokens.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	au := auth.NewAudit(filepath.Join(e.home, "audit"))
	t.Cleanup(func() { au.Close() })
	e.h.SetAuth(auth.New(testToken, store, au))
	hs := httptest.NewServer(e.h.Handler())
	t.Cleanup(hs.Close)
	srv := hs.URL
	mint := func(name string, scopes ...string) protectoratev1connect.NodeServiceClient {
		_, secret, err := store.Create(name, scopes, time.Hour, time.Now())
		if err != nil {
			t.Fatal(err)
		}
		return protectoratev1connect.NewNodeServiceClient(&http.Client{Transport: bearer{secret}}, srv)
	}
	src := filepath.Join(t.TempDir(), "f")
	_ = os.WriteFile(src, bytes.Repeat([]byte("0123456789abcdef"), 3*xfer.ChunkSize/16), 0o600)

	reader, putter, getter := mint("reader", "read"), mint("putter", "node_put_*"), mint("getter", "node_get_file")
	if _, err := putPath(t, reader, "peer", src, "f", false); connect.CodeOf(err) != connect.CodePermissionDenied {
		t.Fatalf("read token put err = %v", err)
	}
	if _, _, _, err := getPath(reader, "peer", "f"); connect.CodeOf(err) != connect.CodePermissionDenied {
		t.Fatalf("read token get err = %v", err)
	}
	if _, _, _, err := getPath(putter, "peer", "f"); connect.CodeOf(err) != connect.CodePermissionDenied {
		t.Fatalf("put-only token get err = %v", err)
	}
	if _, err := putPath(t, putter, "peer", src, "f", false); err != nil {
		t.Fatalf("put with node_put_*: %v", err)
	}
	if _, err := putPath(t, getter, "peer", src, "g", false); connect.CodeOf(err) != connect.CodePermissionDenied {
		t.Fatalf("get-only token put err = %v", err)
	}
	if _, _, _, err := getPath(getter, "peer", "f"); err != nil {
		t.Fatalf("get with node_get_file: %v", err)
	}

	au.Close()
	entries, _ := filepath.Glob(filepath.Join(e.home, "audit", "audit-*.jsonl"))
	var log []byte
	for _, p := range entries {
		b, _ := os.ReadFile(p)
		log = append(log, b...)
	}
	count := func(proc, outcome string) int {
		n := 0
		for _, l := range strings.Split(string(log), "\n") {
			if strings.Contains(l, proc) && strings.Contains(l, `"outcome":"`+outcome+`"`) {
				n++
			}
		}
		return n
	}
	// 3 data chunks plus the closing call: only the first and last are logged.
	if n := count("NodeService/PutFile", "ok"); n != 2 {
		t.Fatalf("ok PutFile audit lines = %d, want 2\n%s", n, log)
	}
	if n := count("NodeService/GetFile", "ok"); n != 1 {
		t.Fatalf("ok GetFile audit lines = %d, want 1\n%s", n, log)
	}
	if count("NodeService/GetFile", "denied") == 0 {
		t.Fatalf("denied get not audited\n%s", log)
	}
}

func TestDeskAutoLease(t *testing.T) {
	e := newJobEnv(t, 1)
	store, err := auth.OpenStore(filepath.Join(e.home, "tokens.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	au := auth.NewAudit(filepath.Join(e.home, "audit"))
	t.Cleanup(func() { au.Close() })
	e.h.SetAuth(auth.New(testToken, store, au))
	hs := httptest.NewServer(e.h.Handler())
	t.Cleanup(hs.Close)
	ctx := context.Background()
	mint := func(name string) protectoratev1connect.DeskServiceClient {
		_, secret, err := store.Create(name, []string{"desk_*", "lease_*"}, time.Hour, time.Now())
		if err != nil {
			t.Fatal(err)
		}
		return protectoratev1connect.NewDeskServiceClient(&http.Client{Transport: bearer{secret}}, hs.URL)
	}
	alice, bob := mint("alice"), mint("bob")
	target := func(lease string) *pb.DeskTarget { return &pb.DeskTarget{Name: "peer", LeaseId: lease} }
	click := func(c protectoratev1connect.DeskServiceClient, lease string) (*connect.Response[pb.InputResponse], error) {
		return c.Click(ctx, connect.NewRequest(&pb.ClickRequest{Target: target(lease), X: 1, Y: 2}))
	}
	holder := func() *pb.Lease {
		for _, l := range e.h.leases.List() {
			if l.Resource == "desk:peer" {
				return l
			}
		}
		return nil
	}
	shift := func(d time.Duration) {
		e.h.leases.mu.Lock()
		defer e.h.leases.mu.Unlock()
		prev := e.h.leases.now
		e.h.leases.now = func() time.Time { return prev().Add(d) }
	}

	res, err := click(alice, "")
	if err != nil {
		t.Fatalf("first click without a lease: %v", err)
	}
	id := res.Header().Get(DeskLeaseHeader)
	l := holder()
	if l == nil || l.Id != id || l.Holder != "auto:token:alice" || l.Ttl.AsDuration() != autoDeskTTL {
		t.Fatalf("auto lease = %v, header %q", l, id)
	}
	if res.Header().Get(DeskLeaseExpiresHeader) == "" {
		t.Error("expiry header missing")
	}

	if _, err := click(bob, ""); connect.CodeOf(err) != connect.CodeFailedPrecondition || !strings.Contains(err.Error(), "auto:token:alice") || !strings.Contains(err.Error(), "expires in") {
		t.Fatalf("other caller: %v", err)
	}
	if _, err := click(bob, id); err != nil {
		t.Fatalf("holding the id is the proof: %v", err)
	}
	if _, err := bob.Screenshot(ctx, connect.NewRequest(&pb.ScreenshotRequest{Target: target("")})); err != nil {
		t.Fatalf("screenshot needs no lease: %v", err)
	}

	shift(90 * time.Second)
	res, err = click(alice, "")
	if err != nil || res.Header().Get(DeskLeaseHeader) != id {
		t.Fatalf("same caller must renew its lease: %v %v", res, err)
	}
	shift(90 * time.Second)
	if holder() == nil {
		t.Fatal("lease expired although it was renewed 90s ago")
	}
	shift(31 * time.Second)
	if holder() != nil {
		t.Fatal("auto lease outlived its ttl")
	}
	if res, err = click(bob, ""); err != nil || res.Header().Get(DeskLeaseHeader) == id {
		t.Fatalf("free again after expiry: %v %v", res, err)
	}

	t.Run("explicit lease is not auto", func(t *testing.T) {
		lc := protectoratev1connect.NewLeaseServiceClient(&http.Client{Transport: bearer{testToken}}, hs.URL)
		if l := holder(); l != nil {
			if _, err := lc.ReleaseLease(ctx, connect.NewRequest(&pb.ReleaseLeaseRequest{Id: l.Id})); err != nil {
				t.Fatal(err)
			}
		}
		long, err := lc.AcquireLease(ctx, connect.NewRequest(&pb.AcquireLeaseRequest{Target: &pb.AcquireLeaseRequest_Resource{Resource: "desk:peer"}, Holder: "me", Ttl: durationpb.New(time.Hour)}))
		if err != nil {
			t.Fatal(err)
		}
		if _, err := click(alice, ""); connect.CodeOf(err) != connect.CodeFailedPrecondition || !strings.Contains(err.Error(), "me") {
			t.Fatalf("explicit lease must block auto: %v", err)
		}
		res, err := click(alice, long.Msg.Id)
		if err != nil || res.Header().Get(DeskLeaseHeader) != "" {
			t.Fatalf("explicit lease use: %v %v", res, err)
		}
		if got := holder().ExpiresAt.AsTime(); !got.Equal(long.Msg.ExpiresAt.AsTime()) {
			t.Errorf("explicit lease renewed by use: %v != %v", got, long.Msg.ExpiresAt.AsTime())
		}
	})

	t.Run("node failure frees a fresh lease", func(t *testing.T) {
		lc := protectoratev1connect.NewLeaseServiceClient(&http.Client{Transport: bearer{testToken}}, hs.URL)
		_, _ = lc.ReleaseLease(ctx, connect.NewRequest(&pb.ReleaseLeaseRequest{Id: holder().Id}))
		e.peerDesk.mu.Lock()
		e.peerDesk.ready = desk.ErrNotInteractive
		e.peerDesk.mu.Unlock()
		defer func() {
			e.peerDesk.mu.Lock()
			e.peerDesk.ready = nil
			e.peerDesk.mu.Unlock()
		}()
		if _, err := click(alice, ""); connect.CodeOf(err) != connect.CodeFailedPrecondition {
			t.Fatalf("err = %v", err)
		}
		if holder() != nil {
			t.Error("failed command left an auto lease behind")
		}
	})
}

func TestDeskLeaseRule(t *testing.T) {
	e := newJobEnv(t, 1)
	hc := &http.Client{Transport: bearer{testToken}}
	dc := protectoratev1connect.NewDeskServiceClient(hc, e.srv.URL)
	ctx := context.Background()
	acquire := func(resource string) (*pb.Lease, error) {
		res, err := e.lc.AcquireLease(ctx, connect.NewRequest(&pb.AcquireLeaseRequest{Target: &pb.AcquireLeaseRequest_Resource{Resource: resource}, Holder: "t", Ttl: durationpb.New(time.Hour)}))
		if err != nil {
			return nil, err
		}
		return res.Msg, nil
	}
	target := func(lease string) *pb.DeskTarget { return &pb.DeskTarget{Name: "peer", LeaseId: lease} }
	click := func(lease string) error {
		_, err := dc.Click(ctx, connect.NewRequest(&pb.ClickRequest{Target: target(lease), X: 1, Y: 2}))
		return err
	}

	shot, err := dc.Screenshot(ctx, connect.NewRequest(&pb.ScreenshotRequest{Target: target("")}))
	if err != nil || shot.Msg.Width != 4 || shot.Msg.Height != 3 || !bytes.HasPrefix(shot.Msg.Png, []byte("\x89PNG")) {
		t.Fatalf("screenshot without lease: %v %v", shot, err)
	}
	wins, err := dc.ListWindows(ctx, connect.NewRequest(&pb.ListWindowsRequest{Target: target("")}))
	if err != nil || len(wins.Msg.Windows) != 1 || wins.Msg.Windows[0].Title != "ArchiCAD" {
		t.Fatalf("windows without lease: %v %v", wins, err)
	}
	nodeLease, err := acquire("peer")
	if err != nil {
		t.Fatal(err)
	}
	if err := click(nodeLease.Id); connect.CodeOf(err) != connect.CodeFailedPrecondition {
		t.Fatalf("click with a node lease: %v", err)
	}
	if err := click("lease-ffffffff"); connect.CodeOf(err) != connect.CodeNotFound {
		t.Fatalf("click with unknown lease: %v", err)
	}
	if _, err := e.lc.ReleaseLease(ctx, connect.NewRequest(&pb.ReleaseLeaseRequest{Id: nodeLease.Id})); err != nil {
		t.Fatal(err)
	}

	dl, err := acquire("desk:peer")
	if err != nil {
		t.Fatal(err)
	}
	if dl.Resource != "desk:peer" {
		t.Fatalf("resource = %q", dl.Resource)
	}
	if _, err := acquire("desk:peer"); connect.CodeOf(err) != connect.CodeFailedPrecondition {
		t.Fatalf("second desk lease: %v", err)
	}
	if other, err := acquire("desk:hubhost"); err != nil {
		t.Fatalf("desk of another node: %v", err)
	} else if err := click(other.Id); connect.CodeOf(err) != connect.CodeFailedPrecondition {
		t.Fatalf("click on peer with hubhost's desk lease: %v", err)
	}
	if _, err := acquire("desk:ghost"); connect.CodeOf(err) != connect.CodeNotFound {
		t.Fatalf("desk of unknown node: %v", err)
	}
	if nl, err := acquire("peer"); err != nil {
		t.Fatalf("a desk lease must not block a node lease: %v", err)
	} else {
		_, _ = e.lc.ReleaseLease(ctx, connect.NewRequest(&pb.ReleaseLeaseRequest{Id: nl.Id}))
	}
	if _, err := e.jc.SubmitJob(ctx, connect.NewRequest(&pb.SubmitJobRequest{Target: &pb.JobTarget{Target: &pb.JobTarget_LeaseId{LeaseId: dl.Id}}, Command: []string{"true"}})); connect.CodeOf(err) != connect.CodeInvalidArgument {
		t.Fatalf("job on a desk lease: %v", err)
	}

	if err := click(dl.Id); err != nil {
		t.Fatalf("click with desk lease: %v", err)
	}
	if _, err := dc.Type(ctx, connect.NewRequest(&pb.TypeRequest{Target: target(dl.Id), Text: "hi"})); err != nil {
		t.Fatal(err)
	}
	if _, err := dc.Key(ctx, connect.NewRequest(&pb.KeyRequest{Target: target(dl.Id), Chord: "ctrl+s"})); err != nil {
		t.Fatal(err)
	}
	if _, err := dc.Move(ctx, connect.NewRequest(&pb.MoveRequest{Target: target(dl.Id), X: 5, Y: 6})); err != nil {
		t.Fatal(err)
	}
	run, err := dc.Run(ctx, connect.NewRequest(&pb.RunRequest{Target: target(dl.Id), Program: "calc.exe"}))
	if err != nil || run.Msg.Pid != 7 {
		t.Fatalf("run: %v %v", run, err)
	}
	want := []string{"screenshot", "windows", "click", "type hi", "key", "move", "run calc.exe"}
	if got := e.peerDesk.got(); strings.Join(got, "|") != strings.Join(want, "|") {
		t.Fatalf("peer desk calls = %v, want %v (denied commands must not reach the node)", got, want)
	}

	t.Run("not interactive", func(t *testing.T) {
		e.peerDesk.mu.Lock()
		e.peerDesk.ready = desk.ErrNotInteractive
		e.peerDesk.mu.Unlock()
		_, err := dc.Screenshot(ctx, connect.NewRequest(&pb.ScreenshotRequest{Target: target("")}))
		if connect.CodeOf(err) != connect.CodeFailedPrecondition {
			t.Fatalf("err = %v", err)
		}
	})
	t.Run("unsupported", func(t *testing.T) {
		e.peerDesk.mu.Lock()
		e.peerDesk.ready = desk.ErrUnsupported
		e.peerDesk.mu.Unlock()
		_, err := dc.Screenshot(ctx, connect.NewRequest(&pb.ScreenshotRequest{Target: target("")}))
		if connect.CodeOf(err) != connect.CodeUnimplemented {
			t.Fatalf("err = %v", err)
		}
	})
	t.Run("bad arguments", func(t *testing.T) {
		e.peerDesk.mu.Lock()
		e.peerDesk.ready = nil
		e.peerDesk.mu.Unlock()
		for _, req := range []*connect.Request[pb.KeyRequest]{
			connect.NewRequest(&pb.KeyRequest{Target: target(dl.Id), Chord: "ctrl+nonsense"}),
		} {
			if _, err := dc.Key(ctx, req); connect.CodeOf(err) != connect.CodeInvalidArgument {
				t.Fatalf("bad chord: %v", err)
			}
		}
		_, err := dc.Click(ctx, connect.NewRequest(&pb.ClickRequest{Target: target(dl.Id), Button: "back"}))
		if connect.CodeOf(err) != connect.CodeInvalidArgument {
			t.Fatalf("bad button: %v", err)
		}
	})
}

func zstdDecode(b []byte) ([]byte, error) {
	d, err := zstd.NewReader(nil)
	if err != nil {
		return nil, err
	}
	defer d.Close()
	return d.DecodeAll(b, nil)
}
