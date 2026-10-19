package node

import (
	"context"
	"errors"
	"fmt"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/desk"
)

// Desk executes desktop commands the hub has already authorized (lease
// included) against this machine's desktop.
type Desk struct {
	D desk.Desktop
}

func deskErr(err error) error {
	switch {
	case errors.Is(err, desk.ErrNotInteractive):
		return connect.NewError(connect.CodeFailedPrecondition, err)
	case errors.Is(err, desk.ErrUnsupported):
		return connect.NewError(connect.CodeUnimplemented, err)
	case errors.Is(err, desk.ErrInvalid):
		return connect.NewError(connect.CodeInvalidArgument, err)
	}
	return connect.NewError(connect.CodeInternal, err)
}

// Do runs one command. Arguments are validated before the session is probed,
// so a malformed command is reported as such on any OS.
func (d *Desk) Do(_ context.Context, req *pb.DeskActionRequest) (*pb.DeskActionResponse, error) {
	switch a := req.Action.(type) {
	case *pb.DeskActionRequest_Screenshot:
		if err := d.D.Ready(); err != nil {
			return nil, deskErr(err)
		}
		img, err := d.D.Screenshot()
		if err != nil {
			return nil, deskErr(err)
		}
		b, err := desk.EncodePNG(img)
		if err != nil {
			return nil, deskErr(err)
		}
		r := img.Bounds()
		return &pb.DeskActionResponse{Result: &pb.DeskActionResponse_Screenshot{Screenshot: &pb.ScreenshotResponse{Png: b, Width: int32(r.Dx()), Height: int32(r.Dy())}}}, nil
	case *pb.DeskActionRequest_ListWindows:
		if err := d.D.Ready(); err != nil {
			return nil, deskErr(err)
		}
		ws, err := d.D.Windows()
		if err != nil {
			return nil, deskErr(err)
		}
		out := &pb.ListWindowsResponse{}
		for _, w := range ws {
			out.Windows = append(out.Windows, &pb.DeskWindow{Title: w.Title, X: int32(w.X), Y: int32(w.Y), Width: int32(w.Width), Height: int32(w.Height), Pid: w.PID, Minimized: w.Minimized})
		}
		return &pb.DeskActionResponse{Result: &pb.DeskActionResponse_ListWindows{ListWindows: out}}, nil
	case *pb.DeskActionRequest_Click:
		b, err := desk.ParseButton(a.Click.Button)
		if err != nil {
			return nil, deskErr(err)
		}
		return d.input(func() error { return d.D.Click(int(a.Click.X), int(a.Click.Y), b, a.Click.Double) })
	case *pb.DeskActionRequest_Move:
		return d.input(func() error { return d.D.Move(int(a.Move.X), int(a.Move.Y)) })
	case *pb.DeskActionRequest_Type:
		return d.input(func() error { return d.D.Type(a.Type.Text) })
	case *pb.DeskActionRequest_Key:
		c, err := desk.ParseChord(a.Key.Chord)
		if err != nil {
			return nil, deskErr(err)
		}
		return d.input(func() error { return d.D.Key(c) })
	case *pb.DeskActionRequest_Run:
		if a.Run.Program == "" {
			return nil, deskErr(fmt.Errorf("%w: program is empty", desk.ErrInvalid))
		}
		if err := d.D.Ready(); err != nil {
			return nil, deskErr(err)
		}
		pid, err := d.D.Run(a.Run.Program, a.Run.Args)
		if err != nil {
			return nil, deskErr(err)
		}
		return &pb.DeskActionResponse{Result: &pb.DeskActionResponse_Run{Run: &pb.RunResponse{Pid: pid}}}, nil
	}
	return nil, connect.NewError(connect.CodeInvalidArgument, errors.New("no desktop action set"))
}

func (d *Desk) input(do func() error) (*pb.DeskActionResponse, error) {
	if err := d.D.Ready(); err != nil {
		return nil, deskErr(err)
	}
	if err := do(); err != nil {
		return nil, deskErr(err)
	}
	return &pb.DeskActionResponse{Result: &pb.DeskActionResponse_Input{Input: &pb.InputResponse{}}}, nil
}
