package main

import (
	"fmt"
	"io"
	"net/http"
	"os"
	"strconv"
	"strings"

	"connectrpc.com/connect"
	"github.com/spf13/cobra"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/desk"
	"github.com/hotschmoe/protectorate/internal/hub"
)

const deskLeaseEnv = "PROTECTORATE_DESK_LEASE"

func (o *opts) deskClient() (protectoratev1connect.DeskServiceClient, error) {
	hc, err := o.httpClient()
	if err != nil {
		return nil, err
	}
	return protectoratev1connect.NewDeskServiceClient(hc, o.hubURL), nil
}

type deskFlags struct {
	lease, button, output string
	double                bool
	maxWidth              int32
}

func deskCmd(o *opts) *cobra.Command {
	var f deskFlags
	c := &cobra.Command{
		Use:   "desk <node> screenshot|windows|foreground|click X Y|move X Y|type TEXT|key CHORD|run PROGRAM [ARGS]",
		Short: "Drive the interactive desktop of a Windows node",
		Long: `The desktop is the last resort. Prefer, in this order: the program's own API
or batch/CLI mode; protectorate job run or ssh on the node; protectorate node
put/get for files; only then screen and input, which are slow, flaky and can
land in the wrong window.

Screen and input commands for programs that only have a GUI. They work only
when the node agent runs in the logged-in user's desktop session
(protectorate node install, a logon task); in a service or SSH
session they fail with a message saying so.

screenshot, windows and foreground need no lease. click, move, type, key and run need the
node's desktop to be yours. Without --lease the hub takes a 2 minute lease for
you when the desktop is free, renews it on each further command from the same
token, and prints its id on stderr; when someone else holds it the command
fails naming the holder and the expiry. For a long session take an explicit one:

  protectorate lease acquire --node desk:<node> --ttl 30m
  protectorate desk <node> --lease <id> click 400 300

Coordinates are pixels of the screenshot. Write -- before a program's own
arguments or negative numbers: desk win1 --lease L run -- notepad.exe a.txt`,
		Args: cobra.MinimumNArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			if f.lease == "" {
				f.lease = os.Getenv(deskLeaseEnv)
			}
			act, err := parseDeskArgs(args[1], args[2:], f)
			if err != nil {
				return err
			}
			cl, err := o.deskClient()
			if err != nil {
				return err
			}
			return runDesk(cmd, o, cl, args[0], act, f)
		},
	}
	c.AddCommand(deskPolicyCmd(o))
	fl := c.Flags()
	fl.StringVar(&f.lease, "lease", "", "lease id on desk:<node> (default: the hub leases the desktop for 2 minutes per command; or env "+deskLeaseEnv+")")
	fl.StringVarP(&f.output, "output", "o", "", "screenshot file (default screenshot.png)")
	fl.Int32Var(&f.maxWidth, "max-width", 0, "scale the screenshot down to at most this many pixels wide (coordinates stay screen pixels)")
	fl.StringVar(&f.button, "button", "left", "mouse button for click: left, right or middle")
	fl.BoolVar(&f.double, "double", false, "double-click")
	return c
}

func deskInts(verb string, args []string) (int32, int32, error) {
	if len(args) != 2 {
		return 0, 0, fmt.Errorf("usage: desk <node> %s X Y", verb)
	}
	x, err := strconv.ParseInt(args[0], 10, 32)
	if err != nil {
		return 0, 0, fmt.Errorf("X %q is not an integer", args[0])
	}
	y, err := strconv.ParseInt(args[1], 10, 32)
	if err != nil {
		return 0, 0, fmt.Errorf("Y %q is not an integer", args[1])
	}
	return int32(x), int32(y), nil
}

// parseDeskArgs turns "<verb> args..." into the node-side action, checking
// what can be checked without the node.
func parseDeskArgs(verb string, args []string, f deskFlags) (*pb.DeskActionRequest, error) {
	switch verb {
	case "screenshot", "windows", "foreground":
		if len(args) != 0 {
			return nil, fmt.Errorf("desk %s takes no arguments", verb)
		}
		switch verb {
		case "windows":
			return &pb.DeskActionRequest{Action: &pb.DeskActionRequest_ListWindows{ListWindows: &pb.ListWindowsRequest{}}}, nil
		case "foreground":
			return &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Foreground{Foreground: &pb.ForegroundRequest{}}}, nil
		}
		return &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Screenshot{Screenshot: &pb.ScreenshotRequest{MaxWidth: f.maxWidth}}}, nil
	case "click":
		x, y, err := deskInts(verb, args)
		if err != nil {
			return nil, err
		}
		if _, err := desk.ParseButton(f.button); err != nil {
			return nil, err
		}
		return &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Click{Click: &pb.ClickRequest{X: x, Y: y, Button: f.button, Double: f.double}}}, nil
	case "move":
		x, y, err := deskInts(verb, args)
		if err != nil {
			return nil, err
		}
		return &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Move{Move: &pb.MoveRequest{X: x, Y: y}}}, nil
	case "type":
		if len(args) == 0 {
			return nil, fmt.Errorf("usage: desk <node> type TEXT")
		}
		return &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Type{Type: &pb.TypeRequest{Text: strings.Join(args, " ")}}}, nil
	case "key":
		if len(args) != 1 {
			return nil, fmt.Errorf("usage: desk <node> key CHORD (for example ctrl+s)")
		}
		if _, err := desk.ParseChord(args[0]); err != nil {
			return nil, err
		}
		return &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Key{Key: &pb.KeyRequest{Chord: args[0]}}}, nil
	case "run":
		if len(args) == 0 {
			return nil, fmt.Errorf("usage: desk <node> run PROGRAM [ARGS]")
		}
		return &pb.DeskActionRequest{Action: &pb.DeskActionRequest_Run{Run: &pb.RunRequest{Program: args[0], Args: args[1:]}}}, nil
	}
	return nil, fmt.Errorf("unknown desk command %q: want screenshot, windows, foreground, click, move, type, key or run", verb)
}

