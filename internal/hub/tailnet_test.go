package hub

import (
	"context"
	"errors"
	"net/http"
	"testing"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

func TestTailnetGetStatus(t *testing.T) {
	tests := []struct {
		name     string
		source   func(context.Context) (*pb.TailnetServiceGetStatusResponse, error)
		wantCode connect.Code
		wantPeer string
	}{
		{"disabled", nil, connect.CodeFailedPrecondition, ""},
		{"failing", func(context.Context) (*pb.TailnetServiceGetStatusResponse, error) { return nil, errors.New("boom") }, connect.CodeUnavailable, ""},
		{"ok", func(context.Context) (*pb.TailnetServiceGetStatusResponse, error) {
			return &pb.TailnetServiceGetStatusResponse{Peers: []*pb.TailnetPeer{{Name: "alpha"}}}, nil
		}, 0, "alpha"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			h, srv := newTestHub(t)
			if tt.source != nil {
				h.SetTailnetStatus(tt.source)
			}
			c := protectoratev1connect.NewTailnetServiceClient(&http.Client{Transport: bearer{testToken}}, srv.URL)
			res, err := c.GetStatus(context.Background(), connect.NewRequest(&pb.TailnetServiceGetStatusRequest{}))
			if tt.wantCode != 0 {
				if connect.CodeOf(err) != tt.wantCode {
					t.Fatalf("code %v, want %v (err %v)", connect.CodeOf(err), tt.wantCode, err)
				}
				return
			}
			if err != nil || res.Msg.Peers[0].Name != tt.wantPeer {
				t.Fatalf("res %v err %v", res, err)
			}
		})
	}
}
