package main

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strings"
	"sync/atomic"

	"connectrpc.com/connect"
	"github.com/coder/websocket"
	"golang.org/x/term"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/hub"
)

func sessionClient(o *opts) (protectoratev1connect.SessionServiceClient, error) {
	hc, err := o.httpClient()
	if err != nil {
		return nil, err
	}
	return protectoratev1connect.NewSessionServiceClient(hc, o.hubURL), nil
}

func sessionCmd(o *opts) *command {
	c := &command{Use: "session", Aliases: []string{"sessions"}, Short: "Manage tmux-backed harness sessions"}
	c.AddCommand(sessionLsCmd(o), sessionNewCmd(o), sessionKillCmd(o), sessionRespawnCmd(o), sessionAttachCmd(o), sessionCaptureCmd(o))
	return c
}

func sessionLsCmd(o *opts) *command {
	return &command{
		Use:   "ls",
		Short: "List sessions",
		Args:  noArgs,
		RunE: func(cmd *command, _ []string) error {
			c, err := sessionClient(o)
			if err != nil {
				return err
			}
			res, err := c.ListSessions(cmd.Context(), connect.NewRequest(&pb.ListSessionsRequest{}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintln(w, "NAME\tSTATE\tCLIENTS\tSIZE\tACTIVE\tCWD\tCOMMAND")
				for _, s := range res.Msg.Sessions {
					fmt.Fprintf(w, "%s\t%s\t%d\t%dx%d\t%s\t%s\t%s\n", s.Name, sessionStateLabel(s), s.AttachedClients, s.PaneCols, s.PaneRows,
						s.LastActivity.AsTime().Local().Format("15:04:05"), s.Cwd, strings.Join(s.Command, " "))
				}
			})
		},
	}
}

// sessionStateLabel is "running", or "exited(3)" / "exited(signal 9)".
func sessionStateLabel(s *pb.Session) string {
	if s.State != pb.SessionState_SESSION_STATE_EXITED {
		return "running"
	}
	if s.ExitSignal > 0 {
		return fmt.Sprintf("exited(signal %d)", s.ExitSignal)
	}
	return fmt.Sprintf("exited(%d)", s.ExitCode)
}

func sessionNewCmd(o *opts) *command {
	var cwd string
	var env []string
	c := &command{
		Use:   "new <name> [--cwd dir] [--env K=V] [-- cmd args...]",
		Short: "Start a session running cmd (default: the login shell)",
		Args:  minimumArgs(1),
		RunE: func(cmd *command, args []string) error {
			name, argv := args[0], args[1:]
			if dash := cmd.ArgsLenAtDash(); dash >= 0 && dash != 1 {
				return errors.New("usage: session new <name> [flags] -- <cmd...>")
			} else if dash < 0 && len(args) > 1 {
				return errors.New("separate the command from the name with --")
			}
			envMap := map[string]string{}
			for _, kv := range env {
				k, v, ok := strings.Cut(kv, "=")
				if !ok || k == "" {
					return fmt.Errorf("invalid --env %q, want K=V", kv)
				}
				envMap[k] = v
			}
			c, err := sessionClient(o)
			if err != nil {
				return err
			}
			res, err := c.CreateSession(cmd.Context(), connect.NewRequest(&pb.CreateSessionRequest{
				Name: name, Cwd: cwd, Command: argv, Env: envMap,
			}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintf(w, "created\t%s\n", res.Msg.Session.Name)
			})
		},
	}
	c.Flags().StringVar(&cwd, "cwd", "", "working directory on the hub machine")
	c.Flags().StringArrayVar(&env, "env", nil, "environment variable K=V (repeatable)")
	return c
}

func sessionKillCmd(o *opts) *command {
	return &command{
		Use:   "kill <name>",
		Short: "Kill a session and its processes",
		Args:  exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			c, err := sessionClient(o)
			if err != nil {
				return err
			}
			res, err := c.KillSession(cmd.Context(), connect.NewRequest(&pb.KillSessionRequest{Name: args[0]}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprintf(w, "killed\t%s\n", args[0]) })
		},
	}
}

func sessionRespawnCmd(o *opts) *command {
	return &command{
		Use:   "respawn <name>",
		Short: "Restart an exited session with its original command",
		Args:  exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			c, err := sessionClient(o)
			if err != nil {
				return err
			}
			res, err := c.RespawnSession(cmd.Context(), connect.NewRequest(&pb.RespawnSessionRequest{Name: args[0]}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprintf(w, "respawned\t%s\n", args[0]) })
		},
	}
}

func sessionCaptureCmd(o *opts) *command {
	var lines int
	c := &command{
		Use:   "capture <name>",
		Short: "Print the pane contents plus scrollback without attaching",
		Args:  exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			c, err := sessionClient(o)
			if err != nil {
				return err
			}
			res, err := c.CapturePane(cmd.Context(), connect.NewRequest(&pb.CapturePaneRequest{Name: args[0], Lines: int32(lines)}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprint(w, res.Msg.Text) })
		},
	}
	c.Flags().IntVar(&lines, "lines", 200, "scrollback lines above the visible screen")
	return c
}

func sessionAttachCmd(o *opts) *command {
	var readOnly bool
	var escape string
	c := &command{
		Use:   "attach <name>",
		Short: "Attach this terminal to a session through the hub",
		Long: `Attach this terminal to a session through the hub's WebSocket terminal.
Works against a remote hub. Detach with the escape key (default Ctrl-]),
which leaves the session running. Use --escape none to disable it.`,
		Args: exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			esc, err := parseEscape(escape)
			if err != nil {
				return err
			}
			return attach(cmd.Context(), o, args[0], readOnly, esc)
		},
	}
	c.Flags().BoolVar(&readOnly, "readonly", false, "attach without being able to send input")
	c.Flags().StringVar(&escape, "escape", "^]", `detach key as ^X, or "none"`)
	return c
}

