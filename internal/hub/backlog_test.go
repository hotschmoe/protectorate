package hub

import (
	"context"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

func ptr[T any](v T) *T { return &v }

func TestNotesService(t *testing.T) {
	home := t.TempDir()
	t.Setenv("PROTECTORATE_HOME", home)
	_, srv := newTestHub(t)
	hc := &http.Client{Transport: bearer{testToken}}
	c := protectoratev1connect.NewBacklogServiceClient(hc, srv.URL)
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	ev, err := protectoratev1connect.NewEventServiceClient(hc, srv.URL).Subscribe(ctx, connect.NewRequest(&pb.SubscribeRequest{}))
	if err != nil {
		t.Fatal(err)
	}
	defer ev.Close()
	if !ev.Receive() {
		t.Fatal(ev.Err())
	}
	expect := func(t *testing.T, kind, id string) {
		t.Helper()
		for ev.Receive() {
			e := ev.Msg()
			if e.Kind == "heartbeat" {
				continue
			}
			if e.Kind != kind || e.Subject != id {
				t.Fatalf("event = %v, want %s %s", e, kind, id)
			}
			return
		}
		t.Fatalf("no %s event: %v", kind, ev.Err())
	}

	created, err := c.CreateNote(ctx, connect.NewRequest(&pb.CreateNoteRequest{Title: "buy cables", Body: "- usb-c\n- hdmi", Project: "me/repo"}))
	if err != nil {
		t.Fatal(err)
	}
	n := created.Msg.Note
	if n.Id == "" || n.Source != "owner" || n.Project != "me/repo" || n.CreatedAt == nil || n.Body != "- usb-c\n- hdmi" {
		t.Fatalf("created = %v", n)
	}
	expect(t, "note.created", n.Id)

	tests := []struct {
		name     string
		req      *pb.UpdateNoteRequest
		wantCode connect.Code
		wantKind string
		check    func(*pb.Note) bool
	}{
		{"unknown id", &pb.UpdateNoteRequest{Id: "ffff", Title: ptr("x")}, connect.CodeNotFound, "", nil},
		{"multi-line title", &pb.UpdateNoteRequest{Id: n.Id, Title: ptr("a\nb")}, connect.CodeInvalidArgument, "", nil},
		{"empties the note", &pb.UpdateNoteRequest{Id: n.Id, Title: ptr(""), Body: ptr("")}, connect.CodeInvalidArgument, "", nil},
		{"pin", &pb.UpdateNoteRequest{Id: n.Id, Pinned: ptr(true)}, 0, "note.updated", func(n *pb.Note) bool { return n.Pinned && n.Title == "buy cables" }},
		{"same again changes nothing", &pb.UpdateNoteRequest{Id: n.Id, Pinned: ptr(true)}, 0, "", func(n *pb.Note) bool { return n.Pinned }},
		{"body keeps title", &pb.UpdateNoteRequest{Id: n.Id, Body: ptr("done")}, 0, "note.updated", func(n *pb.Note) bool { return n.Body == "done" && n.Title == "buy cables" }},
		{"clear project", &pb.UpdateNoteRequest{Id: n.Id, Project: ptr("")}, 0, "note.updated", func(n *pb.Note) bool { return n.Project == "" }},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			res, err := c.UpdateNote(ctx, connect.NewRequest(tt.req))
			if tt.wantCode != 0 {
				if connect.CodeOf(err) != tt.wantCode {
					t.Fatalf("err = %v, want %v", err, tt.wantCode)
				}
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			if !tt.check(res.Msg.Note) {
				t.Fatalf("note = %v", res.Msg.Note)
			}
			if tt.wantKind != "" {
				expect(t, tt.wantKind, n.Id)
			}
		})
	}

	second, err := c.CreateNote(ctx, connect.NewRequest(&pb.CreateNoteRequest{Body: "no title, newer"}))
	if err != nil {
		t.Fatal(err)
	}
	expect(t, "note.created", second.Msg.Note.Id)
	list, err := c.ListNotes(ctx, connect.NewRequest(&pb.ListNotesRequest{}))
	if err != nil || len(list.Msg.Notes) != 2 || list.Msg.Notes[0].Id != n.Id {
		t.Fatalf("pinned note must lead: %v err=%v", list, err)
	}
	found, err := c.ListNotes(ctx, connect.NewRequest(&pb.ListNotesRequest{Query: "NEWER"}))
	if err != nil || len(found.Msg.Notes) != 1 || found.Msg.Notes[0].Id != second.Msg.Note.Id {
		t.Fatalf("query = %v err=%v", found, err)
	}
	got, err := c.GetNote(ctx, connect.NewRequest(&pb.GetNoteRequest{Id: n.Id}))
	if err != nil || got.Msg.Note.Body != "done" {
		t.Fatalf("get = %v err=%v", got, err)
	}

	if _, err := c.DeleteNote(ctx, connect.NewRequest(&pb.DeleteNoteRequest{Id: n.Id})); err != nil {
		t.Fatal(err)
	}
	expect(t, "note.deleted", n.Id)
	if _, err := c.GetNote(ctx, connect.NewRequest(&pb.GetNoteRequest{Id: n.Id})); connect.CodeOf(err) != connect.CodeNotFound {
		t.Fatalf("deleted note still there: %v", err)
	}
	if _, err := c.CreateNote(ctx, connect.NewRequest(&pb.CreateNoteRequest{})); connect.CodeOf(err) != connect.CodeInvalidArgument {
		t.Fatalf("empty note: %v", err)
	}
	b, err := os.ReadFile(filepath.Join(home, "notes.json"))
	if err != nil || strings.Contains(string(b), "buy cables") || !strings.Contains(string(b), "no title, newer") {
		t.Fatalf("file = %q err=%v", b, err)
	}
}
