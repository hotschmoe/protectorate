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

func TestBacklogService(t *testing.T) {
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

	add, err := c.AddItem(ctx, connect.NewRequest(&pb.AddItemRequest{Project: "me/repo", Title: "do it", Priority: 2, Harness: "codex"}))
	if err != nil {
		t.Fatal(err)
	}
	id := add.Msg.Item.Id
	if id == "" || add.Msg.Item.State != pb.ItemState_ITEM_STATE_TODO || add.Msg.Item.Priority != 2 {
		t.Fatalf("added = %v", add.Msg.Item)
	}

	expect := func(t *testing.T, kind string) {
		t.Helper()
		for ev.Receive() {
			e := ev.Msg()
			if e.Kind == "heartbeat" {
				continue
			}
			if e.Kind != kind || e.Subject != id {
				t.Fatalf("event = %v, want %s", e, kind)
			}
			return
		}
		t.Fatalf("no %s event: %v", kind, ev.Err())
	}
	expect(t, "backlog.added")

	tests := []struct {
		name     string
		id       string
		state    pb.ItemState
		by       string
		wantCode connect.Code
		wantKind string
	}{
		{"unspecified state", id, pb.ItemState_ITEM_STATE_UNSPECIFIED, "", connect.CodeInvalidArgument, ""},
		{"claim without by", id, pb.ItemState_ITEM_STATE_CLAIMED, "", connect.CodeInvalidArgument, ""},
		{"unknown id", "ffff", pb.ItemState_ITEM_STATE_DONE, "", connect.CodeUnknown, ""},
		{"claim", id, pb.ItemState_ITEM_STATE_CLAIMED, "alice/codex", 0, "backlog.claimed"},
		{"claim again same holder", id, pb.ItemState_ITEM_STATE_CLAIMED, "alice/codex", 0, ""},
		{"claim taken", id, pb.ItemState_ITEM_STATE_CLAIMED, "bob", connect.CodeFailedPrecondition, ""},
		{"release", id, pb.ItemState_ITEM_STATE_TODO, "", 0, "backlog.released"},
		{"done", id, pb.ItemState_ITEM_STATE_DONE, "alice", 0, "backlog.done"},
		{"reopen", id, pb.ItemState_ITEM_STATE_TODO, "", 0, "backlog.reopened"},
	}
	tests[2].wantCode = connect.CodeNotFound
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			req := &pb.UpdateItemRequest{Id: tt.id, State: tt.state}
			if tt.by != "" {
				req.Claim = &pb.Claim{By: tt.by}
			}
			res, err := c.UpdateItem(ctx, connect.NewRequest(req))
			if tt.wantCode != 0 {
				if connect.CodeOf(err) != tt.wantCode {
					t.Fatalf("err = %v, want %v", err, tt.wantCode)
				}
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			if res.Msg.Item.State != tt.state {
				t.Fatalf("state = %v", res.Msg.Item.State)
			}
			if tt.wantKind == "" {
				return
			}
			expect(t, tt.wantKind)
		})
	}

	list, err := c.ListItems(ctx, connect.NewRequest(&pb.ListItemsRequest{Project: "me/repo", State: pb.ItemState_ITEM_STATE_TODO, Harness: "codex"}))
	if err != nil || len(list.Msg.Items) != 1 || list.Msg.Items[0].Id != id {
		t.Fatalf("list = %v err=%v", list, err)
	}
	none, err := c.ListItems(ctx, connect.NewRequest(&pb.ListItemsRequest{Harness: "grok"}))
	if err != nil || len(none.Msg.Items) != 0 {
		t.Fatalf("grok should not see a codex item: %v err=%v", none, err)
	}

	_, err = c.AddItem(ctx, connect.NewRequest(&pb.AddItemRequest{Project: "donate: o/r", Title: "x"}))
	if connect.CodeOf(err) != connect.CodeInvalidArgument {
		t.Fatalf("donate without url: %v", err)
	}
	b, err := os.ReadFile(filepath.Join(home, "backlog.md"))
	if err != nil || !strings.HasPrefix(string(b), "## me/repo\n- [ ] do it  #p2 @codex id:"+id) {
		t.Fatalf("file = %q err=%v", b, err)
	}
}
