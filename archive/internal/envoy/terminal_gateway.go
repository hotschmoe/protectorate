package envoy

import (
	"context"
	"encoding/json"
	"io"
	"log"
	"net/http"
	"sync"
	"time"

	"github.com/gorilla/websocket"
)


const (
	msgData   = 0x30 // '0' - input/output data
	msgResize = 0x31 // '1' - resize command

	wsWriteWait  = 10 * time.Second
	wsPingPeriod = 30 * time.Second
	wsPongWait   = 60 * time.Second
)

var terminalUpgrader = websocket.Upgrader{
	ReadBufferSize:  4096,
	WriteBufferSize: 4096,
	CheckOrigin: func(r *http.Request) bool {
		return true
	},
}

// TerminalGateway bridges a WebSocket connection to a Docker exec session
type TerminalGateway struct {
	docker       DockerExecClient
	container    string
	socketPath   string
	readOnly     bool
	wsConn       *websocket.Conn
	execSession  *ExecSession
	ctx          context.Context
	cancel       context.CancelFunc
	mu           sync.Mutex
	initialCols  uint
	initialRows  uint
}

// TerminalInitMessage is sent by the client on connection
type TerminalInitMessage struct {
	Cols uint `json:"cols"`
	Rows uint `json:"rows"`
}

// TerminalResizeMessage is sent by the client to resize the terminal
type TerminalResizeMessage struct {
	Columns uint `json:"columns"`
	Rows    uint `json:"rows"`
}

// NewTerminalGateway creates a new terminal gateway
func NewTerminalGateway(docker DockerExecClient, container, socketPath string, readOnly bool) *TerminalGateway {
	return &TerminalGateway{
		docker:     docker,
		container:  container,
		socketPath: socketPath,
		readOnly:   readOnly,
	}
}

// Start upgrades the HTTP connection and starts the terminal session
func (g *TerminalGateway) Start(w http.ResponseWriter, r *http.Request) {
	wsConn, err := terminalUpgrader.Upgrade(w, r, nil)
	if err != nil {
		log.Printf("terminal gateway: websocket upgrade failed: %v", err)
		return
	}
	g.wsConn = wsConn
	defer g.wsConn.Close()

	g.ctx, g.cancel = context.WithCancel(r.Context())
	defer g.cancel()

	g.wsConn.SetReadDeadline(time.Now().Add(wsPongWait))
	g.wsConn.SetPongHandler(func(string) error {
		g.wsConn.SetReadDeadline(time.Now().Add(wsPongWait))
		return nil
	})

	if err := g.waitForInit(); err != nil {
		log.Printf("terminal gateway: init failed: %v", err)
		return
	}

	if err := g.startExecSession(); err != nil {
		log.Printf("terminal gateway: exec failed: %v", err)
		return
	}
	defer g.execSession.Conn.Close()

	errCh := make(chan error, 2)

	go g.relayExecToWS(errCh)
	go g.relayWSToExec(errCh)
	go g.keepalive()

	<-errCh
	log.Printf("terminal gateway: session ended for %s", g.container)
}

func (g *TerminalGateway) waitForInit() error {
	_, msg, err := g.wsConn.ReadMessage()
	if err != nil {
		return err
	}

	var init TerminalInitMessage
	if err := json.Unmarshal(msg, &init); err != nil {
		g.initialCols = 80
		g.initialRows = 24
	} else {
		g.initialCols = init.Cols
		g.initialRows = init.Rows
		if g.initialCols == 0 {
			g.initialCols = 80
		}
		if g.initialRows == 0 {
			g.initialRows = 24
		}
	}

	return nil
}

func (g *TerminalGateway) startExecSession() error {
	// dtach -a: attach to existing session, -e: detach key (Ctrl+\), -z: disable suspend
	cmd := []string{"dtach", "-a", g.socketPath, "-e", `^\`, "-z"}

	session, err := g.docker.ExecAttach(g.ctx, ExecAttachOptions{
		Container: g.container,
		Cmd:       cmd,
		User:      "agent",
		Env: []string{
			"TERM=xterm-256color",
			"PATH=/home/agent/.local/bin:/usr/local/bin:/usr/bin:/bin",
		},
		Cols: g.initialCols,
		Rows: g.initialRows,
	})
	if err != nil {
		return err
	}

	g.execSession = session
	return nil
}

func (g *TerminalGateway) relayExecToWS(errCh chan<- error) {
	buf := make([]byte, 4096)
	for {
		n, err := g.execSession.Conn.Reader.Read(buf)
		if err != nil {
			if err != io.EOF {
				errCh <- err
			} else {
				errCh <- nil
			}
			return
		}

		if n > 0 {
			msg := make([]byte, n+1)
			msg[0] = msgData
			copy(msg[1:], buf[:n])

			g.mu.Lock()
			g.wsConn.SetWriteDeadline(time.Now().Add(wsWriteWait))
			err := g.wsConn.WriteMessage(websocket.BinaryMessage, msg)
			g.mu.Unlock()

			if err != nil {
				errCh <- err
				return
			}
		}
	}
}

func (g *TerminalGateway) relayWSToExec(errCh chan<- error) {
	for {
		_, msg, err := g.wsConn.ReadMessage()
		if err != nil {
			errCh <- err
			return
		}

		if len(msg) == 0 {
			continue
		}

		msgType := msg[0]
		payload := msg[1:]

		switch msgType {
		case msgData:
			if g.readOnly {
				continue
			}
			if len(payload) > 0 {
				_, err := g.execSession.Conn.Conn.Write(payload)
				if err != nil {
					errCh <- err
					return
				}
			}

		case msgResize:
			var resize TerminalResizeMessage
			if err := json.Unmarshal(payload, &resize); err != nil {
				continue
			}
			if resize.Columns > 0 && resize.Rows > 0 {
				_ = g.docker.ExecResize(g.ctx, g.execSession.ID, resize.Columns, resize.Rows)
			}
		}
	}
}

func (g *TerminalGateway) keepalive() {
	ticker := time.NewTicker(wsPingPeriod)
	defer ticker.Stop()

	for {
		select {
		case <-ticker.C:
			g.mu.Lock()
			g.wsConn.SetWriteDeadline(time.Now().Add(wsWriteWait))
			err := g.wsConn.WriteMessage(websocket.PingMessage, nil)
			g.mu.Unlock()

			if err != nil {
				return
			}
		case <-g.ctx.Done():
			return
		}
	}
}