// parseEscape returns the detach byte, or -1 when disabled.
func parseEscape(s string) (int, error) {
	if s == "none" || s == "" {
		return -1, nil
	}
	if len(s) == 2 && s[0] == '^' {
		c := s[1] &^ 0x20
		if c >= '@' && c <= '_' {
			return int(c - '@'), nil
		}
	}
	return 0, fmt.Errorf("invalid --escape %q, want ^A..^Z, ^[ ^\\ ^] ^^ ^_ or none", s)
}

func terminalURL(hubURL, name string, readOnly bool, cols, rows int) (string, error) {
	u, err := url.Parse(hubURL)
	if err != nil {
		return "", fmt.Errorf("parse hub url: %w", err)
	}
	switch u.Scheme {
	case "http":
		u.Scheme = "ws"
	case "https":
		u.Scheme = "wss"
	default:
		return "", fmt.Errorf("unsupported hub url scheme %q", u.Scheme)
	}
	u.Path = strings.TrimRight(u.Path, "/") + "/ws/terminal/" + url.PathEscape(name)
	q := url.Values{}
	if readOnly {
		q.Set("readonly", "1")
	}
	if cols > 0 && rows > 0 {
		q.Set("cols", fmt.Sprint(cols))
		q.Set("rows", fmt.Sprint(rows))
	}
	u.RawQuery = q.Encode()
	return u.String(), nil
}

// terminalReset undoes modes tmux may have left enabled on the local terminal.
const terminalReset = "\x1b[?1049l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[?2004l\x1b[?25h\x1b[0m"

func attach(ctx context.Context, o *opts, name string, readOnly bool, esc int) error {
	in, out := int(os.Stdin.Fd()), int(os.Stdout.Fd())
	if !term.IsTerminal(in) || !term.IsTerminal(out) {
		return errors.New("attach needs an interactive terminal")
	}
	tok, err := o.token()
	if err != nil {
		return err
	}
	cols, rows, err := term.GetSize(out)
	if err != nil {
		return fmt.Errorf("terminal size: %w", err)
	}
	u, err := terminalURL(o.hubURL, name, readOnly, cols, rows)
	if err != nil {
		return err
	}
	conn, res, err := websocket.Dial(ctx, u, &websocket.DialOptions{
		HTTPHeader:   http.Header{"Authorization": {"Bearer " + tok}},
		Subprotocols: []string{hub.TerminalProtocol},
	})
	if err != nil {
		if res != nil {
			return fmt.Errorf("attach %q: %s", name, res.Status)
		}
		return fmt.Errorf("attach %q: %w", name, err)
	}
	conn.SetReadLimit(1 << 20)

	state, err := term.MakeRaw(in)
	if err != nil {
		conn.CloseNow()
		return fmt.Errorf("raw mode: %w", err)
	}
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	var detached atomic.Bool
	var notice string
	defer func() {
		_ = term.Restore(in, state)
		os.Stdout.WriteString(terminalReset)
		if notice != "" {
			fmt.Fprintf(os.Stderr, "[%s]\n", notice)
		}
		if detached.Load() {
			fmt.Fprintf(os.Stderr, "[detached from %s]\n", name)
		} else {
			fmt.Fprintf(os.Stderr, "[connection to %s closed]\n", name)
		}
	}()

	go forwardInput(ctx, conn, os.Stdin, esc, readOnly, func() { detached.Store(true); cancel() })
	go forwardResize(ctx, conn, out)

	for {
		typ, msg, err := conn.Read(ctx)
		if err != nil {
			conn.CloseNow()
			if ctx.Err() != nil {
				return nil
			}
			if websocket.CloseStatus(err) == websocket.StatusNormalClosure {
				return nil
			}
			return err
		}
		if typ != websocket.MessageBinary || len(msg) < 2 {
			continue
		}
		switch msg[0] {
		case hub.FrameData:
			if _, err := os.Stdout.Write(msg[1:]); err != nil {
				conn.CloseNow()
				return err
			}
		case hub.FrameNotice:
			notice = string(msg[1:])
		}
	}
}

func forwardInput(ctx context.Context, conn *websocket.Conn, in io.Reader, esc int, readOnly bool, detach func()) {
	buf := make([]byte, 4096)
	frame := make([]byte, 1, len(buf)+1)
	frame[0] = hub.FrameData
	for {
		n, err := in.Read(buf)
		if n > 0 {
			data := buf[:n]
			stop := false
			if esc >= 0 {
				if i := bytes.IndexByte(data, byte(esc)); i >= 0 {
					data, stop = data[:i], true
				}
			}
			if len(data) > 0 && !readOnly {
				if werr := conn.Write(ctx, websocket.MessageBinary, append(frame[:1], data...)); werr != nil {
					return
				}
			}
			if stop {
				detach()
				return
			}
		}
		if err != nil {
			detach()
			return
		}
	}
}

func forwardResize(ctx context.Context, conn *websocket.Conn, fd int) {
	sigs := resizeSignals(ctx)
	for {
		select {
		case <-ctx.Done():
			return
		case <-sigs:
		}
		cols, rows, err := term.GetSize(fd)
		if err != nil {
			continue
		}
		msg := fmt.Sprintf(`%c{"cols":%d,"rows":%d}`, hub.FrameResize, cols, rows)
		if conn.Write(ctx, websocket.MessageBinary, []byte(msg)) != nil {
			return
		}
	}
}
