package hub

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os/exec"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"
	"github.com/coder/websocket"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/session"
)

// newSessionHub serves a hub whose sessions live on a private tmux socket
// that is torn down with the test; the default tmux server is never touched.
func newSessionHub(t *testing.T) (*Hub, string, string) {
	t.Helper()
	if _, err := exec.LookPath("tmux"); err != nil {
		t.Skip("tmux not installed")
	}
	h, srv := newTestHub(t)
	sock := fmt.Sprintf("protectorate-test-%d", time.Now().UnixNano())
	h.Sessions = session.New(sock)
	h.SessionPoll = 20 * time.Millisecond
	h.PingInterval = 50 * time.Millisecond
	t.Cleanup(func() { exec.Command("tmux", "-L", sock, "kill-server").Run() })

	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { defer close(done); h.WatchSessions(ctx) }()
	t.Cleanup(func() { cancel(); <-done })
	return h, srv.URL, sock
}

func waitFor(t *testing.T, what string, cond func() bool) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for !cond() {
		if time.Now().After(deadline) {
			t.Fatalf("timed out waiting for %s", what)
		}
		time.Sleep(10 * time.Millisecond)
	}
}

func TestSessionRPC(t *testing.T) {
	_, url, _ := newSessionHub(t)
	c := protectoratev1connect.NewSessionServiceClient(&http.Client{Transport: bearer{testToken}}, url)
	ctx := context.Background()
	dir := t.TempDir()

	create := func(name string) error {
		_, err := c.CreateSession(ctx, connect.NewRequest(&pb.CreateSessionRequest{
			Name: name, Cwd: dir, Command: []string{"sh", "-c", "echo $GREETING; sleep 60"},
			Env: map[string]string{"GREETING": "hi-from-env"},
		}))
		return err
	}
	tests := []struct {
		name string
		op   func() error
		want connect.Code
	}{
		{"create ok", func() error { return create("one") }, 0},
		{"create duplicate", func() error { return create("one") }, connect.CodeAlreadyExists},
		{"create bad name", func() error { return create("a:b") }, connect.CodeInvalidArgument},
		{"create empty name", func() error { return create("") }, connect.CodeInvalidArgument},
		{"kill missing", func() error {
			_, err := c.KillSession(ctx, connect.NewRequest(&pb.KillSessionRequest{Name: "ghost"}))
			return err
		}, connect.CodeNotFound},
		{"capture missing", func() error {
			_, err := c.CapturePane(ctx, connect.NewRequest(&pb.CapturePaneRequest{Name: "ghost"}))
			return err
		}, connect.CodeNotFound},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.op()
			if tt.want == 0 {
				if err != nil {
					t.Fatal(err)
				}
				return
			}
			if connect.CodeOf(err) != tt.want {
				t.Fatalf("got %v, want code %v", err, tt.want)
			}
		})
	}

	res, err := c.ListSessions(ctx, connect.NewRequest(&pb.ListSessionsRequest{}))
	if err != nil {
		t.Fatal(err)
	}
	if len(res.Msg.Sessions) != 1 {
		t.Fatalf("want 1 session, got %v", res.Msg.Sessions)
	}
	s := res.Msg.Sessions[0]
	if s.Name != "one" || len(s.Command) != 3 || s.Command[0] != "sh" || s.PaneCols == 0 || s.CreatedAt == nil || s.Cwd == "" {
		t.Fatalf("unexpected session: %v", s)
	}

	waitFor(t, "env echoed in pane", func() bool {
		r, err := c.CapturePane(ctx, connect.NewRequest(&pb.CapturePaneRequest{Name: "one", Lines: 10}))
		return err == nil && strings.Contains(r.Msg.Text, "hi-from-env")
	})

	if _, err := c.KillSession(ctx, connect.NewRequest(&pb.KillSessionRequest{Name: "one"})); err != nil {
		t.Fatal(err)
	}
	res, _ = c.ListSessions(ctx, connect.NewRequest(&pb.ListSessionsRequest{}))
	if len(res.Msg.Sessions) != 0 {
		t.Fatalf("want no sessions, got %v", res.Msg.Sessions)
	}
}

