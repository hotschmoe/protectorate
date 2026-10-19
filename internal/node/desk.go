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
	// Guard enforces the desk policy on every input command; nil allows all.
	Guard *DeskGuard
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
		return d.input(func() error { return d.typeText(a.Type.Text) })
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
		if err := d.admit(); err != nil {
			return nil, err
		}
		pid, err := d.D.Run(a.Run.Program, a.Run.Args)
		if err != nil {
			return nil, deskErr(err)
		}
		return &pb.DeskActionResponse{Result: &pb.DeskActionResponse_Run{Run: &pb.RunResponse{Pid: pid}}}, nil
	}
	return nil, connect.NewError(connect.CodeInvalidArgument, errors.New("no desktop action set"))
}

// admit is the check before anything that changes the desktop: the session
// can be driven and the policy, if any, allows agents right now.
func (d *Desk) admit() error {
	if err := d.D.Ready(); err != nil {
		return deskErr(err)
	}
	if d.Guard != nil {
		return d.Guard.Check()
	}
	return nil
}

// typeText sends text in short pieces, asking the guard before each so a user
// who comes back mid-sentence is not typed over.
func (d *Desk) typeText(text string) error {
	rs := []rune(text)
	for len(rs) > 0 {
		n := min(len(rs), typeChunk)
		if err := d.D.Type(string(rs[:n])); err != nil {
			return err
		}
		rs = rs[n:]
		if len(rs) > 0 && d.Guard != nil {
			if err := d.Guard.Check(); err != nil {
				return err
			}
		}
	}
	return nil
}

func (d *Desk) input(do func() error) (*pb.DeskActionResponse, error) {
	if err := d.admit(); err != nil {
		return nil, err
	}
	if err := do(); err != nil {
		var ce *connect.Error
		if errors.As(err, &ce) {
			return nil, err
		}
		return nil, deskErr(err)
	}
	return &pb.DeskActionResponse{Result: &pb.DeskActionResponse_Input{Input: &pb.InputResponse{}}}, nil
}
