package hub

import (
	"context"
	"encoding/json"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"os"
	"runtime"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

const testToken = "secret"

type bearer struct{ tok string }

func (b bearer) RoundTrip(r *http.Request) (*http.Response, error) {
	r = r.Clone(r.Context())
	if b.tok != "" {
		r.Header.Set("Authorization", "Bearer "+b.tok)
	}
	return http.DefaultTransport.RoundTrip(r)
}

func newTestHub(t *testing.T) (*Hub, *httptest.Server) {
	t.Helper()
	h := New("test", testToken)
	h.Heartbeat = 20 * time.Millisecond
	srv := httptest.NewServer(h.Handler())
	t.Cleanup(srv.Close)
	return h, srv
}

func TestGetStatusGeneratedClient(t *testing.T) {
	_, srv := newTestHub(t)
	c := protectoratev1connect.NewHubServiceClient(&http.Client{Transport: bearer{testToken}}, srv.URL)
	res, err := c.GetStatus(context.Background(), connect.NewRequest(&pb.GetStatusRequest{}))
	if err != nil {
		t.Fatal(err)
	}
	m := res.Msg
	if m.Version != "test" || m.Os != runtime.GOOS || m.Arch != runtime.GOARCH || m.Hostname == "" || m.StartedAt == nil {
		t.Fatalf("unexpected status: %v", m)
	}
}

func TestRawJSON(t *testing.T) {
	_, srv := newTestHub(t)
	tests := []struct {
		name       string
		auth       string
		wantStatus int
		wantKey    string
	}{
		{"valid token", "Bearer " + testToken, http.StatusOK, "version"},
		{"no token", "", http.StatusUnauthorized, "code"},
		{"wrong token", "Bearer nope", http.StatusUnauthorized, "code"},
		{"wrong scheme", "Basic " + testToken, http.StatusUnauthorized, "code"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			req, _ := http.NewRequest(http.MethodPost, srv.URL+"/protectorate.v1.HubService/GetStatus", strings.NewReader("{}"))
			req.Header.Set("Content-Type", "application/json")
			if tt.auth != "" {
				req.Header.Set("Authorization", tt.auth)
			}
			res, err := http.DefaultClient.Do(req)
			if err != nil {
				t.Fatal(err)
			}
			defer res.Body.Close()
			if res.StatusCode != tt.wantStatus {
				t.Fatalf("status %d, want %d", res.StatusCode, tt.wantStatus)
			}
			var body map[string]any
			if err := json.NewDecoder(res.Body).Decode(&body); err != nil {
				t.Fatal(err)
			}
			if _, ok := body[tt.wantKey]; !ok {
				t.Fatalf("body %v missing %q", body, tt.wantKey)
			}
		})
	}
}

func TestAuthRejectedTyped(t *testing.T) {
	_, srv := newTestHub(t)
	tests := []struct {
		name string
		call func(http.Client) error
	}{
		{"unary", func(hc http.Client) error {
			_, err := protectoratev1connect.NewHubServiceClient(&hc, srv.URL).
				GetStatus(context.Background(), connect.NewRequest(&pb.GetStatusRequest{}))
			return err
		}},
		{"stream", func(hc http.Client) error {
			s, err := protectoratev1connect.NewEventServiceClient(&hc, srv.URL).
				Subscribe(context.Background(), connect.NewRequest(&pb.SubscribeRequest{}))
			if err != nil {
				return err
			}
			defer s.Close()
			s.Receive()
			return s.Err()
		}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.call(http.Client{Transport: bearer{"wrong"}})
			if connect.CodeOf(err) != connect.CodeUnauthenticated {
				t.Fatalf("got %v, want unauthenticated", err)
			}
		})
	}
}

func TestSubscribeHeartbeats(t *testing.T) {
	_, srv := newTestHub(t)
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	s, err := protectoratev1connect.NewEventServiceClient(&http.Client{Transport: bearer{testToken}}, srv.URL).
		Subscribe(ctx, connect.NewRequest(&pb.SubscribeRequest{}))
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	var ids []string
	for len(ids) < 3 && s.Receive() {
		e := s.Msg()
		if e.Kind != "heartbeat" || e.Subject != "hub" || e.Time == nil || e.Payload == nil {
			t.Fatalf("unexpected event: %v", e)
		}
		ids = append(ids, e.Id)
	}
	if len(ids) != 3 || ids[0] == ids[1] {
		t.Fatalf("want 3 distinct heartbeats, got %v (err %v)", ids, s.Err())
	}
}

func TestServeHTTP2Cleartext(t *testing.T) {
	h := New("test", testToken)
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- h.Serve(ctx, l) }()

	tr := &http.Transport{Protocols: new(http.Protocols)}
	tr.Protocols.SetUnencryptedHTTP2(true)
	req, _ := http.NewRequest(http.MethodPost, "http://"+l.Addr().String()+"/protectorate.v1.HubService/GetStatus", strings.NewReader("{}"))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+testToken)
	res, err := (&http.Client{Transport: tr}).Do(req)
	if err != nil {
		t.Fatal(err)
	}
	io.Copy(io.Discard, res.Body)
	res.Body.Close()
	if res.ProtoMajor != 2 || res.StatusCode != http.StatusOK {
		t.Fatalf("got proto %d status %d", res.ProtoMajor, res.StatusCode)
	}

	cancel()
	if err := <-done; err != nil {
		t.Fatalf("serve: %v", err)
	}
}

func TestToken(t *testing.T) {
	dir := t.TempDir() + "/home"
	t.Setenv("PROTECTORATE_HOME", dir)
	if _, err := ReadToken(); err == nil {
		t.Fatal("ReadToken succeeded before token exists")
	}
	a, err := EnsureToken()
	if err != nil || len(a) != 64 {
		t.Fatalf("EnsureToken: %q, %v", a, err)
	}
	b, err := EnsureToken()
	if err != nil || a != b {
		t.Fatalf("second EnsureToken = %q, %v; want %q", b, err, a)
	}
	if r, err := ReadToken(); err != nil || r != a {
		t.Fatalf("ReadToken = %q, %v", r, err)
	}
	if runtime.GOOS != "windows" {
		for path, want := range map[string]os.FileMode{dir: 0o700, dir + "/token": 0o600} {
			fi, err := os.Stat(path)
			if err != nil || fi.Mode().Perm() != want {
				t.Fatalf("%s mode %v err %v, want %v", path, fi.Mode().Perm(), err, want)
			}
		}
	}
}
