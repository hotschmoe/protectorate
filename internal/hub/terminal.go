package hub

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"os"
	"os/exec"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/coder/websocket"
	"github.com/creack/pty"

	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/session"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

// Terminal wire protocol. Every WebSocket message is binary: one type byte
// followed by the payload. Server to client: FrameData carries raw PTY output.
// Client to server: FrameData is keyboard input, FrameResize is JSON
// {"cols":N,"rows":N}. Unknown frame types are ignored.
//
// Attaching to an exited session shows its final screen read-only: input
// frames are dropped, and the server sends one FrameNotice (server to client,
// UTF-8 text) right after the upgrade saying so. If the session's command ends
// while a client is attached, the same notice is sent then and the connection
// stays open on the final screen.
const (
	FrameData   byte = '0'
	FrameResize byte = '1'
	FrameNotice byte = '2'

	// TerminalProtocol is the WebSocket subprotocol clients offer and the
	// server selects.
	TerminalProtocol = "protectorate.v1"
	// TokenProtocolPrefix prefixes the hub token in a second offered
	// subprotocol, for browser clients that cannot set headers.
	TokenProtocolPrefix = "token."

	maxFrame   = 64 << 10
	teardownTO = 3 * time.Second
)

type ResizeMessage struct {
	Cols uint16 `json:"cols"`
	Rows uint16 `json:"rows"`
}

// terminalCredentials lists the offered tokens in order: Authorization bearer
// header, a "token.<value>" entry in Sec-WebSocket-Protocol (browsers and
// Flutter web cannot set headers on a WebSocket but can set subprotocols, and
// it stays out of URLs and access logs), and a ?token= query parameter as a
// last resort for clients that can do neither.
func terminalCredentials(r *http.Request) []string {
	var out []string
	if t := bearerToken(r.Header); t != "" {
		out = append(out, t)
	}
	for _, line := range r.Header.Values("Sec-WebSocket-Protocol") {
		for _, p := range strings.Split(line, ",") {
			if got, ok := strings.CutPrefix(strings.TrimSpace(p), TokenProtocolPrefix); ok && got != "" {
				out = append(out, got)
			}
		}
	}
	if got := r.URL.Query().Get("token"); got != "" {
		out = append(out, got)
	}
	return out
}

// authorizeTerminal authenticates the first valid credential and checks it
// for the terminal scope.
func (h *Hub) authorizeTerminal(r *http.Request, name string, readOnly bool) (*auth.Principal, error) {
	var p *auth.Principal
	err := auth.ErrUnauthenticated
	for _, c := range terminalCredentials(r) {
		if p, err = h.authz.Authenticate(c); err == nil {
			break
		}
	}
	if err == nil {
		err = h.authz.AuthorizeTerminal(p, readOnly, !sleeve.IsSleeveSession(name))
	}
	return p, err
}

func queryUint16(r *http.Request, key string, def uint16) uint16 {
	n, err := strconv.ParseUint(r.URL.Query().Get(key), 10, 16)
	if err != nil || n == 0 {
		return def
	}
	return uint16(n)
}

func (h *Hub) handleTerminal(w http.ResponseWriter, r *http.Request) {
	name := r.PathValue("name")
	readOnly := r.URL.Query().Get("readonly") == "1"
	procedure := "terminal.attach"
	if readOnly {
		procedure = "terminal.attach.readonly"
	}
	p, err := h.authorizeTerminal(r, name, readOnly)
	if err != nil {
		h.authz.Record(r.Context(), p, procedure, name, err, deniedCode(err))
		if errors.Is(err, auth.ErrUnauthenticated) {
			http.Error(w, "missing or invalid token", http.StatusUnauthorized)
		} else {
			http.Error(w, err.Error(), http.StatusForbidden)
		}
		return
	}
	cmd, sess, err := h.Sessions.AttachCmd(r.Context(), name, readOnly)
	if err != nil {
		h.authz.Record(r.Context(), p, procedure, name, err, "attach_failed")
	}
	switch {
	case errors.Is(err, session.ErrInvalidName):
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	case errors.Is(err, session.ErrNotFound):
		http.Error(w, err.Error(), http.StatusNotFound)
		return
	case err != nil:
		http.Error(w, "attach failed", http.StatusInternalServerError)
		return
	}

	// Auth is an explicit token, not an ambient cookie, so cross-origin
	// browser pages gain nothing from the origin check.
	h.authz.Record(r.Context(), p, procedure, name, nil, "")
	conn, err := websocket.Accept(w, r, &websocket.AcceptOptions{
		Subprotocols:       []string{TerminalProtocol},
		InsecureSkipVerify: true,
	})
	if err != nil {
		return
	}
	conn.SetReadLimit(maxFrame)
	if sess.State == session.Exited {
		readOnly = true
		if sendNotice(r.Context(), conn, sess) != nil {
			conn.CloseNow()
			return
		}
	}

	ctx, cancel := context.WithCancel(r.Context())
	defer cancel()

	size := &pty.Winsize{Cols: queryUint16(r, "cols", 80), Rows: queryUint16(r, "rows", 24)}
	ptmx, err := pty.StartWithSize(cmd, size)
	if err != nil {
		conn.Close(websocket.StatusInternalError, "attach failed")
		return
	}

	var wg sync.WaitGroup
	if sess.State != session.Exited {
		wg.Go(func() { h.noticeOnExit(ctx, conn, name) })
	}
	wg.Go(func() { defer cancel(); pumpOutput(ctx, conn, ptmx) })
	wg.Go(func() { pingLoop(ctx, conn, h.PingInterval, cancel) })

	pumpInput(ctx, conn, ptmx, readOnly)
	cancel()

	teardown(cmd, ptmx)
	conn.Close(websocket.StatusNormalClosure, "detached")
	wg.Wait()
}