func runDesk(cmd *cobra.Command, o *opts, cl protectoratev1connect.DeskServiceClient, node string, act *pb.DeskActionRequest, f deskFlags) error {
	ctx := cmd.Context()
	t := &pb.DeskTarget{Name: node, LeaseId: f.lease}
	switch a := act.Action.(type) {
	case *pb.DeskActionRequest_Screenshot:
		a.Screenshot.Target = t
		res, err := cl.Screenshot(ctx, connect.NewRequest(a.Screenshot))
		if err != nil {
			return err
		}
		path := f.output
		if path == "" {
			path = "screenshot.png"
		}
		if err := os.WriteFile(path, res.Msg.Png, 0o644); err != nil {
			return fmt.Errorf("write screenshot: %w", err)
		}
		res.Msg.Png = nil
		return o.out(cmd).print(res.Msg, func(w io.Writer) {
			fmt.Fprintf(w, "screenshot\t%s\nsize\t%dx%d\n", path, res.Msg.Width, res.Msg.Height)
		})
	case *pb.DeskActionRequest_ListWindows:
		a.ListWindows.Target = t
		res, err := cl.ListWindows(ctx, connect.NewRequest(a.ListWindows))
		if err != nil {
			return err
		}
		return o.out(cmd).print(res.Msg, func(w io.Writer) {
			fmt.Fprintln(w, "PID\tX\tY\tWIDTH\tHEIGHT\tTITLE")
			for _, win := range res.Msg.Windows {
				title := win.Title
				if win.Minimized {
					title += " (minimized)"
				}
				fmt.Fprintf(w, "%d\t%d\t%d\t%d\t%d\t%s\n", win.Pid, win.X, win.Y, win.Width, win.Height, title)
			}
		})
	case *pb.DeskActionRequest_Foreground:
		a.Foreground.Target = t
		res, err := cl.Foreground(ctx, connect.NewRequest(a.Foreground))
		if err != nil {
			return err
		}
		return o.out(cmd).print(res.Msg, func(w io.Writer) {
			m := res.Msg
			if m.Pid == 0 {
				fmt.Fprintln(w, "foreground\tnone")
				return
			}
			fmt.Fprintf(w, "title\t%s\npid\t%d\nprocess\t%s\nintegrity\t%s\nagent\t%s\nblocks\t%t\n", m.Title, m.Pid, m.Process, m.Integrity, m.AgentIntegrity, m.Blocks)
		})
	case *pb.DeskActionRequest_Click:
		a.Click.Target = t
		return inputResult(cmd, o, "click")(cl.Click(ctx, connect.NewRequest(a.Click)))
	case *pb.DeskActionRequest_Move:
		a.Move.Target = t
		return inputResult(cmd, o, "move")(cl.Move(ctx, connect.NewRequest(a.Move)))
	case *pb.DeskActionRequest_Type:
		a.Type.Target = t
		return inputResult(cmd, o, "type")(cl.Type(ctx, connect.NewRequest(a.Type)))
	case *pb.DeskActionRequest_Key:
		a.Key.Target = t
		return inputResult(cmd, o, "key")(cl.Key(ctx, connect.NewRequest(a.Key)))
	case *pb.DeskActionRequest_Run:
		a.Run.Target = t
		res, err := cl.Run(ctx, connect.NewRequest(a.Run))
		if err != nil {
			return err
		}
		noteAutoLease(cmd, res.Header())
		return o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprintf(w, "started\tpid %d\n", res.Msg.Pid) })
	}
	return nil
}

// noteAutoLease tells the user about a lease the hub took on their behalf.
func noteAutoLease(cmd *cobra.Command, h http.Header) {
	id := h.Get(hub.DeskLeaseHeader)
	if id == "" {
		return
	}
	fmt.Fprintf(cmd.ErrOrStderr(), "desk lease %s (auto, renewed by each desk command, expires %s)\n", id, h.Get(hub.DeskLeaseExpiresHeader))
}

func inputResult(cmd *cobra.Command, o *opts, verb string) func(*connect.Response[pb.InputResponse], error) error {
	return func(res *connect.Response[pb.InputResponse], err error) error {
		if err != nil {
			return err
		}
		noteAutoLease(cmd, res.Header())
		return o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprintf(w, "%s\tok\n", verb) })
	}
}
