package hub

import (
	"context"
	"errors"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/structpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/backlog"
)

// SetBacklog installs the notes store. Call before Handler or Serve.
func (h *Hub) SetBacklog(s *backlog.Store) { h.backlog = s }

func (h *Hub) ListNotes(_ context.Context, req *connect.Request[pb.ListNotesRequest]) (*connect.Response[pb.ListNotesResponse], error) {
	notes, err := h.backlog.List(backlog.Filter{Project: req.Msg.Project, Query: req.Msg.Query, State: req.Msg.State})
	if err != nil {
		return nil, backlogError(err)
	}
	res := &pb.ListNotesResponse{Notes: make([]*pb.Note, len(notes))}
	for i, n := range notes {
		res.Notes[i] = toProtoNote(n)
	}
	return connect.NewResponse(res), nil
}

func (h *Hub) GetNote(_ context.Context, req *connect.Request[pb.GetNoteRequest]) (*connect.Response[pb.GetNoteResponse], error) {
	n, err := h.backlog.Get(req.Msg.Id)
	if err != nil {
		return nil, backlogError(err)
	}
	return connect.NewResponse(&pb.GetNoteResponse{Note: toProtoNote(n)}), nil
}

func (h *Hub) CreateNote(ctx context.Context, req *connect.Request[pb.CreateNoteRequest]) (*connect.Response[pb.CreateNoteResponse], error) {
	m := req.Msg
	n, err := h.backlog.Create(backlog.NewNote{
		Title: m.Title, Body: m.Body, Pinned: m.Pinned, Project: m.Project,
		Source: senderOf(auth.PrincipalFrom(ctx)),
	})
	if err != nil {
		return nil, backlogError(err)
	}
	h.publishNote("note.created", n)
	return connect.NewResponse(&pb.CreateNoteResponse{Note: toProtoNote(n)}), nil
}

func (h *Hub) UpdateNote(ctx context.Context, req *connect.Request[pb.UpdateNoteRequest]) (*connect.Response[pb.UpdateNoteResponse], error) {
	m := req.Msg
	n, changed, err := h.backlog.Update(m.Id, backlog.Patch{
		Title: m.Title, Body: m.Body, Pinned: m.Pinned, Project: m.Project,
		State: m.State, Resolution: m.Resolution, DoneBy: actor(ctx),
	})
	if err != nil {
		return nil, backlogError(err)
	}
	if changed {
		h.publishNote("note.updated", n)
	}
	return connect.NewResponse(&pb.UpdateNoteResponse{Note: toProtoNote(n)}), nil
}

func (h *Hub) DeleteNote(_ context.Context, req *connect.Request[pb.DeleteNoteRequest]) (*connect.Response[pb.DeleteNoteResponse], error) {
	n, err := h.backlog.Delete(req.Msg.Id)
	if err != nil {
		return nil, backlogError(err)
	}
	h.publishNote("note.deleted", n)
	return connect.NewResponse(&pb.DeleteNoteResponse{}), nil
}

// fileNote creates a note on the hub's own initiative and announces it.
func (h *Hub) fileNote(n backlog.NewNote) (backlog.Note, error) {
	note, err := h.backlog.Create(n)
	if err != nil {
		return backlog.Note{}, err
	}
	h.publishNote("note.created", note)
	return note, nil
}

func (h *Hub) publishNote(kind string, n backlog.Note) {
	payload, err := structpb.NewStruct(map[string]any{
		"id": n.ID, "title": n.Title, "project": n.Project, "source": n.Source, "pinned": n.Pinned, "state": n.State,
	})
	if err != nil {
		return
	}
	h.emit(kind, n.ID, payload)
}

func backlogError(err error) error {
	code := connect.CodeInternal
	switch {
	case errors.Is(err, backlog.ErrNotFound):
		code = connect.CodeNotFound
	case errors.Is(err, backlog.ErrInvalid):
		code = connect.CodeInvalidArgument
	}
	return connect.NewError(code, err)
}

func toProtoNote(n backlog.Note) *pb.Note {
	out := &pb.Note{
		Id: n.ID, Title: n.Title, Body: n.Body, Pinned: n.Pinned, Project: n.Project, Source: n.Source,
		CreatedAt: timestamppb.New(n.Created), UpdatedAt: timestamppb.New(n.Updated),
		State: n.State,
	}
	if n.DoneAt != nil {
		out.Completion = &pb.NoteCompletion{DoneAt: timestamppb.New(*n.DoneAt), DoneBy: n.DoneBy, Resolution: n.Resolution}
	}
	return out
}