func TestSessionEvents(t *testing.T) {
	h, url, _ := newSessionHub(t)
	hc := &http.Client{Transport: bearer{testToken}}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	stream, err := protectoratev1connect.NewEventServiceClient(hc, url).Subscribe(ctx, connect.NewRequest(&pb.SubscribeRequest{}))
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()
	events := make(chan *pb.Event, 64)
	go func() {
		for stream.Receive() {
			events <- stream.Msg()
		}
		close(events)
	}()
	expect := func(kind string) {
		t.Helper()
		timeout := time.After(5 * time.Second)
		for {
			select {
			case e, ok := <-events:
				if !ok {
					t.Fatalf("stream closed waiting for %s", kind)
				}
				if e.Kind == kind {
					if e.Subject != "ev" {
						t.Fatalf("subject = %q", e.Subject)
					}
					return
				}
			case <-timeout:
				t.Fatalf("timed out waiting for %s", kind)
			}
		}
	}

	if _, err := h.Sessions.Create(ctx, session.CreateOptions{Name: "ev", Command: []string{"sleep", "60"}}); err != nil {
		t.Fatal(err)
	}
	expect(EventSessionCreated)
	if err := h.Sessions.Kill(ctx, "ev"); err != nil {
		t.Fatal(err)
	}
	expect(EventSessionExited)
}

func wsURL(base, name, query string) string {
	return "ws" + strings.TrimPrefix(base, "http") + "/ws/terminal/" + name + query
}

func dial(t *testing.T, url string, o *websocket.DialOptions) *websocket.Conn {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	conn, _, err := websocket.Dial(ctx, url, o)
	if err != nil {
		t.Fatal(err)
	}
	conn.SetReadLimit(1 << 20)
	return conn
}

func sendData(t *testing.T, c *websocket.Conn, s string) {
	t.Helper()
	if err := c.Write(context.Background(), websocket.MessageBinary, append([]byte{FrameData}, s...)); err != nil {
		t.Fatal(err)
	}
}

func sendResize(t *testing.T, c *websocket.Conn, cols, rows int) {
	t.Helper()
	b, _ := json.Marshal(ResizeMessage{Cols: uint16(cols), Rows: uint16(rows)})
	if err := c.Write(context.Background(), websocket.MessageBinary, append([]byte{FrameResize}, b...)); err != nil {
		t.Fatal(err)
	}
}

// readUntil accumulates output frames until want appears.
func readUntil(t *testing.T, c *websocket.Conn, want string) string {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	var acc bytes.Buffer
	for !strings.Contains(acc.String(), want) {
		typ, msg, err := c.Read(ctx)
		if err != nil {
			t.Fatalf("waiting for %q, got %q: %v", want, acc.String(), err)
		}
		if typ != websocket.MessageBinary || len(msg) == 0 || msg[0] != FrameData {
			t.Fatalf("bad frame %v % x", typ, msg)
		}
		acc.Write(msg[1:])
	}
	return acc.String()
}

func TestTerminalAuthAndErrors(t *testing.T) {
	h, url, _ := newSessionHub(t)
	if _, err := h.Sessions.Create(context.Background(), session.CreateOptions{Name: "auth", Command: []string{"cat"}}); err != nil {
		t.Fatal(err)
	}
	hdr := func(k, v string) http.Header { return http.Header{k: {v}} }
	tests := []struct {
		name    string
		session string
		query   string
		opts    *websocket.DialOptions
		want    int
	}{
		{"no token", "auth", "", &websocket.DialOptions{}, 401},
		{"wrong header", "auth", "", &websocket.DialOptions{HTTPHeader: hdr("Authorization", "Bearer nope")}, 401},
		{"wrong subprotocol", "auth", "", &websocket.DialOptions{Subprotocols: []string{TerminalProtocol, "token.nope"}}, 401},
		{"wrong query", "auth", "?token=nope", &websocket.DialOptions{}, 401},
		{"missing session", "ghost", "", &websocket.DialOptions{HTTPHeader: hdr("Authorization", "Bearer "+testToken)}, 404},
		{"invalid name", "a.b", "", &websocket.DialOptions{HTTPHeader: hdr("Authorization", "Bearer "+testToken)}, 400},
		{"header ok", "auth", "", &websocket.DialOptions{HTTPHeader: hdr("Authorization", "Bearer "+testToken)}, 101},
		{"subprotocol ok", "auth", "", &websocket.DialOptions{Subprotocols: []string{TerminalProtocol, "token." + testToken}}, 101},
		{"query ok", "auth", "?token=" + testToken, &websocket.DialOptions{}, 101},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
			defer cancel()
			conn, res, err := websocket.Dial(ctx, wsURL(url, tt.session, tt.query), tt.opts)
			if tt.want == 101 {
				if err != nil {
					t.Fatal(err)
				}
				if tt.opts.Subprotocols != nil && conn.Subprotocol() != TerminalProtocol {
					t.Fatalf("subprotocol = %q", conn.Subprotocol())
				}
				conn.Close(websocket.StatusNormalClosure, "")
				return
			}
			if err == nil {
				t.Fatal("expected dial failure")
			}
			if res == nil || res.StatusCode != tt.want {
				t.Fatalf("got %v, want status %d", res, tt.want)
			}
		})
	}
}