func exitNotice(s session.Session) string {
	how := "exited with code " + strconv.Itoa(s.ExitCode)
	if s.ExitSignal > 0 {
		how = "killed by signal " + strconv.Itoa(s.ExitSignal)
	}
	return "session " + s.Name + " " + how + "; showing its final screen read-only (respawn it to run again)"
}

func sendNotice(ctx context.Context, conn *websocket.Conn, s session.Session) error {
	return conn.Write(ctx, websocket.MessageBinary, append([]byte{FrameNotice}, exitNotice(s)...))
}

func (h *Hub) noticeOnExit(ctx context.Context, conn *websocket.Conn, name string) {
	t := time.NewTicker(h.SessionPoll)
	defer t.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-t.C:
		}
		s, err := h.Sessions.Get(ctx, name)
		if err != nil {
			return
		}
		if s.State == session.Exited {
			_ = sendNotice(ctx, conn, s)
			return
		}
	}
}

func pumpOutput(ctx context.Context, conn *websocket.Conn, ptmx *os.File) {
	buf := make([]byte, 32<<10)
	buf[0] = FrameData
	for {
		n, err := ptmx.Read(buf[1:])
		if n > 0 {
			if werr := conn.Write(ctx, websocket.MessageBinary, buf[:n+1]); werr != nil {
				return
			}
		}
		if err != nil {
			conn.Close(websocket.StatusNormalClosure, "session ended")
			return
		}
	}
}

func pumpInput(ctx context.Context, conn *websocket.Conn, ptmx *os.File, readOnly bool) {
	for {
		typ, msg, err := conn.Read(ctx)
		if err != nil {
			return
		}
		if typ != websocket.MessageBinary || len(msg) == 0 {
			continue
		}
		switch msg[0] {
		case FrameData:
			if readOnly || len(msg) == 1 {
				continue
			}
			if _, err := ptmx.Write(msg[1:]); err != nil {
				return
			}
		case FrameResize:
			var rs ResizeMessage
			if json.Unmarshal(msg[1:], &rs) != nil || rs.Cols == 0 || rs.Rows == 0 {
				continue
			}
			_ = pty.Setsize(ptmx, &pty.Winsize{Cols: rs.Cols, Rows: rs.Rows})
		}
	}
}

func pingLoop(ctx context.Context, conn *websocket.Conn, every time.Duration, cancel context.CancelFunc) {
	t := time.NewTicker(every)
	defer t.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-t.C:
		}
		pctx, pcancel := context.WithTimeout(ctx, every)
		err := conn.Ping(pctx)
		pcancel()
		if err != nil {
			cancel()
			return
		}
	}
}

// teardown detaches the tmux client: closing the PTY master hangs it up, and
// tmux treats that as a detach, leaving the session running.
func teardown(cmd *exec.Cmd, ptmx *os.File) {
	_ = ptmx.Close()
	done := make(chan struct{})
	go func() { _ = cmd.Wait(); close(done) }()
	select {
	case <-done:
	case <-time.After(teardownTO):
		_ = cmd.Process.Kill()
		<-done
	}
}