// sizeRows allows for the tmux status line taking one row.
func sizeRows(got, want int) bool { return got == want || got == want-1 }

func attachProcesses(sock string) int {
	out, _ := exec.Command("ps", "-eo", "args").Output()
	n := 0
	for _, l := range strings.Split(string(out), "\n") {
		if strings.Contains(l, "-L "+sock+" attach-session") {
			n++
		}
	}
	return n
}

func TestTerminalAttachLifecycle(t *testing.T) {
	h, url, sock := newSessionHub(t)
	ctx := context.Background()
	if _, err := h.Sessions.Create(ctx, session.CreateOptions{Name: "term", Command: []string{"cat"}}); err != nil {
		t.Fatal(err)
	}
	auth := &websocket.DialOptions{HTTPHeader: http.Header{"Authorization": {"Bearer " + testToken}}}

	a := dial(t, wsURL(url, "term", "?cols=100&rows=30"), auth)
	sendData(t, a, "hello-one\r")
	readUntil(t, a, "hello-one")

	waitFor(t, "initial size", func() bool {
		s, err := h.Sessions.Get(ctx, "term")
		return err == nil && s.Cols == 100 && sizeRows(s.Rows, 30)
	})
	sendResize(t, a, 120, 40)
	waitFor(t, "resize", func() bool {
		s, err := h.Sessions.Get(ctx, "term")
		return err == nil && s.Cols == 120 && sizeRows(s.Rows, 40)
	})

	b := dial(t, wsURL(url, "term", "?cols=120&rows=40"), auth)
	waitFor(t, "two clients", func() bool {
		s, _ := h.Sessions.Get(ctx, "term")
		return s.AttachedClients == 2
	})
	sendData(t, b, "from-b\r")
	readUntil(t, a, "from-b")

	ro := dial(t, wsURL(url, "term", "?readonly=1"), auth)
	sendData(t, ro, "blocked-input\r")
	sendData(t, a, "marker\r")
	readUntil(t, ro, "marker")
	text, err := h.Sessions.Capture(ctx, "term", 50)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(text, "blocked-input") {
		t.Fatalf("read-only client injected input:\n%s", text)
	}

	for _, c := range []*websocket.Conn{a, b, ro} {
		c.Close(websocket.StatusNormalClosure, "")
	}
	waitFor(t, "clients detached", func() bool {
		s, err := h.Sessions.Get(ctx, "term")
		return err == nil && s.AttachedClients == 0
	})
	waitFor(t, "no stray tmux attach", func() bool { return attachProcesses(sock) == 0 })
	if _, err := h.Sessions.Get(ctx, "term"); err != nil {
		t.Fatalf("session must survive disconnect: %v", err)
	}
}

func TestTerminalSessionExit(t *testing.T) {
	h, url, sock := newSessionHub(t)
	ctx := context.Background()
	if _, err := h.Sessions.Create(ctx, session.CreateOptions{Name: "short", Command: []string{"sh", "-c", "read x; echo bye"}}); err != nil {
		t.Fatal(err)
	}
	c := dial(t, wsURL(url, "short", ""), &websocket.DialOptions{HTTPHeader: http.Header{"Authorization": {"Bearer " + testToken}}})
	sendData(t, c, "go\r")
	readUntil(t, c, "bye")
	rctx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	for {
		if _, _, err := c.Read(rctx); err != nil {
			if websocket.CloseStatus(err) != websocket.StatusNormalClosure {
				t.Fatalf("want normal close, got %v", err)
			}
			break
		}
	}
	waitFor(t, "no stray tmux attach", func() bool { return attachProcesses(sock) == 0 })
}
